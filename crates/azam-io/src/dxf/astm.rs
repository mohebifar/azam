//! ASTM D6673 (AAMA) export: the pattern in its base size with grade rules, as a DXF and the
//! matching rule table (`.rul`), which pattern CAD systems read back as a graded pattern.
//!
//! Each piece's cut line is its boundary on layer 1. Its vertices are POINT entities too, turn
//! points on layer 2 and curve points on layer 3, and each vertex that moves between sizes carries
//! its rule number as text (`# 12`) on layer 1. Rules are numbered from 1 in the order they are
//! met; points that move the same way in every size share one.
//!
//! The rule table lists, per rule and size, how far the point moves from the next smaller size
//! (a `DELTA` rule): x to the right, y up, in mm.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use azam_doc::{Piece, Project};
use azam_geom::{Outline, OutlinePoint, PointId, SizeName, Vec2, cut_outline};

use super::export::{
    INSUNITS_MM, MEASUREMENT_METRIC, TEXT_INSET_MM, TEXT_MM, Writer, begin_block, end_block, piece_details,
};
use super::layer;
use crate::format::slug;
use crate::model::{Bounds, graded_outline, pieces_in_id_order};

/// Grade moves are written to this many decimals, and points whose moves agree to them share a
/// rule.
const RULE_DIGITS: usize = 2;
/// Units of a grade move per millimetre (`10^RULE_DIGITS`).
const RULE_UNIT: f64 = 100.0;
/// A boundary vertex turning by more than this many degrees is a turn point; gentler ones are
/// curve points.
const TURN_DEG: f64 = 15.0;
/// Annotation lines below the piece name, after the size, material and quantity lines, in mm.
const RULE_TABLE_TEXT_DY: f64 = 42.0;

/// The two files of an ASTM export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstmFiles {
    /// The pattern DXF.
    pub dxf: String,
    /// The grade rule table it refers to.
    pub rul: String,
}

/// The pattern as an ASTM D6673 DXF and rule table, named `table` (written into both so they can
/// be matched up). `created` is the creation date and time written into the rule table's header,
/// as `MM-DD-YYYY HH:MM`-style text of the caller's choosing.
#[must_use]
pub fn export_astm(project: &Project, table: &str, created: (&str, &str)) -> AstmFiles {
    let sizes: Vec<SizeName> = if project.sizes.is_empty() {
        vec![project.base_size.clone()]
    } else {
        project.sizes.clone()
    };
    let base = &project.base_size;
    let mut rules = RuleTable::default();
    let mut w = Writer { out: Vec::new() };
    w.section("HEADER");
    w.pair(9, "$INSUNITS");
    w.pair(70, INSUNITS_MM.to_string());
    w.pair(9, "$MEASUREMENT");
    w.pair(70, MEASUREMENT_METRIC.to_string());
    w.end_section();

    w.section("BLOCKS");
    let mut names = Vec::new();
    for (i, pc) in pieces_in_id_order(project).into_iter().enumerate() {
        let name = format!("{}_{}", slug(&pc.name).to_uppercase(), i + 1);
        let boundary = graded_boundary(project, pc, &sizes);
        let ol = graded_outline(project, pc, Some(base));
        begin_block(&mut w, &name);
        let base_index = sizes.iter().position(|s| s == base).unwrap_or(0);
        let base_line = &boundary[base_index];
        w.polyline(base_line, layer::CUT, true);
        for (v, &p) in base_line.iter().enumerate() {
            let n = base_line.len();
            let turn = turn_deg(base_line[(v + n - 1) % n], p, base_line[(v + 1) % n]);
            w.point(
                p,
                if turn > TURN_DEG {
                    layer::TURN
                } else {
                    layer::CURVE
                },
            );
            let moves: Vec<(i64, i64)> = boundary
                .iter()
                .map(|line| {
                    // DXF y runs up
                    let d = line[v] - p;
                    (fixed_units(d.x), fixed_units(-d.y))
                })
                .collect();
            if let Some(rule) = rules.number(moves) {
                w.text(p, &format!("# {rule}"), layer::CUT, TEXT_MM);
            }
        }
        piece_details(&mut w, project, pc, &ol, base);
        let bb = Bounds::of(ol.poly.iter().copied());
        w.text(
            Vec2::new(bb.cx - TEXT_INSET_MM, bb.cy + RULE_TABLE_TEXT_DY),
            &format!("Rule Table: {table}"),
            layer::CUT,
            TEXT_MM,
        );
        end_block(&mut w);
        names.push(name);
    }
    w.end_section();

    w.section("ENTITIES");
    for name in &names {
        w.pair(0, "INSERT");
        w.pair(8, "0");
        w.pair(2, name.as_str());
        w.pair(10, "0");
        w.pair(20, "0");
        w.pair(30, "0");
    }
    w.end_section();
    w.pair(0, "EOF");
    AstmFiles {
        dxf: w.out.join("\n"),
        rul: rules.write(table, &sizes, base, created),
    }
}

