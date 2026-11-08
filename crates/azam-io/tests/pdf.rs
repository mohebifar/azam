//! Compares the drawing operations of the pattern and tech-pack PDFs with recorded reference data,
//! checks that the written files are well-formed PDF, and that text outside `WinAnsi` (the Persian
//! names of the `persian` project, a tee named in Persian) is embedded, shaped and extractable.

mod common;

use std::collections::BTreeMap;

use azam_doc::Project;
use azam_io::pdf::canvas::{Canvas, Op, Page, Paint, Stroke, TextOpts};
use azam_io::pdf::metrics::{FontStyle, PT_PER_MM, text_width};
use azam_io::pdf::needs_unicode_font;
use azam_io::pdf::pattern::pattern_pdf_pages;
use azam_io::pdf::writer::{PdfInfo, write_pdf};
use azam_io::{TechPackInfo, pattern_pdf, techpack_pdf, techpack_pdf_pages};
use common::{diff, golden, pdf_options, project_named};
use serde_json::{Value, json};

/// Recorded coordinates are rounded to 1e-6 mm.
const TOL: f64 = 2e-6;

/// Rounds like the recorder, so digests compare at the same precision.
fn r6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

fn stroke(s: &Stroke) -> Vec<Value> {
    vec![json!(s.width), json!(s.color), json!([s.dash, s.phase])]
}

fn paint(p: &Paint) -> Vec<Value> {
    match p {
        Paint::Stroke(s) => stroke(s),
        Paint::Fill(c) => vec![json!(c)],
        Paint::FillStroke(c, s) => {
            let mut v = stroke(s);
            v.push(json!(c));
            v
        }
    }
}

