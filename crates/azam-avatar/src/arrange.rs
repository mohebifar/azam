//! Arrangement: placing flat pattern pieces around the body before draping.
//!
//! A piece is assigned a [`Slot`] (front, back, an arm, a leg, the neck…). The slot gives a frame
//! on the body; the flat piece is wrapped around the body axis (or the limb's bone chain) at a
//! clearance from the skin, keeping its 2D lengths.

use std::f64::consts::PI;

use azam_geom::BBox;
use glam::DVec3;
use serde::{Deserialize, Deserializer, Serialize};

use crate::avatar::Avatar;
use crate::joints::Side;
use crate::math::{DEG, clamp, hypot2, hypot3};

/// Default clearance between the skin and an arranged piece, in millimetres.
pub const DEFAULT_OFFSET_MM: f64 = 60.0;
/// Radius used for flat (unwrapped) placement, in millimetres.
const FLAT_RADIUS_MM: f64 = 1e5;
/// Height step when measuring the torso radius over a piece's height, in millimetres.
const RADIUS_SCAN_STEP_MM: f64 = 10.0;
/// Torso radius used when the torso table gives none, in millimetres.
const FALLBACK_BODY_RADIUS_MM: f64 = 160.0;
/// Half length of the blend zone across a limb joint, in millimetres.
const JOINT_BLEND_MM: f64 = 45.0;
/// Name of the radial head drape (semicircular chador) in [`Arrangement::drape`].
pub const CHADOR_DRAPE: &str = "chador";

/// What part of the body a slot wraps around.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    /// Around the vertical body axis.
    Body,
    /// Around an arm's bone chain.
    Arm,
    /// Around the neck.
    Neck,
    /// Around a leg's bone chain.
    Leg,
}

/// A named arrangement position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// Identifier stored in [`Arrangement::slot`].
    pub id: &'static str,
    /// Display label.
    pub label: &'static str,
    /// What the slot wraps around.
    pub kind: SlotKind,
    /// Limb side, for arm and leg slots.
    pub side: Option<Side>,
    /// Default angle in degrees. Around the body: 0 = front, 90 = the avatar's left, 180 = back.
    /// Around a limb: 0 = top of the arm or front of the leg.
    pub angle_deg: f64,
}

const fn slot(
    id: &'static str,
    label: &'static str,
    kind: SlotKind,
    side: Option<Side>,
    angle_deg: f64,
) -> Slot {
    Slot {
        id,
        label,
        kind,
        side,
        angle_deg,
    }
}

/// Every arrangement slot. The first one is the fallback for unknown ids.
pub const SLOTS: [Slot; 15] = [
    slot("front", "Front", SlotKind::Body, None, 0.0),
    slot("frontL", "Front left", SlotKind::Body, None, 40.0),
    slot("frontR", "Front right", SlotKind::Body, None, -40.0),
    slot("left", "Left side", SlotKind::Body, None, 90.0),
    slot("right", "Right side", SlotKind::Body, None, -90.0),
    slot("backL", "Back left", SlotKind::Body, None, 140.0),
    slot("backR", "Back right", SlotKind::Body, None, -140.0),
    slot("back", "Back", SlotKind::Body, None, 180.0),
    slot("armL", "Left arm", SlotKind::Arm, Some(Side::Left), 0.0),
    slot("armR", "Right arm", SlotKind::Arm, Some(Side::Right), 0.0),
    slot("neck", "Neck", SlotKind::Neck, None, 0.0),
    slot("legLF", "Left leg front", SlotKind::Leg, Some(Side::Left), 0.0),
    slot("legLB", "Left leg back", SlotKind::Leg, Some(Side::Left), 180.0),
    slot("legRF", "Right leg front", SlotKind::Leg, Some(Side::Right), 0.0),
    slot("legRB", "Right leg back", SlotKind::Leg, Some(Side::Right), 180.0),
];

/// The slot with identifier `id`, or the front slot if there is none.
#[must_use]
pub fn slot_by_id(id: &str) -> &'static Slot {
    SLOTS.iter().find(|s| s.id == id).unwrap_or(&SLOTS[0])
}

