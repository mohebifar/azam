//! Builds `crates/azam-io/fonts/NotoSansCJKsc-Regular-subset.otf`, the bundled CJK fallback font of
//! the PDF writer, from Noto Sans CJK SC Regular.
//!
//! Usage: `cjk-subset <NotoSansCJKsc-Regular.otf> <Unihan_OtherMappings.txt> <output.otf>`
//!
//! The kept characters:
//!
//! - GB 2312 level 1 and 2 hanzi: every ideograph the GB 2312 rows 16–87 (EUC-CN lead bytes
//!   0xB0–0xF7, trail bytes 0xA1–0xFE) decode to (6,763);
//! - the Jōyō kanji: every code point with a `kJoyoKanji` field in the Unihan database;
//! - KS X 1001 Hangul: every syllable the rows 16–40 (EUC-KR lead bytes 0xB0–0xC8, trail bytes
//!   0xA1–0xFE) decode to (2,350), and the Hangul compatibility jamo U+3131–U+318E;
//! - kana: U+3041–U+309F, U+30A0–U+30FF, U+31F0–U+31FF;
//! - CJK punctuation and full- and half-width forms: U+3000–U+303F, U+FF01–U+FFEF.
//!
//! Glyphs are subset with the `subsetter` crate (which drops the `cmap`), then a format 12 `cmap`
//! for the kept characters and the original `OS/2` table are added back, so the result is a
//! complete font to shape and measure with. Layout tables are dropped (glyph ids change), which
//! loses only optional CJK features such as vertical forms.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use subsetter::GlyphRemapper;
use ttf_parser::{Face, Tag};

/// The characters to keep.
fn charset(unihan: &str) -> BTreeSet<char> {
    let mut set = BTreeSet::new();
    let mut decode = |encoding: &'static encoding_rs::Encoding,
                      leads: std::ops::RangeInclusive<u8>,
                      keep: fn(char) -> bool| {
        for lead in leads {
            for trail in 0xa1..=0xfe_u8 {
                let bytes = [lead, trail];
                let (text, errors) = encoding.decode_without_bom_handling(&bytes);
                if errors {
                    continue;
                }
                set.extend(text.chars().filter(|&c| keep(c)));
            }
        }
    };
    decode(encoding_rs::GBK, 0xb0..=0xf7, |c| {
        ('\u{4e00}'..='\u{9fff}').contains(&c)
    });
    decode(encoding_rs::EUC_KR, 0xb0..=0xc8, |c| {
        ('\u{ac00}'..='\u{d7a3}').contains(&c)
    });
    for line in unihan.lines() {
        let mut fields = line.split('\t');
        if let (Some(code), Some("kJoyoKanji")) = (fields.next(), fields.next())
            && let Some(c) = code
                .strip_prefix("U+")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .and_then(char::from_u32)
        {
            set.insert(c);
        }
    }
    for range in [
        '\u{3131}'..='\u{318e}',
        '\u{3041}'..='\u{309f}',
        '\u{30a0}'..='\u{30ff}',
        '\u{31f0}'..='\u{31ff}',
        '\u{3000}'..='\u{303f}',
        '\u{ff01}'..='\u{ffef}',
    ] {
        set.extend(range);
    }
    set
}