/// A path as the recorder digests it: vertex count, end points, vertex sums and bounds.
fn digest(points: &[[f64; 2]]) -> Value {
    let n = points.len();
    let (mut sx, mut sy) = (0.0, 0.0);
    let (mut x0, mut y0, mut x1, mut y1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        sx += p[0];
        sy += p[1];
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    let (f, l) = (points[0], points[n - 1]);
    json!([n, f[0], f[1], l[0], l[1], r6(sx), r6(sy), x0, y0, x1, y1])
}

/// An operation in the recorded shape.
fn op_json(op: &Op) -> Value {
    let mut v: Vec<Value> = match op {
        Op::Text {
            text,
            x,
            y,
            align,
            baseline,
            angle,
            style,
            size,
            color,
        } => vec![
            json!("text"),
            json!(text),
            json!(x),
            json!(y),
            json!(align),
            json!(baseline),
            json!(angle),
            json!(style),
            json!(size),
            json!(color),
        ],
        Op::Line { a, b, stroke: s } => {
            let mut v = vec![json!("line"), json!(a[0]), json!(a[1]), json!(b[0]), json!(b[1])];
            v.extend(stroke(s));
            v
        }
        Op::Path {
            points,
            closed,
            paint: p,
        } => vec![json!("path"), digest(points), json!(closed), json!(p.code())],
        Op::Rect { x, y, w, h, paint: p } => vec![
            json!("rect"),
            json!(x),
            json!(y),
            json!(w),
            json!(h),
            json!(p.code()),
        ],
        Op::Circle { x, y, r, paint: p } => {
            vec![json!("circle"), json!(x), json!(y), json!(r), json!(p.code())]
        }
        Op::Triangle { points, paint: p } => {
            let flat: Vec<f64> = points.iter().flatten().copied().collect();
            vec![json!("triangle"), json!(flat), json!(p.code())]
        }
        Op::Save => vec![json!("save")],
        Op::Restore => vec![json!("restore")],
        Op::Clip { x, y, w, h } => vec![json!("clip"), json!(x), json!(y), json!(w), json!(h)],
    };
    match op {
        Op::Path { paint: p, .. }
        | Op::Rect { paint: p, .. }
        | Op::Circle { paint: p, .. }
        | Op::Triangle { paint: p, .. } => {
            v.extend(paint(p));
        }
        _ => {}
    }
    Value::Array(v)
}

fn compare_pages(expected: &Value, pages: &[Page], what: &str) {
    let exp = expected.as_array().expect("pages");
    assert_eq!(exp.len(), pages.len(), "{what}: page count");
    for (pi, (e, a)) in exp.iter().zip(pages).enumerate() {
        let size = json!({ "w": a.width, "h": a.height });
        if let Some(d) = diff(
            &json!({ "w": e["w"], "h": e["h"] }),
            &size,
            TOL,
            &format!("{what} page {}", pi + 1),
        ) {
            panic!("{d}");
        }
        let ops = e["ops"].as_array().expect("ops");
        for (oi, (eo, ao)) in ops.iter().zip(&a.ops).enumerate() {
            if let Some(d) = diff(eo, &op_json(ao), TOL, &format!("{what} page {} op {oi}", pi + 1)) {
                panic!("{d}\n expected {eo}\n got      {}", op_json(ao));
            }
        }
        assert_eq!(ops.len(), a.ops.len(), "{what} page {}: operation count", pi + 1);
    }
}

/// Helvetica's default glyph width in em, which the recorded reference gave every character the
/// base-14 metrics lack.
const REFERENCE_DEFAULT_WIDTH_EM: f64 = 0.53;
/// Face and size of the project name in the pattern page header.
const HEADER_NAME_STYLE: (FontStyle, f64) = (FontStyle::Normal, 8.5);
/// How the header line after the project name starts.
const HEADER_LINE_START: &str = "·   size ";

/// The recorded pattern pages with the one layout that depends on measuring text outside `WinAnsi`
/// brought up to date, and the number of operations changed.
///
/// The reference drew such text (the Persian names of the `persian` project) with the
/// base-14 fonts and so measured it with Helvetica's default glyph width, 0.53 em per UTF-16
/// unit. The writer now draws it with an embedded Unicode font and measures it with that font's
/// shaped advances. In the recorded cases the only drawing placed by such a measurement is the
/// page header, where the size and date line starts 3 mm after the project name: it moves by the
/// difference of the two widths of the name. Everything else must still match the recording
/// exactly, and projects with `WinAnsi` names are compared unchanged.
fn with_unicode_metrics(expected: &Value, project: &Project) -> (Value, usize) {
    let name = &project.meta.name;
    let mut out = expected.clone();
    if !needs_unicode_font(name) {
        return (out, 0);
    }
    let (style, size) = HEADER_NAME_STYLE;
    let reference = name.encode_utf16().count() as f64 * REFERENCE_DEFAULT_WIDTH_EM * size / PT_PER_MM;
    let shift = text_width(name, style, size) - reference;
    let mut changed = 0;
    for page in out.as_array_mut().expect("pages") {
        for op in page["ops"].as_array_mut().expect("ops") {
            if op[0] == "text" && op[1].as_str().is_some_and(|t| t.starts_with(HEADER_LINE_START)) {
                op[2] = json!(op[2].as_f64().expect("x") + shift);
                changed += 1;
            }
        }
    }
    (out, changed)
}

#[test]
fn pattern_pdf_operations_match() {
    for case in golden()["patternPdfs"].as_array().expect("pattern pdfs") {
        let name = case["project"].as_str().expect("project");
        let project = project_named(name);
        let o = pdf_options(&case["options"]);
        let pages = pattern_pdf_pages(&project, &o).expect("pieces to print");
        let (expected, changed) = with_unicode_metrics(&case["pages"], &project);
        assert_eq!(
            changed > 0,
            name == "persian",
            "{name}: header operations measured with the Unicode font"
        );
        compare_pages(&expected, &pages, &format!("{name} {}", case["options"]));
    }
}

fn info(v: &Value) -> TechPackInfo {
    let s = |k: &str| v[k].as_str().unwrap_or("").to_owned();
    TechPackInfo {
        season: s("season"),
        designer: s("designer"),
        currency: s("currency"),
        notes: s("notes"),
        date: common::date(),
    }
}

#[test]
fn techpack_pdf_operations_match() {
    for case in golden()["techPdfs"].as_array().expect("tech pdfs") {
        let name = case["project"].as_str().expect("project");
        let project = project_named(name);
        let pages = techpack_pdf_pages(&project, &info(&case["info"]));
        compare_pages(&case["pages"], &pages, &format!("{name} tech pack"));
    }
}

/// Checks the file structure: header, objects, cross-reference table pointing at every object,
/// trailer, and the page count. Offsets are byte offsets, so the checks work on bytes.
fn assert_valid_pdf(bytes: &[u8], pages: usize, what: &str) {
    let find = |pat: &[u8]| bytes.windows(pat.len()).rposition(|w| w == pat);
    let count_of = |pat: &[u8]| bytes.windows(pat.len()).filter(|w| *w == pat).count();
    let at = |off: usize, pat: &[u8]| bytes.get(off..).is_some_and(|b| b.starts_with(pat));
    let line = |off: usize| -> String {
        let end = bytes[off..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |e| off + e);
        String::from_utf8_lossy(&bytes[off..end]).into_owned()
    };
    assert!(bytes.starts_with(b"%PDF-1."), "{what}: header");
    assert!(bytes.ends_with(b"%%EOF\n"), "{what}: end marker");
    let sx = find(b"startxref\n").expect("startxref");
    let xref: usize = line(sx + 10).trim().parse().expect("offset number");
    assert!(at(xref, b"xref\n"), "{what}: startxref points at the xref table");
    let header = line(xref + 5);
    let count: usize = header
        .split(' ')
        .nth(1)
        .expect("count")
        .parse()
        .expect("count number");
    let first = xref + 5 + header.len() + 1;
    for i in 0..count {
        let entry = line(first + 20 * i);
        assert_eq!(entry.len(), 19, "{what}: xref entry {i} is 20 bytes");
        if i == 0 {
            assert_eq!(entry, "0000000000 65535 f ", "{what}: free entry");
            continue;
        }
        let off: usize = entry[..10].parse().expect("object offset");
        assert!(
            at(off, format!("{i} 0 obj\n").as_bytes()),
            "{what}: object {i} offset"
        );
    }
    assert!(
        at(
            first + 20 * count,
            format!("trailer\n<< /Size {count} /Root 1 0 R").as_bytes()
        ),
        "{what}: trailer"
    );
    assert!(
        find(format!("/Count {pages} >>").as_bytes()).is_some(),
        "{what}: page count"
    );
    assert_eq!(count_of(b"/Type /Page /Parent"), pages, "{what}: page objects");
    assert!(find(b"/Producer (azam)").is_some(), "{what}: producer");

    // Every stream has its stated length and inflates; content streams (and ToUnicode CMaps) are
    // ASCII drawing operators, font files are binary.
    let mut streams = 0;
    let mut contents = 0;
    let mut from = 0;
    while let Some(rel) = bytes[from..].windows(8).position(|w| w == b"/Length ") {
        let at_len = from + rel + 8;
        let len: usize = line(at_len)
            .split(' ')
            .next()
            .expect("length")
            .parse()
            .expect("length number");
        let data = at_len
            + bytes[at_len..]
                .windows(7)
                .position(|w| w == b"stream\n")
                .expect("stream")
            + 7;
        assert!(at(data + len, b"\nendstream"), "{what}: stream {streams} length");
        let content =
            miniz_oxide::inflate::decompress_to_vec_zlib(&bytes[data..data + len]).expect("stream inflates");
        let dict_line = line(at_len);
        if !dict_line.contains("/Length1 ") && !dict_line.contains("/Subtype /OpenType") {
            let content = String::from_utf8(content).expect("content is ASCII");
            assert!(
                content.lines().all(|l| l.is_empty() || l.is_ascii()),
                "{what}: stream {streams} text"
            );
            if !content.starts_with("/CIDInit") {
                contents += 1;
            }
        }
        streams += 1;
        from = data + len;
    }
    assert_eq!(contents, pages, "{what}: one content stream per page");
    assert_eq!(
        streams,
        pages + 2 * count_of(b"/Subtype /Type0"),
        "{what}: a font file and a ToUnicode CMap per embedded font"
    );
}

#[test]
fn written_pdfs_are_well_formed() {
    for name in ["tee", "persian", "sampler"] {
        let project = project_named(name);
        for case in golden()["patternPdfs"]
            .as_array()
            .expect("pattern pdfs")
            .iter()
            .filter(|c| c["project"] == name)
        {
            let o = pdf_options(&case["options"]);
            let bytes = pattern_pdf(&project, &o).expect("pdf");
            let pages = case["pages"].as_array().expect("pages").len();
            assert_valid_pdf(&bytes, pages, &format!("{name} {}", case["options"]));
        }
        let bytes = techpack_pdf(&project, &TechPackInfo::default());
        assert_valid_pdf(&bytes, 5, &format!("{name} tech pack"));
    }
}

#[test]
fn empty_project_has_no_pattern_pdf() {
    let project = project_named("blank");
    assert_eq!(
        pattern_pdf(&project, &azam_io::PatternPdfOptions::default()),
        Err(azam_io::PatternPdfError::NoPieces)
    );
}

/// The objects of a PDF file by number, found through the cross-reference table.
fn pdf_objects(bytes: &[u8]) -> BTreeMap<usize, &[u8]> {
    let sx = bytes
        .windows(10)
        .rposition(|w| w == b"startxref\n")
        .expect("startxref");
    let tail = std::str::from_utf8(&bytes[sx + 10..]).expect("ASCII trailer");
    let xref: usize = tail
        .lines()
        .next()
        .expect("offset")
        .parse()
        .expect("offset number");
    let table = std::str::from_utf8(&bytes[xref..sx]).expect("ASCII xref table");
    let mut lines = table.lines().skip(1);
    let count: usize = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .expect("count")
        .parse()
        .expect("count number");
    lines
        .take(count)
        .enumerate()
        .skip(1)
        .map(|(i, entry)| {
            let off: usize = entry[..10].parse().expect("object offset");
            let start = off + format!("{i} 0 obj\n").len();
            let len = bytes[start..]
                .windows(8)
                .position(|w| w == b"\nendobj\n")
                .expect("endobj");
            (i, &bytes[start..start + len])
        })
        .collect()
}

/// The dictionary of an object (its text up to the stream data).
fn dict(obj: &[u8]) -> String {
    let end = obj.windows(7).position(|w| w == b"stream\n").unwrap_or(obj.len());
    String::from_utf8_lossy(&obj[..end]).into_owned()
}

/// The object number `key N 0 R` refers to in `dict`.
fn reference(dict: &str, key: &str) -> Option<usize> {
    let at = dict.find(&format!("{key} "))? + key.len() + 1;
    dict[at..].trim_start_matches('[').split(' ').next()?.parse().ok()
}

/// The inflated data of a stream object.
fn stream_data(obj: &[u8]) -> Vec<u8> {
    let d = dict(obj);
    let len: usize = d[d.find("/Length ").expect("length") + 8..]
        .split(' ')
        .next()
        .expect("length")
        .parse()
        .expect("length number");
    let start = d.len() + 7;
    miniz_oxide::inflate::decompress_to_vec_zlib(&obj[start..start + len]).expect("stream inflates")
}

/// The `bfchar` entries of a `ToUnicode` `CMap`.
fn parse_cmap(cmap: &str) -> BTreeMap<u16, String> {
    let mut map = BTreeMap::new();
    let mut inside = false;
    for line in cmap.lines() {
        if line.ends_with("beginbfchar") {
            inside = true;
        } else if line == "endbfchar" {
            inside = false;
        } else if inside {
            let parts: Vec<&str> = line.split(' ').map(|p| p.trim_matches(['<', '>'])).collect();
            let cid = u16::from_str_radix(parts[0], 16).expect("cid");
            let units: Vec<u16> = (0..parts[1].len() / 4)
                .map(|i| u16::from_str_radix(&parts[1][4 * i..4 * i + 4], 16).expect("unit"))
                .collect();
            map.insert(cid, String::from_utf16(&units).expect("utf-16"));
        }
    }
    map
}

/// A piece of drawn text in visual order.
#[derive(Debug)]
enum Drawn {
    /// A base-14 string, decoded.
    Base(String),
    /// Embedded-font glyphs, mapped back through the font's `ToUnicode` `CMap`.
    Glyphs(Vec<String>),
}

/// Decodes a PDF literal string (`WinAnsi` printable ASCII and octal escapes) at the start of `s`;
/// returns it and the rest.
fn literal(s: &str) -> (String, &str) {
    let mut out = String::new();
    let mut chars = s.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            ')' => return (out, &s[i + 1..]),
            '\\' => {
                let (_, e) = chars.next().expect("escape");
                if e.is_ascii_digit() {
                    let oct: String = std::iter::once(e)
                        .chain((0..2).filter_map(|_| chars.next().map(|x| x.1)))
                        .collect();
                    out.push(char::from(u8::from_str_radix(&oct, 8).expect("octal")));
                } else {
                    out.push(e);
                }
            }
            _ => out.push(c),
        }
    }
    (out, "")
}

