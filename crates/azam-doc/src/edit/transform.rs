//! Moving pieces: rigid transforms of everything anchored in piece space, flips and quarter turns.

use std::collections::{BTreeMap, HashMap};

use azam_geom::{OutlinePoint, PointId, Vec2, bbox};
use glam::DAffine2;
use serde::{Deserialize, Serialize};

use crate::ids::PieceId;
use crate::num::negate;
use crate::project::{Project, WHOLE_OUTLINE};

/// A transform of piece space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PieceTransform {
    /// Move by a vector, in mm.
    Translate(Vec2),
    /// Rotate by `angle` radians around `center` (positive turns +x towards +y, clockwise on the
    /// y-down screen).
    Rotate {
        /// Centre of rotation, in mm.
        center: Vec2,
        /// Angle in radians.
        angle: f64,
    },
    /// Any affine map.
    Affine(DAffine2),
}

impl PieceTransform {
    /// The image of `p`.
    #[must_use]
    pub fn apply(&self, p: Vec2) -> Vec2 {
        match *self {
            Self::Translate(d) => Vec2::new(p.x + d.x, p.y + d.y),
            Self::Rotate { center: c, angle } => {
                let (s, co) = angle.sin_cos();
                let (dx, dy) = (p.x - c.x, p.y - c.y);
                Vec2::new(c.x + dx * co - dy * s, c.y + dx * s + dy * co)
            }
            Self::Affine(m) => m.transform_point2(p),
        }
    }
}

/// Best rigid fit (2D Procrustes) between the points two outlines share by id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidMap {
    cos: f64,
    sin: f64,
    old_center: Vec2,
    new_center: Vec2,
}

impl RigidMap {
    /// Maps a point of the new frame to the old frame.
    #[must_use]
    pub fn apply(&self, p: Vec2) -> Vec2 {
        let (dx, dy) = (p.x - self.new_center.x, p.y - self.new_center.y);
        Vec2::new(
            self.cos * dx + self.sin * dy + self.old_center.x,
            -self.sin * dx + self.cos * dy + self.old_center.y,
        )
    }
}

/// The rigid map taking `new_pts` onto `old_pts`, fitted on the points both share by id, or `None`
/// when fewer than two points are shared.
#[must_use]
pub fn rigid_map(old_pts: &[OutlinePoint], new_pts: &[OutlinePoint]) -> Option<RigidMap> {
    let old_by_id: HashMap<&PointId, Vec2> = old_pts.iter().map(|p| (&p.id, p.pos)).collect();
    let pairs: Vec<(Vec2, Vec2)> = new_pts
        .iter()
        .filter_map(|p| old_by_id.get(&p.id).map(|&o| (o, p.pos)))
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    let n = pairs.len() as f64;
    let (mut cox, mut coy, mut cnx, mut cny) = (0.0, 0.0, 0.0, 0.0);
    for (o, q) in &pairs {
        cox += o.x;
        coy += o.y;
        cnx += q.x;
        cny += q.y;
    }
    let (cox, coy, cnx, cny) = (cox / n, coy / n, cnx / n, cny / n);
    let (mut a, mut b) = (0.0, 0.0);
    for (o, q) in &pairs {
        let (ox, oy, nx, ny) = (o.x - cox, o.y - coy, q.x - cnx, q.y - cny);
        a += ox * nx + oy * ny;
        b += ox * ny - oy * nx;
    }
    let th = b.atan2(a);
    Some(RigidMap {
        cos: th.cos(),
        sin: th.sin(),
        old_center: Vec2::new(cox, coy),
        new_center: Vec2::new(cnx, cny),
    })
}

/// Applies `f` to everything anchored in the space of piece `piece`: outline, internal lines,
/// pins, graphics, and the buttons, buttonholes and comments placed on it.
pub fn transform_piece(project: &mut Project, piece: &PieceId, f: impl Fn(Vec2) -> Vec2) {
    let Some(pc) = project.pieces.get_mut(piece) else {
        return;
    };
    for p in &mut pc.pts {
        p.pos = f(p.pos);
    }
    for p in pc.internals.iter_mut().flat_map(|il| il.pts.iter_mut()) {
        p.pos = f(p.pos);
    }
    for pin in &mut pc.pins {
        let q = f(Vec2::new(pin.x, pin.y));
        (pin.x, pin.y) = (q.x, q.y);
    }
    for g in &mut pc.graphics {
        let q = f(Vec2::new(g.x, g.y));
        (g.x, g.y) = (q.x, q.y);
    }
    let items = project
        .buttons
        .values_mut()
        .chain(project.holes.values_mut())
        .chain(project.comments.values_mut());
    for item in items.filter(|it| it.p.as_ref() == Some(piece)) {
        if let (Some(x), Some(y)) = (item.x, item.y) {
            let q = f(Vec2::new(x, y));
            (item.x, item.y) = (Some(q.x), Some(q.y));
        }
    }
}