/// A format 12 `cmap` (for platforms 0/4 and 3/10) mapping each character to its glyph.
fn cmap(map: &BTreeMap<char, u16>) -> Vec<u8> {
    let mut groups: Vec<(u32, u32, u32)> = Vec::new();
    for (&c, &g) in map {
        let (c, g) = (u32::from(c), u32::from(g));
        match groups.last_mut() {
            Some(last) if last.1 + 1 == c && last.2 + (last.1 - last.0) + 1 == g => last.1 = c,
            _ => groups.push((c, c, g)),
        }
    }
    let mut out = Vec::new();
    out.extend(0u16.to_be_bytes());
    out.extend(2u16.to_be_bytes());
    for (platform, encoding) in [(0u16, 4u16), (3, 10)] {
        out.extend(platform.to_be_bytes());
        out.extend(encoding.to_be_bytes());
        out.extend(20u32.to_be_bytes());
    }
    let length = 16 + 12 * groups.len() as u32;
    out.extend(12u16.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(length.to_be_bytes());
    out.extend(0u32.to_be_bytes());
    out.extend((groups.len() as u32).to_be_bytes());
    for (start, end, glyph) in groups {
        out.extend(start.to_be_bytes());
        out.extend(end.to_be_bytes());
        out.extend(glyph.to_be_bytes());
    }
    out
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// A table tag and its data.
type Table = ([u8; 4], Vec<u8>);

/// The tables of a single (non-collection) font.
fn tables(font: &[u8]) -> Result<Vec<Table>, Box<dyn Error>> {
    let count = usize::from(u16::from_be_bytes([font[4], font[5]]));
    (0..count)
        .map(|i| {
            let r = &font[12 + 16 * i..28 + 16 * i];
            let tag = [r[0], r[1], r[2], r[3]];
            let offset = u32::from_be_bytes([r[8], r[9], r[10], r[11]]) as usize;
            let len = u32::from_be_bytes([r[12], r[13], r[14], r[15]]) as usize;
            let data = font.get(offset..offset + len).ok_or("table out of bounds")?;
            Ok((tag, data.to_vec()))
        })
        .collect()
}

/// An sfnt file of `flavor` with `tables`, checksums and the head checksum adjustment.
fn assemble(flavor: [u8; 4], mut tables: Vec<Table>) -> Vec<u8> {
    tables.sort_by_key(|t| t.0);
    let count = tables.len() as u16;
    let selector = 15 - count.leading_zeros() as u16;
    let range = (1u16 << selector) * 16;
    let mut out = flavor.to_vec();
    for v in [count, range, selector, count * 16 - range] {
        out.extend(v.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    let mut head_at = None;
    for (tag, data) in &mut tables {
        if tag == b"head" {
            data[8..12].fill(0);
            head_at = Some(offset + 8);
        }
        out.extend(*tag);
        out.extend(checksum(data).to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        out.extend(data);
        out.resize(out.len().div_ceil(4) * 4, 0);
    }
    if let Some(at) = head_at {
        let adjust = 0xb1b0_afba_u32.wrapping_sub(checksum(&out));
        out[at..at + 4].copy_from_slice(&adjust.to_be_bytes());
    }
    out
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [font_path, unihan_path, out_path] = &args[..] else {
        return Err(
            "usage: cjk-subset <NotoSansCJKsc-Regular.otf> <Unihan_OtherMappings.txt> <output.otf>".into(),
        );
    };
    let data = std::fs::read(font_path)?;
    let unihan = std::fs::read_to_string(unihan_path)?;
    let face = Face::parse(&data, 0)?;
    let wanted = charset(&unihan);

    let mut remapper = GlyphRemapper::new();
    let mut map = BTreeMap::new();
    let mut missing = 0;
    for &c in &wanted {
        match face.glyph_index(c).filter(|g| g.0 != 0) {
            Some(g) => {
                map.insert(c, remapper.remap(g.0));
            }
            None => missing += 1,
        }
    }
    let subset = subsetter::subset(&data, 0, &remapper)?;
    let mut tables = tables(&subset)?;
    tables.push((*b"cmap", cmap(&map)));
    let os2 = face
        .raw_face()
        .table(Tag::from_bytes(b"OS/2"))
        .ok_or("no OS/2 table")?;
    tables.push((*b"OS/2", os2.to_vec()));
    let flavor = [subset[0], subset[1], subset[2], subset[3]];
    let out = assemble(flavor, tables);

    // The result maps every kept character to the outline it had.
    let check = Face::parse(&out, 0)?;
    for (&c, &g) in &map {
        if check.glyph_index(c).map(|x| x.0) != Some(g) {
            return Err(format!("{c:?} does not map to glyph {g}").into());
        }
    }
    std::fs::write(out_path, &out)?;
    eprintln!(
        "{} characters ({} not in the font), {} glyphs, {} bytes",
        map.len(),
        missing,
        check.number_of_glyphs(),
        out.len()
    );
    Ok(())
}