/// The text objects of every page, as drawn pieces per `BT … ET`.
fn drawn_text(bytes: &[u8]) -> Vec<Vec<Drawn>> {
    let objects = pdf_objects(bytes);
    let mut out = Vec::new();
    for obj in objects
        .values()
        .filter(|o| dict(o).starts_with("<< /Type /Page /Parent"))
    {
        let page = dict(obj);
        // Resource name to ToUnicode map, for the embedded fonts.
        let fonts = &page[page.find("/Font << ").expect("fonts") + 9..];
        let fonts = &fonts[..fonts.find(" >>").expect("end of fonts")];
        let mut cmaps: BTreeMap<String, BTreeMap<u16, String>> = BTreeMap::new();
        let words: Vec<&str> = fonts.split(' ').collect();
        for w in words.chunks(4) {
            let font = dict(objects[&w[1].parse::<usize>().expect("font object")]);
            if let Some(tu) = reference(&font, "/ToUnicode") {
                let cmap = String::from_utf8(stream_data(objects[&tu])).expect("cmap text");
                cmaps.insert(w[0].trim_start_matches('/').to_owned(), parse_cmap(&cmap));
            }
        }
        let contents = reference(&page, "/Contents").expect("contents");
        let content = String::from_utf8(stream_data(objects[&contents])).expect("content");
        for line in content.lines().filter(|l| l.starts_with("BT ")) {
            let mut pieces = Vec::new();
            let mut font = String::new();
            let mut rest = line;
            while !rest.is_empty() {
                rest = rest.trim_start();
                if let Some(r) = rest.strip_prefix('/') {
                    let end = r.find(' ').unwrap_or(r.len());
                    r[..end].clone_into(&mut font);
                    rest = &r[end..];
                } else if rest.starts_with('(') {
                    let (s, r) = literal(rest);
                    pieces.push(Drawn::Base(s));
                    rest = r;
                } else if let Some(r) = rest.strip_prefix('[') {
                    let end = r.find(']').expect("end of array");
                    let cmap = &cmaps[&font];
                    let mut glyphs = Vec::new();
                    for hex in r[..end].split('<').skip(1) {
                        let hex = &hex[..hex.find('>').expect("end of hex")];
                        for i in 0..hex.len() / 4 {
                            let cid = u16::from_str_radix(&hex[4 * i..4 * i + 4], 16).expect("cid");
                            glyphs.push(cmap.get(&cid).cloned().unwrap_or_default());
                        }
                    }
                    pieces.push(Drawn::Glyphs(glyphs));
                    rest = &r[end + 1..];
                } else {
                    rest = rest.find(' ').map_or("", |i| &rest[i..]);
                }
            }
            out.push(pieces);
        }
    }
    out
}

