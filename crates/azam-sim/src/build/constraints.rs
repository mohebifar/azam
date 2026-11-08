//! Stretch, bending and fold constraints of one piece, from its mesh and fabric.

use azam_doc::Piece;

use super::zipper::{CHORD_COMPRESSION, ZIPPER_BEND_FACTOR, ZIPPER_STRETCH_FACTOR, ZipperTape};
use azam_geom::hypot;
use serde_json::Value;

use super::particles::Material;
use crate::numeric::clamp;
use crate::state::{BendConstraints, PieceRecord, StretchConstraints};

/// Elastic edges hold their (shortened) length this many times more loosely.
const ELASTIC_COMPLIANCE_FACTOR: f64 = 4.0;
/// Fold lines within this many degrees of flat do not fold.
const FLAT_FOLD_TOLERANCE_DEG: f64 = 0.5;
/// Fold angle of a flat line, in degrees.
const FLAT_FOLD_DEG: f64 = 180.0;
/// Compliance marking a bending constraint as a fold line (held both ways).
pub(crate) const FOLD_COMPLIANCE: f64 = -0.01;
/// Smallest squared rest distance of a fold constraint, in mm².
const MIN_FOLD_DISTANCE_SQ: f64 = 1.0;
/// Length factor of each outline segment from the piece's elastic settings (1 = not elastic).
fn segment_elastic(piece: &Piece, record: &PieceRecord) -> Vec<f64> {
    record
        .mesh
        .outline
        .segments
        .iter()
        .map(|sg| {
            let e = piece
                .elastic
                .as_ref()
                .and_then(|m| m.get(&sg.id))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if e == 0.0 || e.is_nan() { 1.0 } else { e / 100.0 }
        })
        .collect()
}

/// Adds the constraints of an active piece. Edges with both ends in a zipper's band (see
/// [`super::zipper`]) are held much more stiffly, as a zipper's tape holds the cloth it is sewn to,
/// and chords along the tape keep it from stretching.
pub(crate) fn add_piece(
    stretch: &mut StretchConstraints,
    bend: &mut BendConstraints,
    piece: &Piece,
    record: &PieceRecord,
    m: &Material,
    zipper: &ZipperTape,
) {
    let tape = &zipper.band;
    let mesh = &record.mesh;
    let xy = &mesh.xy;
    let off = record.offset as u32;
    let (wx, wy) = m.grain;
    let elastic = segment_elastic(piece, record);
    let nb = mesh.boundary_count();
    let mut boundary_index = vec![None; record.count];
    for (i, &v) in mesh.boundary.iter().enumerate() {
        boundary_index[v as usize] = Some(i);
    }
    let mut bend_of_edge = vec![None; record.topology.edges.len()];
    for (ei, e) in record.topology.edges.iter().enumerate() {
        let (a, b) = (e.a as usize, e.b as usize);
        let (dx, dy) = (xy[2 * b] - xy[2 * a], xy[2 * b + 1] - xy[2 * a + 1]);
        let len = nonzero(hypot(dx, dy));
        let c = (dx * wx + dy * wy).abs() / len;
        let s = (-dx * wy + dy * wx).abs() / len;
        let (c2, s2) = (c * c, s * s);
        let mut compliance = m.weft * c2 * c2 + m.warp * s2 * s2 + m.shear * 2.0 * c2 * s2;
        let mut rest = len * (m.shrink_weft * c2 + m.shrink_warp * s2);
        if let (Some(ia), Some(ib)) = (boundary_index[a], boundary_index[b]) {
            let gap = ia.abs_diff(ib);
            if gap == 1 || gap + 1 == nb {
                let k = if gap == 1 { ia.min(ib) } else { ia.max(ib) };
                let factor = elastic[mesh.boundary_segment[k] as usize];
                if is_elastic(factor) {
                    rest *= factor;
                    compliance *= ELASTIC_COMPLIANCE_FACTOR;
                }
            }
        }
        let taped = tape.get(a).copied().unwrap_or(false) && tape.get(b).copied().unwrap_or(false);
        if taped {
            compliance *= ZIPPER_STRETCH_FACTOR;
        }
        stretch.i.push(off + e.a);
        stretch.j.push(off + e.b);
        stretch.rest.push(rest as f32);
        stretch.compliance.push(compliance as f32);
        stretch.compression.push(m.compression as f32);
        if let Some(o2) = e.other_opposite {
            let o1 = e.opposite;
            let (ex, ey) = (
                xy[2 * o2 as usize] - xy[2 * o1 as usize],
                xy[2 * o2 as usize + 1] - xy[2 * o1 as usize + 1],
            );
            let el = nonzero(hypot(ex, ey));
            let cc = (ex * wx + ey * wy).abs() / el;
            let ss = (-ex * wy + ey * wx).abs() / el;
            bend.i.push(off + o1);
            bend.j.push(off + o2);
            bend.rest
                .push((el * (m.shrink_weft * cc * cc + m.shrink_warp * ss * ss)) as f32);
            let bend_factor = if taped { ZIPPER_BEND_FACTOR } else { 1.0 };
            bend.compliance
                .push(((m.bend_weft * cc * cc + m.bend_warp * ss * ss) * bend_factor) as f32);
            bend_of_edge[ei] = Some(bend.i.len() - 1);
        }
    }
    add_folds(bend, record, &bend_of_edge);
    add_tape_chords(stretch, record, m, &zipper.chains);
}