/// A move in hundredths of a millimetre, so equal moves compare equal.
fn fixed_units(mm: f64) -> i64 {
    (mm * RULE_UNIT).round() as i64
}

/// How far the boundary turns at `b` (from `a`, on to `c`), in degrees.
fn turn_deg(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let (u, v) = (b - a, c - b);
    let (lu, lv) = (u.x.hypot(u.y), v.x.hypot(v.y));
    if lu == 0.0 || lv == 0.0 {
        return 0.0;
    }
    let cos = ((u.x * v.x + u.y * v.y) / (lu * lv)).clamp(-1.0, 1.0);
    cos.acos().to_degrees()
}

/// The piece's cut line in each of `sizes`, vertex for vertex: each outline segment is sampled at
/// the same fractions of its length in every size (as many as the base size flattens it to), so
/// vertex `v` is the same spot of the pattern in every size. A piece whose cut line comes out
/// with a different vertex count in some size (an allowance corner that mitres in one size and
/// not another) falls back to its sewing line, which always matches.
fn graded_boundary(project: &Project, pc: &Piece, sizes: &[SizeName]) -> Vec<Vec<Vec2>> {
    let base = graded_outline(project, pc, Some(&project.base_size));
    let steps: Vec<usize> = base
        .segments
        .iter()
        .map(|sg| sg.poly.len().saturating_sub(1).max(1))
        .collect();
    let allowances = pc.allowances();
    let sampled: Vec<(Vec<OutlinePoint>, BTreeMap<PointId, PointId>)> = sizes
        .iter()
        .map(|size| sample(&graded_outline(project, pc, Some(size)), &steps))
        .collect();
    let cuts: Vec<Vec<Vec2>> = sampled
        .iter()
        .map(|(pts, segment_of)| {
            let ol = Outline::new(pts);
            cut_outline(&ol, |id| allowances.finish(segment_of.get(id).unwrap_or(id)))
        })
        .collect();
    if cuts.windows(2).all(|w| w[0].len() == w[1].len()) {
        cuts
    } else {
        sampled
            .into_iter()
            .map(|(pts, _)| pts.into_iter().map(|p| p.pos).collect())
            .collect()
    }
}

/// The outline's segments sampled at `steps[k]` equal fractions of segment `k`'s length, as
/// corner points, and the original segment of each sample (allowances are set per segment).
fn sample(ol: &Outline, steps: &[usize]) -> (Vec<OutlinePoint>, BTreeMap<PointId, PointId>) {
    let mut pts = Vec::new();
    let mut segment_of = BTreeMap::new();
    for (sg, &n) in ol.segments.iter().zip(steps) {
        for i in 0..n {
            let id = PointId::new(format!("{}~{i}", sg.id.0));
            let p = sg.point_at(i as f64 / n as f64).pos;
            pts.push(OutlinePoint::corner(id.0.clone(), p.x, p.y));
            segment_of.insert(id, sg.id.clone());
        }
    }
    (pts, segment_of)
}

/// Rules met so far, numbered from 1.
#[derive(Default)]
struct RuleTable {
    /// Each rule's move from the base size in every size, in hundredths of a millimetre.
    rules: Vec<Vec<(i64, i64)>>,
    by_moves: BTreeMap<Vec<(i64, i64)>, usize>,
}