/// The text of each text object drawn only with embedded glyphs, in logical order: the glyphs of
/// right-to-left text are drawn right to left, so they read back reversed.
fn right_to_left_texts(bytes: &[u8]) -> Vec<String> {
    drawn_text(bytes)
        .iter()
        .filter(|pieces| pieces.iter().all(|p| matches!(p, Drawn::Glyphs(_))))
        .map(|pieces| {
            let glyphs: Vec<&String> = pieces
                .iter()
                .flat_map(|p| match p {
                    Drawn::Glyphs(g) => g.iter(),
                    Drawn::Base(_) => [].iter(),
                })
                .collect();
            glyphs.into_iter().rev().map(String::as_str).collect()
        })
        .collect()
}

/// The Persian tee's pattern (one piece per page, with dimensions) and tech pack.
fn persian_pdfs() -> (Project, Vec<u8>, Vec<u8>) {
    let project = project_named("persian");
    let case = golden()["patternPdfs"]
        .as_array()
        .expect("pattern pdfs")
        .iter()
        .find(|c| c["project"] == "persian" && c["options"]["layout"] == "pieces")
        .expect("persian pieces case");
    let pattern = pattern_pdf(&project, &pdf_options(&case["options"])).expect("pdf");
    let tech = techpack_pdf(&project, &TechPackInfo::default());
    (project, pattern, tech)
}

