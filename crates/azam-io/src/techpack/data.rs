//! Tech-pack tables: bill of materials, trims, pattern pieces, points of measure and sewing.

use azam_doc::{
    FabricId, PieceId, PomId, Project, SeamSide, SizeName, piece_area, pom_value, seam_numbers, sew_lengths,
};
use azam_geom::signed_area;
use indexmap::IndexMap;
use serde::Serialize;

use crate::format::number;
use crate::model::{
    Bounds, DEFAULT_FABRIC_WASTE, DEFAULT_FABRIC_WIDTH_MM, DEFAULT_PARTICLE_DISTANCE_MM, extra_f64,
    extra_str, fabric_content, graded_outline, or_default, pieces_in_id_order, qty,
};

/// Square millimetres per square metre.
const MM2_PER_M2: f64 = 1e6;
/// Square millimetres per square centimetre.
const MM2_PER_CM2: f64 = 1e4;
/// Millimetres per metre.
const MM_PER_M: f64 = 1000.0;
/// Tolerance of a point of measure without one, in mm.
const DEFAULT_POM_TOLERANCE_MM: f64 = 6.0;
/// Thread used per mm of topstitching, including take-up.
const THREAD_TAKE_UP: f64 = 2.5;
/// Button shape, size and finish assumed when a button does not say.
const DEFAULT_BUTTON_SHAPE: &str = "round";
const DEFAULT_BUTTON_SIZE_MM: f64 = 15.0;
const DEFAULT_BUTTON_FINISH: &str = "plastic";
/// Segment id of a topstitch that runs around the whole outline.
const WHOLE_OUTLINE: &str = "*";

/// One fabric of the bill of materials.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricRow {
    /// Fabric id.
    pub id: FabricId,
    /// Fabric name.
    pub name: String,
    /// Colour in the active colorway, `#rrggbb`.
    pub color: String,
    /// Fibre content (the preset name when none is given).
    pub content: String,
    /// Weight in g/m².
    pub weight: f64,
    /// Roll width in mm.
    pub width: f64,
    /// Pieces cut from it, with quantities (`Front ×2, Back`).
    pub placement: String,
    /// Area inside the sewing lines, in m².
    pub area_net: f64,
    /// Area inside the cut lines, in m².
    pub area_gross: f64,
    /// Length of fabric needed, including waste, in m.
    pub linear: f64,
    /// Price per metre.
    pub price: f64,
    /// Cost of the length needed.
    pub cost: f64,
}

/// One trim of the bill of materials.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimRow {
    /// Description.
    pub name: String,
    /// Colour, `#rrggbb`, or empty.
    pub color: String,
    /// Quantity in `unit`.
    pub qty: f64,
    /// `pcs` or `m`.
    pub unit: &'static str,
}

/// One pattern piece.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternRow {
    /// Piece id.
    pub id: PieceId,
    /// Piece name.
    pub name: String,
    /// Fabric name, or `—`.
    pub fabric: String,
    /// Pieces cut.
    pub qty: f64,
    /// Width of the sewing line's bounding box, in mm.
    pub w: f64,
    /// Height of the sewing line's bounding box, in mm.
    pub h: f64,
    /// Area inside the sewing line, in cm².
    pub net: f64,
    /// Area inside the cut line, in cm².
    pub gross: f64,
    /// Length of the sewing line, in mm.
    pub perim: f64,
    /// Seam allowance, in mm.
    pub sa: f64,
    /// Grain direction, in degrees.
    pub grain: f64,
    /// Simulation particle distance, in mm.
    pub pd: f64,
}

/// One point of measure across the sizes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PomRow {
    /// Measurement id.
    pub id: PomId,
    /// Name.
    pub name: String,
    /// Tolerance (±), in mm.
    pub tol: f64,
    /// Value per size, in mm.
    pub vals: IndexMap<SizeName, f64>,
}

/// One seam.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SewingRow {
    /// Seam number.
    pub n: usize,
    /// Display colour.
    pub color: String,
    /// Pieces of side A (`Front + Back`).
    pub a: String,
    /// Pieces of side B.
    pub b: String,
    /// Length of side A, in mm.
    pub la: f64,
    /// Length of side B, in mm.
    pub lb: f64,
    /// `la - lb`, in mm.
    pub diff: f64,
}