impl RuleTable {
    /// The number of the rule moving a point by `moves` (per size, from the base size), adding it
    /// if it is new; `None` for a point that does not move.
    fn number(&mut self, moves: Vec<(i64, i64)>) -> Option<usize> {
        if moves.iter().all(|&m| m == (0, 0)) {
            return None;
        }
        if let Some(&n) = self.by_moves.get(&moves) {
            return Some(n);
        }
        self.rules.push(moves.clone());
        let n = self.rules.len();
        self.by_moves.insert(moves, n);
        Some(n)
    }

    /// The rule table file.
    fn write(&self, table: &str, sizes: &[SizeName], base: &SizeName, created: (&str, &str)) -> String {
        let mut out = String::new();
        let list: Vec<&str> = sizes.iter().map(|s| s.0.as_str()).collect();
        let _ = writeln!(out, "ASTM/D13Proposal 1 Version: D 6673-10");
        let _ = writeln!(out, "Product: azam");
        let _ = writeln!(out, "Version: {}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(out, "Creation Date: {}", created.0);
        let _ = writeln!(out, "Creation Time: {}", created.1);
        let _ = writeln!(out, "Author: azam");
        let _ = writeln!(out, "NOTES:");
        let _ = writeln!(out, "UNITS: METRIC");
        let _ = writeln!(out, "GRADE RULE TABLE: {table}");
        let _ = writeln!(out, "NUMBER OF SIZES: {}", sizes.len());
        let _ = writeln!(out, "SIZE LIST: {}", list.join(" "));
        let _ = writeln!(out, "SAMPLE SIZE: {}", base.0);
        for (k, moves) in self.rules.iter().enumerate() {
            let _ = writeln!(out, "RULE: DELTA {}", k + 1);
            let steps: Vec<String> = moves
                .iter()
                .enumerate()
                .map(|(i, &(x, y))| {
                    let (px, py) = if i == 0 { (x, y) } else { moves[i - 1] };
                    let (dx, dy) = if i == 0 { (0, 0) } else { (x - px, y - py) };
                    format!(
                        "{:.prec$},{:.prec$}",
                        dx as f64 / RULE_UNIT,
                        dy as f64 / RULE_UNIT,
                        prec = RULE_DIGITS
                    )
                })
                .collect();
            let _ = writeln!(out, "{}", steps.join(" "));
        }
        out.push_str("END\n");
        out
    }
}

/// Millimetres per inch, for rule tables in English units.
const MM_PER_INCH: f64 = 25.4;

/// A grade rule table read from a `.rul` file.
#[derive(Clone, Debug, PartialEq)]
pub struct GradeRuleTable {
    /// The table's name.
    pub name: String,
    /// The size run, smallest first.
    pub sizes: Vec<SizeName>,
    /// The sample (base) size the pattern is drawn in.
    pub sample: SizeName,
    /// Per rule number, how far the point sits from its sample-size position in each size of
    /// the run: mm, pattern frame (x right, y down).
    pub rules: BTreeMap<u32, Vec<Vec2>>,
}

/// Why a rule table could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RulError {
    /// The table names no sizes.
    NoSizes,
    /// The sample size is not one of the sizes.
    UnknownSample(String),
    /// A rule has too few values for the size run.
    ShortRule(u32),
    /// A rule kind other than `DELTA`.
    UnsupportedRule(String),
}

impl std::fmt::Display for RulError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSizes => f.write_str("the rule table lists no sizes"),
            Self::UnknownSample(s) => write!(f, "the sample size {s:?} is not in the size list"),
            Self::ShortRule(n) => write!(f, "rule {n} has fewer values than there are sizes"),
            Self::UnsupportedRule(k) => write!(f, "rules of kind {k:?} are not supported (only DELTA)"),
        }
    }
}

impl std::error::Error for RulError {}