#[test]
fn persian_text_uses_embedded_subset_fonts() {
    let (_, pattern, tech) = persian_pdfs();
    for (what, bytes) in [("pattern", &pattern), ("tech pack", &tech)] {
        let objects = pdf_objects(bytes);
        let type0: Vec<String> = objects
            .values()
            .map(|o| dict(o))
            .filter(|d| d.contains("/Subtype /Type0"))
            .collect();
        // Regular and bold Vazirmatn, and (in the tech pack) bold Noto Sans for the Greek Δ of
        // the grading column.
        let expected = if what == "pattern" { 2 } else { 3 };
        assert_eq!(type0.len(), expected, "{what}: embedded fonts");
        for t in &type0 {
            assert!(t.contains("/Encoding /Identity-H"), "{what}: {t}");
            let base = &t[t.find("/BaseFont /").expect("base font") + 11..];
            let base = &base[..base.find(' ').expect("name end")];
            let (tag, name) = base.split_once('+').expect("subset tag");
            assert!(
                tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()),
                "{what}: {base}"
            );
            assert!(
                name.starts_with("Vazirmatn-") || name == "NotoSans-Bold",
                "{what}: {base}"
            );
            let cid = dict(objects[&reference(t, "/DescendantFonts").expect("descendant")]);
            assert!(cid.contains("/Subtype /CIDFontType2"), "{what}: {cid}");
            assert!(
                cid.contains("/CIDToGIDMap /Identity") && cid.contains("/W [1 ["),
                "{what}: {cid}"
            );
            let descriptor = dict(objects[&reference(&cid, "/FontDescriptor").expect("descriptor")]);
            let file = objects[&reference(&descriptor, "/FontFile2").expect("font file")];
            let data = stream_data(file);
            let stated: usize = reference(&dict(file), "/Length1").expect("length1");
            assert_eq!(data.len(), stated, "{what}: /Length1");
            // A TrueType subset: far smaller than the 120 KB font, and parseable.
            assert!(data.starts_with(&[0, 1, 0, 0]), "{what}: sfnt version");
            assert!(data.len() < 40_000, "{what}: subset of {} bytes", data.len());
            assert!(reference(t, "/ToUnicode").is_some(), "{what}: ToUnicode");
        }
    }
}

