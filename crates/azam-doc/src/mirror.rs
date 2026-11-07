//! Linked mirror pieces: a mirror is regenerated from its source after every edit.

use azam_geom::{GradeDelta, GradeRules, OutlinePoint, PointId, Vec2};

use crate::ids::{IdGen, PartId, prefix};
use crate::num::negate;
use crate::piece::{InternalLine, LinePoint, Piece};
use crate::project::Project;

/// Prefix of a mirrored point's id.
pub const MIRROR_PREFIX: &str = "m";

/// Id of the mirrored copy of point `id`.
#[must_use]
pub fn mirrored_point_id(id: &PointId) -> PointId {
    PointId::new(format!("{MIRROR_PREFIX}{}", id.0))
}

/// Id of the segment on a linked mirror that matches segment `segment` of its source.
///
/// Mirroring reverses the point order, so the mirrored segment starts at the mirror of the source
/// segment's end corner. Returns `None` if the source has no such point.
#[must_use]
pub fn mirror_seg_id(src: &Piece, segment: &PointId) -> Option<PointId> {
    let pts = &src.pts;
    let n = pts.len();
    let i = src.point_index(segment)?;
    let mut j = (i + 1) % n;
    let mut guard = 0;
    while pts[j].curve && guard < n {
        guard += 1;
        j = (j + 1) % n;
    }
    Some(mirrored_point_id(&pts[j].id))
}

/// Id of the source segment that matches segment `mirrored` of a linked mirror; the inverse of
/// [`mirror_seg_id`]. Returns `None` if the source has no matching point.
#[must_use]
pub fn src_seg_id(src: &Piece, mirrored: &PointId) -> Option<PointId> {
    let pts = &src.pts;
    let n = pts.len();
    let unprefixed: String = mirrored.0.chars().skip(1).collect();
    let j = pts.iter().position(|p| p.id.0 == unprefixed)?;
    let mut i = (j + n - 1) % n;
    let mut guard = 0;
    while pts[i].curve && guard < n {
        guard += 1;
        i = (i + n - 1) % n;
    }
    Some(pts[i].id.clone())
}

/// `pts` mirrored across `x = axis` in reverse order, with mirrored ids.
pub(crate) fn mirrored_outline(pts: &[OutlinePoint], axis: f64) -> Vec<OutlinePoint> {
    pts.iter()
        .rev()
        .map(|p| OutlinePoint {
            id: mirrored_point_id(&p.id),
            pos: Vec2::new(2.0 * axis - p.pos.x, p.pos.y),
            curve: p.curve,
        })
        .collect()
}

/// An internal line mirrored across `x = axis` with id `id`; its points lose their ids.
pub(crate) fn mirrored_line(line: &InternalLine, axis: f64, id: PartId) -> InternalLine {
    InternalLine {
        id: Some(id),
        pts: line
            .pts
            .iter()
            .map(|p| LinePoint {
                id: None,
                pos: Vec2::new(2.0 * axis - p.pos.x, p.pos.y),
                curve: p.curve,
            })
            .collect(),
        extra: line.extra.clone(),
    }
}

/// Grade rules of a mirror: horizontal offsets negated, ids mirrored.
pub(crate) fn mirrored_grade(rules: &GradeRules) -> GradeRules {
    rules
        .iter()
        .map(|(id, by_size)| {
            let mirrored = by_size
                .iter()
                .map(|(size, d)| {
                    (
                        size.clone(),
                        GradeDelta {
                            dx: negate(d.dx),
                            dy: d.dy,
                        },
                    )
                })
                .collect();
            (mirrored_point_id(id), mirrored)
        })
        .collect()
}

/// Regenerates every linked mirror from its source: outline, internal lines, per-segment
/// allowances, hems and grading. A mirror whose source is gone becomes an ordinary piece.
pub fn sync_mirrors(project: &mut Project) {
    project.ensure_ids_seeded();
    let Project { pieces, ids, .. } = project;
    for i in 0..pieces.len() {
        let Some(link) = pieces[i].mirror.clone() else {
            continue;
        };
        let Some(src) = pieces.get(&link.src) else {
            pieces[i].mirror = None;
            continue;
        };
        let synced = mirror_of(src, &pieces[i], link.axis, ids);
        pieces[i].apply_mirror(synced);
    }
}

/// The parts of a mirror that follow its source.
struct MirrorParts {
    pts: Vec<OutlinePoint>,
    internals: Vec<InternalLine>,
    sa_seg: Option<std::collections::BTreeMap<PointId, f64>>,
    hem: Option<std::collections::BTreeMap<PointId, f64>>,
    grade: Option<GradeRules>,
}

fn mirror_of(src: &Piece, mirror: &Piece, axis: f64, ids: &mut IdGen) -> MirrorParts {
    let internals = src
        .internals
        .iter()
        .enumerate()
        .map(|(i, il)| {
            let id = mirror
                .internals
                .get(i)
                .and_then(|m| m.id.clone())
                .filter(|id| !id.0.is_empty())
                .unwrap_or_else(|| PartId::new(ids.fresh(prefix::INTERNAL)));
            mirrored_line(il, axis, id)
        })
        .collect();
    let map_segments = |m: &std::collections::BTreeMap<PointId, f64>, keep: fn(f64) -> bool| {
        m.iter()
            .filter(|&(_, &v)| keep(v))
            .filter_map(|(k, &v)| mirror_seg_id(src, k).map(|mk| (mk, v)))
            .collect()
    };
    MirrorParts {
        pts: mirrored_outline(&src.pts, axis),
        internals,
        sa_seg: src
            .sa_seg
            .as_ref()
            .filter(|m| !m.is_empty())
            .map(|m| map_segments(m, |_| true)),
        hem: src
            .hem
            .as_ref()
            .filter(|m| !m.is_empty())
            .map(|m| map_segments(m, |v| v > 0.0)),
        grade: src.grade.as_ref().map(mirrored_grade),
    }
}

impl Piece {
    fn apply_mirror(&mut self, parts: MirrorParts) {
        self.pts = parts.pts;
        self.internals = parts.internals;
        self.sa_seg = parts.sa_seg;
        self.hem = parts.hem;
        self.grade = parts.grade;
    }
}