/// How a piece is arranged around the body, as stored on the piece (`arr` in the project file).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Arrangement {
    /// Slot id (see [`SLOTS`]); `None` means the front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// Clearance from the skin; `None` means [`DEFAULT_OFFSET_MM`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off: Option<f64>,
    /// Angle in degrees; `None` means the slot's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ang: Option<f64>,
    /// Height of the piece centre (body and neck slots); `None` means waist or neck height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<f64>,
    /// Distance of the piece centre along the limb from its root (limb slots).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<f64>,
    /// Mirror the piece (place it inside out).
    #[serde(default, skip_serializing_if = "is_false", deserialize_with = "null_as_false")]
    pub flip: bool,
    /// Place the piece flat instead of curving it around the body.
    #[serde(default, skip_serializing_if = "is_false", deserialize_with = "null_as_false")]
    pub flat: bool,
    /// Sideways offset along the body surface (body slots), in millimetres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shift: Option<f64>,
    /// Special drape: [`CHADOR_DRAPE`] places the piece radially over the head.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drape: Option<String>,
    /// Chador drape: x of the polar origin in the piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_x: Option<f64>,
    /// Chador drape: y of the polar origin in the piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_y: Option<f64>,
    /// Chador drape: height of the crown above the head; `None` means 12 mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crown_lift: Option<f64>,
    /// Chador drape: z of the head centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_z: Option<f64>,
    /// A rigid move after placing, mm (a piece dragged in the 3D view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved: Option<[f64; 3]>,
    /// A turn about the body's vertical axis before that move, radians (a piece turned in the 3D
    /// view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turned: Option<f64>,
}

impl Arrangement {
    /// Whether `self` and `other` differ only in [`Self::moved`] and [`Self::turned`].
    #[must_use]
    pub fn same_but_placement(&self, other: &Self) -> bool {
        let strip = |a: &Self| Self {
            moved: None,
            turned: None,
            ..a.clone()
        };
        strip(self) == strip(other)
    }

    /// Applies the 3D move and turn to `p`: turned about the vertical axis through the origin, then
    /// moved.
    #[must_use]
    pub fn place(&self, p: DVec3) -> DVec3 {
        let (s, c) = self.turned.unwrap_or(0.0).sin_cos();
        let m = self.moved.unwrap_or([0.0; 3]);
        DVec3::new(p.x * c + p.z * s + m[0], p.y + m[1], -p.x * s + p.z * c + m[2])
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

fn null_as_false<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(d)?.unwrap_or(false))
}

impl Arrangement {
    /// An arrangement in `slot` with the given clearance.
    #[must_use]
    pub fn in_slot(slot: &str, off: f64) -> Self {
        Self {
            slot: Some(slot.to_owned()),
            off: Some(off),
            ..Self::default()
        }
    }

    /// The slot this arrangement uses.
    #[must_use]
    pub fn slot(&self) -> &'static Slot {
        slot_by_id(self.slot.as_deref().unwrap_or("front"))
    }
}

/// Frame of one segment of a limb's bone chain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainFrame {
    /// Start joint.
    pub start: DVec3,
    /// Unit direction along the bone.
    pub axis: DVec3,
    /// Unit outward direction at the wrap angle.
    pub normal: DVec3,
    /// `normal × axis`.
    pub right: DVec3,
    /// Arc length of the chain before this segment.
    pub offset: f64,
    /// Segment length.
    pub length: f64,
}

/// Placement frame of a slot.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotFrame {
    /// Origin on the body axis (or limb root).
    pub origin: DVec3,
    /// Unit direction "down the piece" (along the body or bone).
    pub axis: DVec3,
    /// Unit outward direction at the slot angle.
    pub normal: DVec3,
    /// Viewer's right on the piece, `normal × axis`.
    pub right: DVec3,
    /// Wrap radius: body or limb radius plus clearance.
    pub radius: f64,
    /// Arc length of the piece centre along the axis.
    pub t0: f64,
    /// Whether the piece wraps around the vertical body axis (body slots).
    pub wraps_body: bool,
    /// Bone chain frames for limb slots, empty otherwise.
    pub chain: Vec<ChainFrame>,
}