#[test]
fn persian_names_map_back_through_to_unicode() {
    let (project, pattern, tech) = persian_pdfs();
    let pattern_texts = right_to_left_texts(&pattern);
    let mut names: Vec<&str> = project.pieces.values().map(|p| p.name.as_str()).collect();
    names.push(&project.meta.name);
    for name in &names {
        assert!(
            pattern_texts.iter().any(|t| t == name),
            "pattern: {name:?} not found in {pattern_texts:?}"
        );
    }
    // Fabric names in the tech pack, including one with a zero-width non-joiner.
    let tech_texts = right_to_left_texts(&tech);
    for fabric in project.fabrics.values() {
        assert!(
            tech_texts.iter().any(|t| t == &fabric.name),
            "tech pack: {:?} not found in {tech_texts:?}",
            fabric.name
        );
    }
    assert!(tech_texts.iter().any(|t| t == "سرمه\u{200c}ای"));
}

#[test]
fn persian_names_are_not_replaced_by_question_marks() {
    let (_, pattern, tech) = persian_pdfs();
    for (what, bytes) in [("pattern", &pattern), ("tech pack", &tech)] {
        for pieces in drawn_text(bytes) {
            for p in pieces {
                match p {
                    Drawn::Base(s) => assert!(!s.contains('?'), "{what}: {s:?}"),
                    Drawn::Glyphs(g) => assert!(g.iter().all(|t| !t.contains('?')), "{what}: {g:?}"),
                }
            }
        }
    }
}

/// The text of a text object read left to right (base strings and glyph texts in drawing
/// order), for left-to-right text.
fn left_to_right_text(pieces: &[Drawn]) -> String {
    pieces
        .iter()
        .map(|p| match p {
            Drawn::Base(s) => s.clone(),
            Drawn::Glyphs(g) => g.concat(),
        })
        .collect()
}

/// The `/BaseFont` names (without subset tags) of the embedded fonts of a PDF.
fn embedded_fonts(bytes: &[u8]) -> Vec<String> {
    pdf_objects(bytes)
        .values()
        .map(|o| dict(o))
        .filter(|d| d.contains("/Subtype /Type0"))
        .map(|d| {
            let base = &d[d.find("/BaseFont /").expect("base font") + 11..];
            let base = &base[..base.find(' ').expect("name end")];
            base.split_once('+').map_or(base, |(_, n)| n).to_owned()
        })
        .collect()
}