/// Everything a tech pack lists, for the active size and colorway.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TechData {
    /// Fabrics, in order of first use.
    pub fabrics: Vec<FabricRow>,
    /// Buttons by kind, buttonholes and topstitch thread.
    pub trims: Vec<TrimRow>,
    /// Pattern pieces, in id order.
    pub patterns: Vec<PatternRow>,
    /// Points of measure.
    pub poms: Vec<PomRow>,
    /// Seams by number.
    pub sewing: Vec<SewingRow>,
    /// Total fabric cost.
    pub total_cost: f64,
}

/// The tech-pack tables of a project.
#[must_use]
pub fn tech_data(project: &Project) -> TechData {
    let pieces = pieces_in_id_order(project);
    TechData {
        fabrics: fabric_rows(project),
        trims: trim_rows(project),
        patterns: pieces
            .iter()
            .map(|pc| {
                let ol = graded_outline(project, pc, None);
                let bb = Bounds::of(ol.poly.iter().copied());
                PatternRow {
                    id: pc.id.clone(),
                    name: pc.name.clone(),
                    fabric: project
                        .fabrics
                        .get(&pc.fabric)
                        .map_or_else(|| "—".to_owned(), |f| f.name.clone()),
                    qty: qty(pc),
                    w: bb.w,
                    h: bb.h,
                    net: signed_area(&ol.poly).abs() / MM2_PER_CM2,
                    gross: piece_area(project, pc, None, true) / MM2_PER_CM2,
                    perim: ol.total,
                    sa: or_default(pc.sa, 0.0),
                    grain: or_default(pc.grain, 0.0),
                    pd: or_default(pc.pd, DEFAULT_PARTICLE_DISTANCE_MM),
                }
            })
            .collect(),
        poms: project
            .poms
            .values()
            .map(|pm| PomRow {
                id: pm.id.clone(),
                name: pm.name.clone(),
                tol: extra_f64(&pm.extra, "tol").unwrap_or(DEFAULT_POM_TOLERANCE_MM),
                vals: project
                    .sizes
                    .iter()
                    .map(|sz| (sz.clone(), pom_value(project, pm, sz)))
                    .collect(),
            })
            .collect(),
        sewing: sewing_rows(project),
        total_cost: 0.0,
    }
    .with_total()
}

impl TechData {
    fn with_total(mut self) -> Self {
        self.total_cost = self.fabrics.iter().map(|f| f.cost).sum();
        self
    }
}

fn fabric_rows(project: &Project) -> Vec<FabricRow> {
    struct Acc {
        pieces: Vec<String>,
        net: f64,
        gross: f64,
    }
    let mut acc: IndexMap<&FabricId, Acc> = IndexMap::new();
    for pc in pieces_in_id_order(project) {
        if !project.fabrics.contains_key(&pc.fabric) {
            continue;
        }
        let q = qty(pc);
        let net = piece_area(project, pc, None, false);
        let gross = piece_area(project, pc, None, true);
        let r = acc.entry(&pc.fabric).or_insert(Acc {
            pieces: Vec::new(),
            net: 0.0,
            gross: 0.0,
        });
        r.pieces.push(if q > 1.0 {
            format!("{} ×{}", pc.name, number(q))
        } else {
            pc.name.clone()
        });
        r.net += net * q;
        r.gross += gross * q;
    }
    acc.into_iter()
        .filter_map(|(id, r)| {
            let fab = project.fabrics.get(id)?;
            let width = extra_f64(&fab.extra, "width").map_or(DEFAULT_FABRIC_WIDTH_MM, |w| {
                or_default(w, DEFAULT_FABRIC_WIDTH_MM)
            });
            let waste = extra_f64(&fab.extra, "waste").unwrap_or(DEFAULT_FABRIC_WASTE);
            let linear = r.gross / width * (1.0 + waste) / MM_PER_M;
            let price = extra_f64(&fab.extra, "price").map_or(0.0, |p| or_default(p, 0.0));
            Some(FabricRow {
                id: id.clone(),
                name: fab.name.clone(),
                color: project
                    .effective_fabric(id)
                    .map_or_else(|| fab.color.clone(), |f| f.color),
                content: fabric_content(fab),
                weight: fab.phys.density,
                width,
                placement: r.pieces.join(", "),
                area_net: r.net / MM2_PER_M2,
                area_gross: r.gross / MM2_PER_M2,
                linear,
                price,
                cost: linear * price,
            })
        })
        .collect()
}