/// Reads an ASTM D6673 rule table. `DELTA` rules (each size's step from the next smaller size)
/// are summed into each size's offset from the sample size; English units are converted to mm.
///
/// # Errors
/// A [`RulError`] when the size list, the sample size or a rule does not make sense.
pub fn import_rul(text: &str) -> Result<GradeRuleTable, RulError> {
    let mut name = String::new();
    let mut sizes: Vec<SizeName> = Vec::new();
    let mut sample = String::new();
    let mut scale = 1.0;
    let mut steps: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    let mut current: Option<u32> = None;
    let value = |line: &str, key: &str| -> Option<String> {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(key).then(|| v.trim().to_owned())
    };
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(v) = value(line, "UNITS") {
            scale = if v.eq_ignore_ascii_case("ENGLISH") {
                MM_PER_INCH
            } else {
                1.0
            };
        } else if let Some(v) = value(line, "GRADE RULE TABLE") {
            name = v;
        } else if let Some(v) = value(line, "SIZE LIST") {
            sizes = v.split_whitespace().map(SizeName::from).collect();
        } else if let Some(v) = value(line, "SAMPLE SIZE") {
            sample = v;
        } else if let Some(v) = value(line, "RULE") {
            let mut words = v.split_whitespace();
            let kind = words.next().unwrap_or_default();
            if !kind.eq_ignore_ascii_case("DELTA") {
                return Err(RulError::UnsupportedRule(kind.to_owned()));
            }
            current = words.next().and_then(|n| n.parse().ok());
            if let Some(n) = current {
                steps.insert(n, Vec::new());
            }
        } else if line.eq_ignore_ascii_case("END") {
            current = None;
        } else if let Some(values) = current.and_then(|n| steps.get_mut(&n)) {
            values.extend(
                line.split(|c: char| c.is_whitespace() || c == ',')
                    .filter_map(|w| w.parse::<f64>().ok()),
            );
        }
    }
    if sizes.is_empty() {
        return Err(RulError::NoSizes);
    }
    let sample_index = sizes
        .iter()
        .position(|s| s.0 == sample)
        .ok_or_else(|| RulError::UnknownSample(sample.clone()))?;
    let mut rules = BTreeMap::new();
    for (n, values) in steps {
        if values.len() < 2 * sizes.len() {
            return Err(RulError::ShortRule(n));
        }
        let mut at = Vec2::ZERO;
        let summed: Vec<Vec2> = (0..sizes.len())
            .map(|i| {
                // y up in the file, down in the pattern
                at += Vec2::new(values[2 * i] * scale, -values[2 * i + 1] * scale);
                at
            })
            .collect();
        let origin = summed[sample_index];
        rules.insert(n, summed.into_iter().map(|p| p - origin).collect());
    }
    Ok(GradeRuleTable {
        name,
        sample: sizes[sample_index].clone(),
        sizes,
        rules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tee() -> Project {
        let mut p = azam_doc::from_template(azam_doc::Template::Tee, azam_doc::Sex::Female, None, None, 0.0);
        p.ensure_ids_seeded();
        p
    }

    /// The rule table's moves per rule, summed back up to moves from the smallest size.
    fn parse_rules(rul: &str) -> Vec<Vec<(f64, f64)>> {
        let mut out = Vec::new();
        let mut lines = rul.lines();
        while let Some(l) = lines.next() {
            if l.starts_with("RULE: DELTA") {
                let mut acc = (0.0, 0.0);
                let row = lines
                    .next()
                    .expect("rule values")
                    .split_whitespace()
                    .map(|pair| {
                        let (x, y) = pair.split_once(',').expect("x,y");
                        acc.0 += x.parse::<f64>().expect("x");
                        acc.1 += y.parse::<f64>().expect("y");
                        acc
                    })
                    .collect();
                out.push(row);
            }
        }
        out
    }

    #[test]
    fn the_rule_table_lists_the_size_run_and_each_rule() {
        let p = tee();
        let files = export_astm(&p, "TEE", ("11-07-2026", "12:00:00"));
        let rul = &files.rul;
        assert!(rul.contains("GRADE RULE TABLE: TEE"));
        assert!(rul.contains(&format!("NUMBER OF SIZES: {}", p.sizes.len())));
        assert!(rul.contains(&format!("SAMPLE SIZE: {}", p.base_size.0)));
        let rules = parse_rules(rul);
        assert!(rules.len() > 3, "the tee grades: {} rules", rules.len());
        for r in &rules {
            assert_eq!(r.len(), p.sizes.len(), "a move per size");
            assert_eq!(r[0], (0.0, 0.0), "the smallest size's step is from itself");
        }
        // every rule number the DXF uses is in the table, and every rule is used
        let used: std::collections::BTreeSet<usize> = files
            .dxf
            .lines()
            .filter_map(|l| l.strip_prefix("# "))
            .map(|n| n.parse().expect("rule number"))
            .collect();
        assert_eq!(used, (1..=rules.len()).collect());
    }

    #[test]
    fn rules_move_the_base_boundary_onto_each_sizes_boundary() {
        let p = tee();
        let pc = pieces_in_id_order(&p)[0];
        let boundary = graded_boundary(&p, pc, &p.sizes);
        let n = boundary[0].len();
        assert!(boundary.iter().all(|b| b.len() == n), "vertex for vertex");
        let base = p
            .sizes
            .iter()
            .position(|s| *s == p.base_size)
            .expect("base in the run");
        // the largest size really is larger, and the base is where it is
        let span = |b: &[Vec2]| Bounds::of(b.iter().copied()).w;
        assert!(span(&boundary[p.sizes.len() - 1]) > span(&boundary[base]) + 1.0);
        let ol = graded_outline(&p, pc, Some(&p.base_size));
        let cut = crate::model::cut_line(pc, &ol);
        let area = |b: &[Vec2]| azam_geom::signed_area(b).abs();
        assert!(
            (area(&boundary[base]) - area(&cut)).abs() / area(&cut) < 0.01,
            "the resampled base cut line covers the cut line's area"
        );
    }

    #[test]
    fn the_dxf_marks_turn_and_curve_points() {
        let files = export_astm(&tee(), "TEE", ("", ""));
        let on = |layer: &str| {
            files
                .dxf
                .lines()
                .collect::<Vec<_>>()
                .windows(4)
                .filter(|w| w[0] == "0" && w[1] == "POINT" && w[2] == "8" && w[3] == layer)
                .count()
        };
        assert!(on("2") >= 4, "corners are turn points");
        assert!(on("3") > on("2"), "curves are mostly curve points");
    }

    #[test]
    fn the_rule_table_reads_back_as_each_sizes_offsets() {
        let p = tee();
        let files = export_astm(&p, "TEE", ("", ""));
        let table = import_rul(&files.rul).expect("readable");
        assert_eq!(table.name, "TEE");
        assert_eq!(table.sizes, p.sizes);
        assert_eq!(table.sample, p.base_size);
        // vertex for vertex, the rules move the base boundary onto each size's boundary
        let pieces = crate::import_dxf(&files.dxf).expect("readable DXF");
        let pc = pieces_in_id_order(&p)[0];
        let boundary = graded_boundary(&p, pc, &p.sizes);
        let imported = &pieces[0];
        assert!(imported.allowance_included);
        assert_eq!(imported.points.len(), boundary[0].len());
        let base = table
            .sizes
            .iter()
            .position(|s| *s == table.sample)
            .expect("sample");
        let mut graded = 0;
        for (v, pt) in imported.points.iter().enumerate() {
            assert!((pt.pos - boundary[base][v]).length() < 0.01, "base vertex {v}");
            let Some(rule) = imported.rules[v] else { continue };
            graded += 1;
            for (k, line) in boundary.iter().enumerate() {
                let moved = pt.pos + table.rules[&rule][k];
                assert!(
                    (moved - line[v]).length() < 0.02,
                    "vertex {v} in size {k}: {moved:?} vs {:?}",
                    line[v]
                );
            }
        }
        assert!(graded > 4, "{graded} graded vertices");
    }

    #[test]
    fn english_rule_tables_are_converted_to_millimetres() {
        let rul = "UNITS: ENGLISH\nGRADE RULE TABLE: T\nSIZE LIST: 8 10 12\nSAMPLE SIZE: 10\nRULE: DELTA 1\n0,0 0.25,0.5\n0.25,0.5\nEND\n";
        let t = import_rul(rul).expect("readable");
        let r = &t.rules[&1];
        assert!((r[0] - Vec2::new(-6.35, 12.7)).length() < 1e-9, "{r:?}");
        assert_eq!(r[1], Vec2::ZERO);
        assert!((r[2] - Vec2::new(6.35, -12.7)).length() < 1e-9);
        assert_eq!(
            import_rul("SIZE LIST: A B\nSAMPLE SIZE: C\n"),
            Err(RulError::UnknownSample("C".into()))
        );
    }
}