/// Latin projects stay on the base-14 path, byte for byte as before, wherever their text is all
/// `WinAnsi`. Their only other text is the Greek Δ heading the grading columns (of the tech pack,
/// and of the pattern schedules): it used to print as `?` and is now drawn, deliberately, with
/// embedded Noto Sans, so those files gain one font for that glyph and nothing else changes.
#[test]
fn latin_projects_embed_fonts_only_for_greek() {
    let check = |bytes: &[u8], ascii_only: bool, what: &str| {
        for pieces in drawn_text(bytes) {
            if let [Drawn::Base(s)] = &pieces[..] {
                assert!(!s.contains('?'), "{what}: {s:?}");
                continue;
            }
            assert!(!ascii_only, "{what}: {pieces:?}");
            assert!(left_to_right_text(&pieces).starts_with('Δ'), "{what}: {pieces:?}");
            for p in &pieces {
                if let Drawn::Glyphs(g) = p {
                    assert_eq!(g.concat(), "Δ", "{what}");
                }
            }
        }
        for f in embedded_fonts(bytes) {
            assert!(f.starts_with("NotoSans-"), "{what}: {f}");
        }
    };
    for name in ["tee", "dress", "skirt", "sampler"] {
        let project = project_named(name);
        for case in golden()["patternPdfs"]
            .as_array()
            .expect("pattern pdfs")
            .iter()
            .filter(|c| c["project"] == name)
        {
            let bytes = pattern_pdf(&project, &pdf_options(&case["options"])).expect("pdf");
            let pages = case["pages"].as_array().expect("pages");
            let what = format!("{name} {}", case["options"]);
            let win_ansi = pages.iter().all(|p| {
                p["ops"]
                    .as_array()
                    .expect("ops")
                    .iter()
                    .all(|o| o[0] != "text" || !needs_unicode_font(o[1].as_str().unwrap_or("")))
            });
            if win_ansi {
                // Catalog, page tree, information, three Helvetica faces, a page and its content.
                assert_eq!(pdf_objects(&bytes).len(), 6 + 2 * pages.len(), "{what}");
                assert!(bytes.starts_with(b"%PDF-1.4\n"), "{what}");
            }
            check(&bytes, win_ansi, &what);
        }
        let tech = techpack_pdf(&project, &TechPackInfo::default());
        check(&tech, false, &format!("{name} tech pack"));
        // The Δ of the grading table: one embedded font of five objects.
        assert_eq!(embedded_fonts(&tech), ["NotoSans-Bold"], "{name} tech pack");
        assert_eq!(pdf_objects(&tech).len(), 6 + 2 * 5 + 5, "{name} tech pack");
    }
}

#[test]
fn greek_text_maps_back_through_to_unicode() {
    let text = "Ελληνικά Δ λ ω";
    let mut canvas = Canvas::new();
    canvas.text(text, 20.0, 20.0, TextOpts::left());
    canvas.set_font(FontStyle::Bold);
    canvas.text("Δ MM", 20.0, 30.0, TextOpts::right());
    let bytes = write_pdf(&canvas.into_pages(), &PdfInfo::default());
    assert_valid_pdf(&bytes, 1, "greek");
    let texts: Vec<String> = drawn_text(&bytes).iter().map(|p| left_to_right_text(p)).collect();
    assert_eq!(texts, [text, "Δ MM"]);
    assert_eq!(embedded_fonts(&bytes), ["NotoSans-Regular", "NotoSans-Bold"]);
    if poppler("pdftotext", &["-v"]).is_some() {
        let out = poppler_text(&bytes, "greek");
        assert!(out.contains(text) && out.contains("Δ MM"), "{out}");
    }
}

/// CJK text is drawn with the bundled Noto Sans CJK subset, the same on every machine.
#[test]
fn cjk_text_uses_the_bundled_font() {
    let text = "袖子 汉字 漢字 ひらがな 한국어 「sleeve」";
    let mut canvas = Canvas::new();
    canvas.text(text, 20.0, 20.0, TextOpts::left());
    canvas.set_font(FontStyle::Bold);
    canvas.text("前片", 20.0, 30.0, TextOpts::center());
    let bytes = write_pdf(&canvas.into_pages(), &PdfInfo::default());
    assert_valid_pdf(&bytes, 1, "cjk");
    // CFF outlines are embedded as OpenType font files, a PDF 1.6 feature.
    assert!(bytes.starts_with(b"%PDF-1.6\n"));
    let texts: Vec<String> = drawn_text(&bytes).iter().map(|p| left_to_right_text(p)).collect();
    assert_eq!(texts, [text, "前片"]);
    // One font for both weights.
    assert_eq!(embedded_fonts(&bytes), ["NotoSansCJKsc-Regular"]);
    let objects = pdf_objects(&bytes);
    let cid = objects
        .values()
        .map(|o| dict(o))
        .find(|d| d.contains("/Subtype /CIDFontType0"))
        .expect("CFF CID font");
    let descriptor = dict(objects[&reference(&cid, "/FontDescriptor").expect("descriptor")]);
    let file = objects[&reference(&descriptor, "/FontFile3").expect("font file")];
    assert!(dict(file).contains("/Subtype /OpenType"));
    let data = stream_data(file);
    assert!(
        data.starts_with(b"OTTO") && data.len() < 20_000,
        "{} bytes",
        data.len()
    );
    if poppler("pdftotext", &["-v"]).is_some() {
        let out = poppler_text(&bytes, "cjk");
        assert!(out.contains(text) && out.contains("前片"), "{out}");
    }
}