/// Axis of a flip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlipAxis {
    /// Mirror left–right.
    Horizontal,
    /// Mirror top–bottom.
    Vertical,
}

/// Flips piece `piece` in place around the centre of its points' bounding box.
///
/// The point order is reversed so the outline keeps its winding; seams, notches, topstitches,
/// points of measure, per-segment allowances and elastics follow their edges, grain and grade
/// offsets are mirrored. Hems keep their segment ids. Returns `false` if there is no such piece.
pub fn flip_piece(project: &mut Project, piece: &PieceId, axis: FlipAxis) -> bool {
    let Some(pc) = project.pieces.get(piece) else {
        return false;
    };
    let Some(bb) = bbox(pc.pts.iter().map(|p| p.pos)) else {
        return false;
    };
    let c = bb.center();
    // Each segment's start corner becomes the start of the reversed segment's successor.
    let next: HashMap<PointId, PointId> = pc
        .outline()
        .segments
        .iter()
        .map(|sg| (sg.id.clone(), pc.pts[sg.end_point].id.clone()))
        .collect();
    let ms = |s: &PointId| next.get(s).cloned().unwrap_or_else(|| s.clone());
    transform_piece(project, piece, |p| match axis {
        FlipAxis::Horizontal => Vec2::new(2.0 * c.x - p.x, p.y),
        FlipAxis::Vertical => Vec2::new(p.x, 2.0 * c.y - p.y),
    });
    for sw in project.sewing.values_mut() {
        for sd in sw.a.iter_mut().chain(sw.b.iter_mut()) {
            if &sd.piece != piece {
                continue;
            }
            for end in [&mut sd.side.a, &mut sd.side.b] {
                end.segment = ms(&end.segment);
                end.t = 1.0 - end.t;
            }
            sd.side.direction = match sd.side.direction {
                azam_geom::SideDirection::Forward => azam_geom::SideDirection::Backward,
                azam_geom::SideDirection::Backward => azam_geom::SideDirection::Forward,
            };
        }
    }
    for st in project.stitches.values_mut() {
        if st.p.as_ref() == Some(piece) && st.seg.0 != WHOLE_OUTLINE {
            st.seg = ms(&st.seg);
        }
    }
    for r in project.poms.values_mut().flat_map(|pm| pm.refs.iter_mut()) {
        if &r.p == piece {
            r.s = ms(&r.s);
        }
    }
    let Some(pc) = project.pieces.get_mut(piece) else {
        return false;
    };
    pc.pts.reverse();
    for n in &mut pc.notches {
        n.s = ms(&n.s);
        n.t = Some(1.0 - n.t.unwrap_or(0.0));
    }
    if let Some(sa) = &mut pc.sa_seg {
        *sa = remap_keys(sa, &ms);
    }
    if let Some(el) = &mut pc.elastic {
        *el = remap_keys(el, &ms);
    }
    if axis == FlipAxis::Horizontal {
        pc.grain = negate(pc.grain);
    }
    for g in pc
        .grade
        .iter_mut()
        .flat_map(|r| r.values_mut())
        .flat_map(|s| s.values_mut())
    {
        match axis {
            FlipAxis::Horizontal => g.dx = negate(g.dx),
            FlipAxis::Vertical => g.dy = negate(g.dy),
        }
    }
    true
}

fn remap_keys<V: Clone>(
    map: &BTreeMap<PointId, V>,
    ms: &impl Fn(&PointId) -> PointId,
) -> BTreeMap<PointId, V> {
    map.iter().map(|(k, v)| (ms(k), v.clone())).collect()
}

/// Turns piece `piece` a quarter turn around the centre of its points' bounding box.
pub fn rotate_piece_90(project: &mut Project, piece: &PieceId) {
    let Some(bb) = project
        .pieces
        .get(piece)
        .and_then(|pc| bbox(pc.pts.iter().map(|p| p.pos)))
    else {
        return;
    };
    let c = bb.center();
    transform_piece(project, piece, |p| {
        Vec2::new(c.x - (p.y - c.y), c.y + (p.x - c.x))
    });
}