/// Stretch-only chords along each zipper chain, spanning 2, 4, 8… vertices, so the tape's
/// stiffness reaches along it in a few solver passes whatever the mesh spacing.
fn add_tape_chords(
    stretch: &mut StretchConstraints,
    record: &PieceRecord,
    m: &Material,
    chains: &[Vec<usize>],
) {
    let xy = &record.mesh.xy;
    let off = record.offset as u32;
    let (wx, wy) = m.grain;
    for chain in chains {
        let mut span = 2;
        while span < chain.len() {
            for w in 0..chain.len() - span {
                let (a, b) = (chain[w], chain[w + span]);
                let (dx, dy) = (xy[2 * b] - xy[2 * a], xy[2 * b + 1] - xy[2 * a + 1]);
                let len = hypot(dx, dy);
                if len == 0.0 || len.is_nan() {
                    continue;
                }
                let c = (dx * wx + dy * wy).abs() / len;
                let s = (-dx * wy + dy * wx).abs() / len;
                let (c2, s2) = (c * c, s * s);
                let compliance =
                    (m.weft * c2 * c2 + m.warp * s2 * s2 + m.shear * 2.0 * c2 * s2) * ZIPPER_STRETCH_FACTOR;
                stretch.i.push(off + a as u32);
                stretch.j.push(off + b as u32);
                stretch
                    .rest
                    .push((len * (m.shrink_weft * c2 + m.shrink_warp * s2)) as f32);
                stretch.compliance.push(compliance as f32);
                stretch.compression.push(CHORD_COMPRESSION as f32);
            }
            span *= 2;
        }
    }
}

/// Whether a segment length factor shortens the edge (exactly 1 means not elastic).
#[allow(clippy::float_cmp, reason = "1 is the stored marker of a non-elastic segment")]
fn is_elastic(factor: f64) -> bool {
    factor != 1.0
}

fn nonzero(len: f64) -> f64 {
    if len == 0.0 || len.is_nan() { 1.0 } else { len }
}

/// Turns the bending constraints across fold lines into two-way constraints at the fold angle.
fn add_folds(bend: &mut BendConstraints, record: &PieceRecord, bend_of_edge: &[Option<usize>]) {
    let mesh = &record.mesh;
    let xy = &mesh.xy;
    for il in &mesh.internals {
        let Some(fold) = il.fold else { continue };
        if (fold - FLAT_FOLD_DEG).abs() < FLAT_FOLD_TOLERANCE_DEG {
            continue;
        }
        let phi = clamp(fold, 0.0, 360.0) * azam_avatar::DEG;
        let phi_in = if phi > std::f64::consts::PI {
            2.0 * std::f64::consts::PI - phi
        } else {
            phi
        };
        for pair in il.verts.windows(2) {
            let (a, b) = (pair[0].min(pair[1]), pair[0].max(pair[1]));
            let Some(ei) = record.topology.edge_index(a, b) else {
                continue;
            };
            let e = &record.topology.edges[ei];
            let (Some(o2), Some(bi)) = (e.other_opposite, bend_of_edge[ei]) else {
                continue;
            };
            let (a, b) = (a as usize, b as usize);
            let (ax, ay) = (xy[2 * a], xy[2 * a + 1]);
            let (hx, hy) = (xy[2 * b] - ax, xy[2 * b + 1] - ay);
            let hl = nonzero(hypot(hx, hy));
            let (ux, uy) = (hx / hl, hy / hl);
            let project = |o: u32| {
                let o = o as usize;
                let (qx, qy) = (xy[2 * o] - ax, xy[2 * o + 1] - ay);
                (qx * ux + qy * uy, (-qx * uy + qy * ux).abs())
            };
            let (p3, p4) = (project(e.opposite), project(o2));
            let d2 =
                (p3.0 - p4.0) * (p3.0 - p4.0) + p3.1 * p3.1 + p4.1 * p4.1 - 2.0 * p3.1 * p4.1 * phi_in.cos();
            bend.rest[bi] = d2.max(MIN_FOLD_DISTANCE_SQ).sqrt() as f32;
            bend.compliance[bi] = FOLD_COMPLIANCE as f32;
        }
    }
}