/// Ideographs outside the bundled subset come from a system font that has them, when the machine
/// has one (and print as `?` otherwise, which skips the rest of the test).
#[test]
fn rare_ideographs_use_a_system_font_when_present() {
    // 丂 and 丄: in GBK, outside GB 2312 and the Jōyō kanji.
    let text = "丂丄 sleeve";
    let mut canvas = Canvas::new();
    canvas.text(text, 20.0, 20.0, TextOpts::left());
    let bytes = write_pdf(&canvas.into_pages(), &PdfInfo::default());
    assert_valid_pdf(&bytes, 1, "rare cjk");
    let texts: Vec<String> = drawn_text(&bytes).iter().map(|p| left_to_right_text(p)).collect();
    let fonts = embedded_fonts(&bytes);
    if fonts.is_empty() {
        assert_eq!(texts, ["?? sleeve"]);
        eprintln!("no system font with these ideographs; skipping");
        return;
    }
    assert_eq!(texts, [text]);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    assert_ne!(fonts[0], "NotoSansCJKsc-Regular");
    if poppler("pdftotext", &["-v"]).is_some() {
        let out = poppler_text(&bytes, "rare-cjk");
        assert!(out.contains(text), "{out}");
    }
}

/// `bytes` written to a file and read back with `pdftotext`.
fn poppler_text(bytes: &[u8], name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("unicode-pdf");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("{name}.pdf"));
    std::fs::write(&path, bytes).expect("write pdf");
    let out = poppler(
        "pdftotext",
        &["-enc", "UTF-8", path.to_str().expect("utf-8 path"), "-"],
    )
    .expect("pdftotext");
    assert!(
        out.status.success() && out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 text")
}

/// Runs a poppler tool, or `None` when it is not installed.
fn poppler(tool: &str, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new(tool).args(args).output().ok()
}

#[test]
fn poppler_renders_and_extracts_persian_names() {
    if poppler("pdftotext", &["-v"]).is_none() {
        eprintln!("poppler not installed; skipping");
        return;
    }
    let (project, pattern, tech) = persian_pdfs();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("unicode-pdf");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let pattern_path = dir.join("persian-pattern.pdf");
    let tech_path = dir.join("persian-tech.pdf");
    std::fs::write(&pattern_path, &pattern).expect("write pdf");
    std::fs::write(&tech_path, &tech).expect("write pdf");
    let path = |p: &std::path::Path| p.to_str().expect("utf-8 path").to_owned();

    let fonts = poppler("pdffonts", &[&path(&pattern_path)]).expect("pdffonts");
    let fonts = String::from_utf8_lossy(&fonts.stdout).into_owned();
    let embedded: Vec<&str> = fonts.lines().filter(|l| l.contains("Vazirmatn")).collect();
    assert_eq!(embedded.len(), 2, "{fonts}");
    for l in embedded {
        // type, encoding, emb sub uni
        assert!(l.contains("CID TrueType") && l.contains("Identity-H"), "{l}");
        assert!(l.contains("yes yes yes"), "{l}");
    }

    for p in [&pattern_path, &tech_path] {
        let out = poppler("pdftotext", &["-enc", "UTF-8", &path(p), "-"]).expect("pdftotext");
        assert!(out.status.success(), "pdftotext failed");
        assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
        let text = String::from_utf8(out.stdout).expect("utf-8 text");
        if p == &pattern_path {
            for pc in project.pieces.values() {
                assert!(text.contains(&pc.name), "pdftotext lacks {:?}:\n{text}", pc.name);
            }
        } else {
            for f in project.fabrics.values() {
                assert!(text.contains(&f.name), "pdftotext lacks {:?}:\n{text}", f.name);
            }
        }
    }

    let prefix = dir.join("page");
    let out = poppler(
        "pdftoppm",
        &[
            "-r",
            "40",
            "-f",
            "1",
            "-l",
            "1",
            "-png",
            &path(&pattern_path),
            &path(&prefix),
        ],
    )
    .expect("pdftoppm");
    assert!(out.status.success(), "pdftoppm failed");
    assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
}