fn trim_rows(project: &Project) -> Vec<TrimRow> {
    struct Group {
        shape: String,
        size: f64,
        color: String,
        finish: String,
        qty: f64,
    }
    let mut groups: IndexMap<String, Group> = IndexMap::new();
    for b in project.buttons.values() {
        let shape = extra_str(&b.extra, "shape")
            .unwrap_or(DEFAULT_BUTTON_SHAPE)
            .to_owned();
        let size = extra_f64(&b.extra, "size")
            .map_or(DEFAULT_BUTTON_SIZE_MM, |s| or_default(s, DEFAULT_BUTTON_SIZE_MM));
        let color = extra_str(&b.extra, "color").unwrap_or("").to_owned();
        let finish = extra_str(&b.extra, "finish")
            .unwrap_or(DEFAULT_BUTTON_FINISH)
            .to_owned();
        let key = format!("{shape}|{}|{color}|{finish}", number(size));
        groups
            .entry(key)
            .or_insert(Group {
                shape,
                size,
                color,
                finish,
                qty: 0.0,
            })
            .qty += 1.0;
    }
    let mut trims: Vec<TrimRow> = groups
        .into_values()
        .map(|g| TrimRow {
            name: format!("Button — {} {} mm ({})", g.shape, number(g.size), g.finish),
            color: g.color,
            qty: g.qty,
            unit: "pcs",
        })
        .collect();
    let holes = project.holes.len();
    if holes > 0 {
        trims.push(TrimRow {
            name: "Buttonhole — keyhole".to_owned(),
            color: String::new(),
            qty: holes as f64,
            unit: "pcs",
        });
    }
    let thread = topstitch_length(project);
    if thread != 0.0 {
        trims.push(TrimRow {
            name: "Topstitch thread (incl. 2.5× take-up)".to_owned(),
            color: String::new(),
            qty: (thread * THREAD_TAKE_UP / MM_PER_M).ceil(),
            unit: "m",
        });
    }
    trims
}

/// Total length of topstitching (every row), in mm.
fn topstitch_length(project: &Project) -> f64 {
    let mut thread = 0.0;
    for st in project.stitches.values() {
        let Some(pc) = st.p.as_ref().and_then(|p| project.pieces.get(p)) else {
            continue;
        };
        let ol = graded_outline(project, pc, None);
        let rows = extra_f64(&st.extra, "rows").map_or(1.0, |r| or_default(r, 1.0));
        if st.seg.0 == WHOLE_OUTLINE {
            for sg in &ol.segments {
                thread += sg.len * rows;
            }
        } else if let Some(k) = ol.segment_index(&st.seg) {
            thread += ol.segments[k].len * rows;
        }
    }
    thread
}

/// Names of the pieces of a group of seam sides (`Front + Back`).
pub(crate) fn side_names(project: &Project, sides: &[SeamSide]) -> String {
    sides
        .iter()
        .map(|sd| project.pieces.get(&sd.piece).map_or("?", |pc| pc.name.as_str()))
        .collect::<Vec<_>>()
        .join(" + ")
}

fn sewing_rows(project: &Project) -> Vec<SewingRow> {
    let lengths = sew_lengths(project);
    let numbers = seam_numbers(project);
    let mut rows: Vec<SewingRow> = project
        .sewing
        .values()
        .enumerate()
        .map(|(i, sw)| {
            let (la, lb) = lengths.get(&sw.id).map_or((0.0, 0.0), |l| (l.a, l.b));
            SewingRow {
                n: numbers.get(&sw.id).copied().filter(|&n| n > 0).unwrap_or(i + 1),
                color: sw.color.clone(),
                a: side_names(project, &sw.a),
                b: side_names(project, &sw.b),
                la,
                lb,
                diff: la - lb,
            }
        })
        .collect();
    rows.sort_by_key(|r| r.n);
    rows
}