fn cross(a: DVec3, b: DVec3) -> DVec3 {
    DVec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

fn length(a: DVec3) -> f64 {
    hypot3(a.x, a.y, a.z)
}

fn normalize(a: DVec3) -> DVec3 {
    let l = length(a);
    let l = if l > 0.0 { l } else { 1.0 };
    DVec3::new(a.x / l, a.y / l, a.z / l)
}

fn project_out(v: DVec3, a: DVec3) -> DVec3 {
    let d = v.x * a.x + v.y * a.y + v.z * a.z;
    DVec3::new(v.x - a.x * d, v.y - a.y * d, v.z - a.z * d)
}

/// Frames along a bone chain through `points`.
///
/// `reference` is the zero-angle direction, `reference2` the direction of positive angles and
/// `fallback` replaces `reference` where it is nearly parallel to a bone. The normal of each
/// segment is turned by `angle` radians from the zero direction.
#[must_use]
pub fn chain_frames(
    points: &[DVec3],
    reference: DVec3,
    reference2: DVec3,
    fallback: DVec3,
    angle: f64,
) -> Vec<ChainFrame> {
    let mut out = Vec::with_capacity(points.len().saturating_sub(1));
    let mut t = 0.0;
    let (cos, sin) = (angle.cos(), angle.sin());
    for pair in points.windows(2) {
        let (a0, b0) = (pair[0], pair[1]);
        let d = DVec3::new(b0.x - a0.x, b0.y - a0.y, b0.z - a0.z);
        let len = length(d);
        let len = if len > 0.0 { len } else { 1.0 };
        let axis = DVec3::new(d.x / len, d.y / len, d.z / len);
        let mut n0 = project_out(reference, axis);
        if length(n0) < 0.2 {
            n0 = project_out(fallback, axis);
        }
        let n0 = normalize(n0);
        let q = project_out(reference2, axis);
        let qd = q.x * n0.x + q.y * n0.y + q.z * n0.z;
        let mut q = DVec3::new(q.x - n0.x * qd, q.y - n0.y * qd, q.z - n0.z * qd);
        if length(q) < 1e-6 {
            q = cross(axis, n0);
        }
        let q = normalize(q);
        let normal = normalize(DVec3::new(
            n0.x * cos + q.x * sin,
            n0.y * cos + q.y * sin,
            n0.z * cos + q.z * sin,
        ));
        out.push(ChainFrame {
            start: a0,
            axis,
            normal,
            right: cross(normal, axis),
            offset: t,
            length: len,
        });
        t += len;
    }
    out
}

/// Point on a bone chain at arc length `t`, wrap angle `phi` (radians) and radius `radius`,
/// blended smoothly across the joints. With `flat_u` the point is placed on the tangent plane
/// at sideways offset `flat_u` instead of on the cylinder.
#[must_use]
pub fn chain_position(chain: &[ChainFrame], t: f64, phi: f64, radius: f64, flat_u: Option<f64>) -> DVec3 {
    let at = |c: &ChainFrame| {
        let u = t - c.offset;
        let (cr, sr) = match flat_u {
            Some(fu) => (radius, fu),
            None => (phi.cos() * radius, phi.sin() * radius),
        };
        DVec3::new(
            c.start.x + c.axis.x * u + c.normal.x * cr + c.right.x * sr,
            c.start.y + c.axis.y * u + c.normal.y * cr + c.right.y * sr,
            c.start.z + c.axis.z * u + c.normal.z * cr + c.right.z * sr,
        )
    };
    if chain.is_empty() {
        return DVec3::ZERO;
    }
    let mut i = 0;
    while i < chain.len() - 1 && t > chain[i + 1].offset {
        i += 1;
    }
    let mut p = at(&chain[i]);
    for j in 1..chain.len() {
        let tj = chain[j].offset;
        let x = (t - (tj - JOINT_BLEND_MM)) / (2.0 * JOINT_BLEND_MM);
        if x <= 0.0 || x >= 1.0 {
            continue;
        }
        let w = x * x * (3.0 - 2.0 * x);
        let (p1, p2) = (at(&chain[j - 1]), at(&chain[j]));
        p = DVec3::new(
            p1.x + (p2.x - p1.x) * w,
            p1.y + (p2.y - p1.y) * w,
            p1.z + (p2.z - p1.z) * w,
        );
        break;
    }
    p
}

/// Placement frame of `arr` on `avatar`. `v_range` is the piece's vertical extent relative to
/// its centre (`[below, above]`, below negative), used to measure the torso radius it spans.
#[must_use]
pub fn slot_frame(avatar: &Avatar, arr: &Arrangement, v_range: [f64; 2]) -> SlotFrame {
    let sl = arr.slot();
    let off = arr.off.unwrap_or(DEFAULT_OFFSET_MM);
    let ang = arr.ang.unwrap_or(sl.angle_deg) * DEG;
    let profile = &avatar.profile;
    let down = DVec3::new(0.0, -1.0, 0.0);
    match sl.kind {
        SlotKind::Body => {
            let h = arr.h.unwrap_or(profile.levels.waist);
            let torso = &profile.torso;
            let mut r = 0.0_f64;
            let (y0, y1) = (h + v_range[0], h + v_range[1]);
            let mut y = y0;
            while y <= y1 {
                let e = torso.eval(clamp(y, torso.y0, torso.y1));
                r = r.max(e.half_width.max(e.half_depth + e.center_z.abs()));
                y += RADIUS_SCAN_STEP_MM;
            }
            if r <= 0.0 {
                r = FALLBACK_BODY_RADIUS_MM;
            }
            let n = DVec3::new(ang.sin(), 0.0, ang.cos());
            SlotFrame {
                origin: DVec3::new(0.0, h, 0.0),
                axis: down,
                normal: n,
                right: cross(n, down),
                radius: r + off,
                t0: 0.0,
                wraps_body: true,
                chain: Vec::new(),
            }
        }
        SlotKind::Neck => {
            let h = arr.h.unwrap_or(profile.levels.neck + 10.0);
            let n = DVec3::new(ang.sin(), 0.0, ang.cos());
            SlotFrame {
                origin: DVec3::new(0.0, h, -10.0),
                axis: down,
                normal: n,
                right: cross(n, down),
                radius: profile.neck_radius + off * 0.5 + 8.0,
                t0: 0.0,
                wraps_body: false,
                chain: Vec::new(),
            }
        }
        SlotKind::Arm | SlotKind::Leg => {
            let side = sl.side.unwrap_or(Side::Left);
            let j = avatar.joints.side(side);
            let sd = side.sign();
            let t0 = arr.t.unwrap_or(0.0);
            let (origin, radius, chain) = if sl.kind == SlotKind::Arm {
                // shoulder -> elbow -> wrist; zero angle = top of the arm, positive = towards the front
                let chain = chain_frames(
                    &[j.shoulder, j.elbow, j.wrist],
                    DVec3::Y,
                    DVec3::Z,
                    DVec3::new(sd, 0.0, 0.0),
                    ang,
                );
                (j.shoulder, j.upper_arm_radius + off * 0.7, chain)
            } else {
                // hip -> knee -> ankle; zero angle = front, positive = outward
                let chain = chain_frames(
                    &[j.hip, j.knee, j.ankle],
                    DVec3::Z,
                    DVec3::new(sd, 0.0, 0.0),
                    DVec3::Z,
                    ang,
                );
                (j.hip, j.thigh_radius + off * 0.7, chain)
            };
            let c0 = chain[0];
            SlotFrame {
                origin,
                axis: c0.axis,
                normal: c0.normal,
                right: c0.right,
                radius,
                t0,
                wraps_body: false,
                chain,
            }
        }
    }
}

/// Places the 2D vertices `xy` (`x, y` per vertex, pattern coordinates) of a piece with outline
/// bounds `bounds` around `avatar`, writing `x, y, z` per vertex into `out`. Returns the frame
/// used.
///
/// # Panics
///
/// Panics if `out` holds fewer than `3 * xy.len() / 2` values.
pub fn arrange_positions(
    avatar: &Avatar,
    arr: &Arrangement,
    bounds: &BBox,
    xy: &[f64],
    out: &mut [f32],
) -> SlotFrame {
    let n = xy.len() / 2;
    assert!(out.len() >= n * 3, "output buffer too small for {n} vertices");
    let (cx, cy) = (
        f64::midpoint(bounds.min.x, bounds.max.x),
        f64::midpoint(bounds.min.y, bounds.max.y),
    );
    let mut write = |i: usize, p: DVec3| {
        out[i * 3] = p.x as f32;
        out[i * 3 + 1] = p.y as f32;
        out[i * 3 + 2] = p.z as f32;
    };
    if arr.drape.as_deref() == Some(CHADOR_DRAPE) {
        return chador(avatar, arr, xy, &mut write);
    }
    let frame = slot_frame(avatar, arr, [-(bounds.max.y - cy), cy - bounds.min.y]);
    let flip = if arr.flip { -1.0 } else { 1.0 };
    if !frame.chain.is_empty() {
        for i in 0..n {
            let u = (xy[2 * i] - cx) * flip;
            let v = -(xy[2 * i + 1] - cy);
            let p = chain_position(
                &frame.chain,
                frame.t0 - v,
                u / frame.radius,
                frame.radius,
                arr.flat.then_some(u),
            );
            write(i, p);
        }
        return frame;
    }
    let radius = if arr.flat { FLAT_RADIUS_MM } else { frame.radius };
    // lateral offset along the body, e.g. to place two open front panels side by side
    let shift = if frame.wraps_body {
        arr.shift.unwrap_or(0.0)
    } else {
        0.0
    };
    let (o, a, nn, r) = (frame.origin, frame.axis, frame.normal, frame.right);
    for i in 0..n {
        let u = (xy[2 * i] - cx) * flip + shift;
        let v = -(xy[2 * i + 1] - cy);
        let phi = u / radius;
        let t = frame.t0 - v;
        let (cr, sr) = (phi.cos() * radius, phi.sin() * radius);
        let base = if arr.flat {
            DVec3::new(
                nn.x * frame.radius + r.x * u,
                nn.y * frame.radius + r.y * u,
                nn.z * frame.radius + r.z * u,
            )
        } else {
            DVec3::new(nn.x * cr + r.x * sr, nn.y * cr + r.y * sr, nn.z * cr + r.z * sr)
        };
        write(
            i,
            DVec3::new(
                o.x + a.x * t + base.x,
                o.y + a.y * t + base.y,
                o.z + a.z * t + base.z,
            ),
        );
    }
    frame
}

/// Radius of the flat crown disc of the chador drape, in millimetres.
const CHADOR_CROWN_RADIUS_MM: f64 = 200.0;
/// Radius of the spherical crown cap, in millimetres.
const CHADOR_CAP_RADIUS_MM: f64 = 100.0;

/// Radial head drape: a semicircular chador with a polar datum shared by all its pieces.
fn chador(avatar: &Avatar, arr: &Arrangement, xy: &[f64], write: &mut impl FnMut(usize, DVec3)) -> SlotFrame {
    let (ox, oy) = (arr.origin_x.unwrap_or(0.0), arr.origin_y.unwrap_or(0.0));
    let crown = avatar.profile.height + arr.crown_lift.unwrap_or(12.0);
    let z0 = arr.head_z.unwrap_or(0.0);
    let cap = CHADOR_CAP_RADIUS_MM;
    for i in 0..xy.len() / 2 {
        let u = xy[2 * i] - ox;
        let v = xy[2 * i + 1] - oy;
        let r = hypot2(u, v);
        let gap = 0.02 + 0.9 * (-((r - 225.0) / 200.0).powi(2)).exp();
        let phi = PI + (2.0 - 2.0 * gap / PI) * u.atan2(v);
        let rr = cap.min(r * 0.5);
        let y = if r <= CHADOR_CROWN_RADIUS_MM {
            crown - cap + (cap * cap - rr * rr).max(0.0).sqrt()
        } else {
            crown - cap - 0.75_f64.sqrt() * (r - CHADOR_CROWN_RADIUS_MM)
        };
        write(i, DVec3::new(0.5 * r * phi.sin(), y, z0 + 0.5 * r * phi.cos()));
    }
    SlotFrame {
        origin: DVec3::new(0.0, crown, z0),
        axis: DVec3::new(0.0, -1.0, 0.0),
        normal: DVec3::new(0.0, 0.0, -1.0),
        right: DVec3::new(-1.0, 0.0, 0.0),
        radius: 1.0,
        t0: 0.0,
        wraps_body: false,
        chain: Vec::new(),
    }
}

/// A clickable arrangement target shown on the avatar.
#[derive(Clone, Debug, PartialEq)]
pub struct ArrangementPoint {
    /// Slot id.
    pub slot: &'static str,
    /// Height of the target (body slots).
    pub h: Option<f64>,
    /// Distance along the limb (limb and neck slots).
    pub t: Option<f64>,
    /// Display label.
    pub label: String,
    /// Position in 3D.
    pub position: DVec3,
}

/// Distance along the arm of an arm target, in millimetres.
const ARM_TARGET_T_MM: f64 = 140.0;
/// Distance along the leg of a leg target, in millimetres.
const LEG_TARGET_T_MM: f64 = 260.0;

/// The arrangement targets of `avatar`: every body slot at chest, waist, hip, thigh and knee
/// height, plus one target per limb and neck slot.
#[must_use]
pub fn arrangement_points(avatar: &Avatar) -> Vec<ArrangementPoint> {
    let ys = avatar.profile.levels;
    let heights = [
        ("Chest", ys.bust),
        ("Waist", ys.waist),
        ("Hip", ys.hip),
        ("Thigh", ys.crotch - 120.0),
        ("Knee", ys.knee + 140.0),
    ];
    let mut pts = Vec::new();
    for sl in &SLOTS {
        if sl.kind == SlotKind::Body {
            for (name, h) in heights {
                let arr = Arrangement {
                    h: Some(h),
                    ..Arrangement::in_slot(sl.id, DEFAULT_OFFSET_MM)
                };
                let f = slot_frame(avatar, &arr, [-10.0, 10.0]);
                pts.push(ArrangementPoint {
                    slot: sl.id,
                    h: Some(h),
                    t: None,
                    label: format!("{} · {name}", sl.label),
                    position: DVec3::new(
                        f.origin.x + f.normal.x * f.radius,
                        f.origin.y,
                        f.origin.z + f.normal.z * f.radius,
                    ),
                });
            }
        } else {
            let arr = Arrangement {
                t: Some(0.0),
                ..Arrangement::in_slot(sl.id, DEFAULT_OFFSET_MM)
            };
            let f = slot_frame(avatar, &arr, [-10.0, 10.0]);
            let tt = match sl.kind {
                SlotKind::Arm => ARM_TARGET_T_MM,
                SlotKind::Leg => LEG_TARGET_T_MM,
                SlotKind::Body | SlotKind::Neck => 0.0,
            };
            let position = if f.chain.is_empty() {
                DVec3::new(
                    f.origin.x + f.axis.x * tt + f.normal.x * f.radius,
                    f.origin.y + f.axis.y * tt + f.normal.y * f.radius,
                    f.origin.z + f.axis.z * tt + f.normal.z * f.radius,
                )
            } else {
                chain_position(&f.chain, tt, 0.0, f.radius, None)
            };
            pts.push(ArrangementPoint {
                slot: sl.id,
                h: None,
                t: Some(tt),
                label: sl.label.to_owned(),
                position,
            });
        }
    }
    pts
}
