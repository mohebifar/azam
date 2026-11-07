//! The compute passes: which buffers each binds and how its WGSL module is assembled.
//!
//! Each pass's shader is the shared header, one `@group(0)` storage declaration per binding
//! (generated from the table below, so bind group layouts and shaders cannot disagree), the
//! dispatch descriptor uniform for passes that use one, the helper functions it needs and the
//! pass's own `main`.

use std::fmt::Write as _;

use crate::buffers::Buf as B;
use crate::layout::{SCAN_WORKGROUP, WORKGROUP};

/// WGSL type of a binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    /// `array<vec4<f32>>`.
    V4,
    /// `array<vec4<u32>>`.
    U4,
    /// `array<f32>`.
    F1,
    /// `array<u32>`.
    U1,
    /// `array<i32>`.
    I1,
    /// `array<atomic<u32>>`.
    AtomicU,
    /// `array<atomic<i32>>`.
    AtomicI,
    /// `array<Con>`.
    Con,
    /// `array<vec2<f32>>`.
    V2,
}

impl Ty {
    fn wgsl(self) -> &'static str {
        match self {
            Self::V4 => "array<vec4<f32>>",
            Self::U4 => "array<vec4<u32>>",
            Self::F1 => "array<f32>",
            Self::U1 => "array<u32>",
            Self::I1 => "array<i32>",
            Self::AtomicU => "array<atomic<u32>>",
            Self::AtomicI => "array<atomic<i32>>",
            Self::Con => "array<Con>",
            Self::V2 => "array<vec2<f32>>",
        }
    }
}

/// One storage binding of a pass.
#[derive(Clone, Copy, Debug)]
pub struct Bind {
    /// Name of the variable in the shader.
    pub name: &'static str,
    /// Buffer bound.
    pub buffer: B,
    /// Whether the pass writes it.
    pub write: bool,
    /// Element type.
    pub ty: Ty,
}

const fn r(name: &'static str, buffer: B, ty: Ty) -> Bind {
    Bind {
        name,
        buffer,
        write: false,
        ty,
    }
}

const fn w(name: &'static str, buffer: B, ty: Ty) -> Bind {
    Bind {
        name,
        buffer,
        write: true,
        ty,
    }
}

/// Helper functions a pass includes.
#[derive(Clone, Copy, Debug, Default)]
#[allow(clippy::struct_excessive_bools, reason = "independent includes")]
pub struct Includes {
    /// The dispatch descriptor uniform (`D`, group 1).
    pub dispatch: bool,
    /// Body distance grid sampling.
    pub grid: bool,
    /// Spatial hash functions.
    pub hash: bool,
    /// Fixed-point correction accumulation.
    pub delta: bool,
    /// Closest point on a triangle.
    pub closest: bool,
}

/// A compute pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pass {
    /// Seam gap sums.
    Gap,
    /// Gravity hold-back.
    Grav,
    /// Clears the pressure normals.
    NormalClear,
    /// Accumulates the pressure normals.
    NormalAccumulate,
    /// Integration.
    Integrate,
    /// Body and floor collision.
    Body,
    /// One colour of distance constraints.
    Dist,
    /// One colour of sewing constraints.
    Sew,
    /// Grab.
    Grab,
    /// End-of-substep penetration guard.
    Guard,
    /// Skin vertices against cloth triangles.
    BodyTri,
    /// Applies accumulated corrections.
    Apply,
    /// Pins.
    Pins,
    /// Velocity update.
    Vel,
    /// Clears the triangle hash counts.
    HashClear,
    /// Counts triangle hash entries.
    HashCount,
    /// Block prefix sums of the counts.
    Scan1,
    /// Prefix sum of the block totals.
    Scan2,
    /// Adds the block offsets.
    Scan3,
    /// Writes the triangle hash entries.
    Scatter,
    /// Contact detection.
    Detect,
    /// Contact resolution.
    Resolve,
    /// Kinetic energy partial sums.
    Energy,
}

/// What a pass is made of.
#[derive(Clone, Copy, Debug)]
pub struct PassSpec {
    /// Label (also used in errors).
    pub label: &'static str,
    /// Storage bindings, at binding 1 onwards.
    pub binds: &'static [Bind],
    /// Helpers.
    pub includes: Includes,
    /// Workgroup size along x.
    pub workgroup: u32,
    /// The pass's `main`.
    pub code: &'static str,
}

const HEADER: &str = include_str!("shaders/header.wgsl");
const FN_GRID: &str = include_str!("shaders/grid.wgsl");
const FN_HASH: &str = include_str!("shaders/hash.wgsl");
const FN_DELTA: &str = include_str!("shaders/delta.wgsl");
const FN_CLOSEST: &str = include_str!("shaders/closest.wgsl");

use Ty::{AtomicI, AtomicU, Con, F1, I1, U1, U4, V2, V4};

const B_GAP: &[Bind] = &[
    r("X", B::X, V4),
    r("SW", B::Sew, U4),
    r("PD", B::Pd, U4),
    w("CTRL", B::Ctrl, AtomicU),
];
const B_GRAV: &[Bind] = &[w("CTRL", B::Ctrl, U1), w("ST", B::State, F1)];
const B_NORMAL_CLEAR: &[Bind] = &[w("NRM", B::Normals, I1)];
const B_NORMAL_ACCUMULATE: &[Bind] = &[
    r("X", B::X, V4),
    r("TRI", B::Tri, U4),
    w("NRM", B::Normals, AtomicI),
];
const B_INTEGRATE: &[Bind] = &[
    w("X", B::X, V4),
    w("P", B::Prev, V4),
    w("V", B::Vel, V4),
    r("PP", B::Pp, V4),
    r("PD", B::Pd, U4),
    r("ST", B::State, F1),
    r("NRM", B::Normals, I1),
];
const B_BODY: &[Bind] = &[
    w("X", B::X, V4),
    w("P", B::Prev, V4),
    r("PP", B::Pp, V4),
    r("GRID", B::Grid, F1),
    r("BVEL", B::BodyVel, V4),
];
const B_DIST: &[Bind] = &[w("X", B::X, V4), r("CON", B::Con, Con)];
const B_SEW: &[Bind] = &[w("X", B::X, V4), r("SW", B::SewColored, U4)];
const B_GRAB: &[Bind] = &[w("X", B::X, V4), r("GR", B::Grab, V4)];
const B_GUARD: &[Bind] = &[w("X", B::X, V4), r("PP", B::Pp, V4), r("GRID", B::Grid, F1)];
const B_BODY_TRI: &[Bind] = &[
    r("X", B::X, V4),
    r("TRI", B::Tri, U4),
    r("PP", B::Pp, V4),
    r("SKIN", B::Skin, V4),
    r("SHEAD", B::SkinHead, I1),
    r("SNEXT", B::SkinNext, I1),
    w("DELTA", B::Delta, AtomicI),
];
const B_APPLY: &[Bind] = &[
    w("X", B::X, V4),
    w("P", B::Prev, V4),
    w("DELTA", B::Delta, AtomicI),
];
const B_PINS: &[Bind] = &[w("X", B::X, V4), r("PIN", B::Pin, V4)];
const B_VEL: &[Bind] = &[r("X", B::X, V4), r("P", B::Prev, V4), w("V", B::Vel, V4)];
const B_HASH_CLEAR: &[Bind] = &[w("HC", B::HashCount, U1), w("CTRL", B::Ctrl, U1)];
const B_HASH_COUNT: &[Bind] = &[
    r("X", B::X, V4),
    r("TRI", B::Tri, U4),
    w("HC", B::HashCount, AtomicU),
];
const B_SCAN1: &[Bind] = &[
    r("HC", B::HashCount, U1),
    w("HASH", B::Hash, U1),
    w("BS", B::BlockSums, U1),
];
const B_SCAN2: &[Bind] = &[w("BS", B::BlockSums, U1)];
const B_SCAN3: &[Bind] = &[
    w("HC", B::HashCount, U1),
    w("HASH", B::Hash, U1),
    r("BS", B::BlockSums, U1),
];
const B_SCATTER: &[Bind] = &[
    r("X", B::X, V4),
    r("TRI", B::Tri, U4),
    w("HC", B::HashCount, AtomicU),
    w("HASH", B::Hash, U1),
];
const B_DETECT: &[Bind] = &[
    r("X", B::X, V4),
    r("P", B::Prev, V4),
    r("PD", B::Pd, U4),
    r("TRI", B::Tri, U4),
    r("HASH", B::Hash, U1),
    w("CONT", B::Contacts, U4),
    w("CTRL", B::Ctrl, AtomicU),
    r("GRID", B::Grid, F1),
];
const B_RESOLVE: &[Bind] = &[
    r("X", B::X, V4),
    r("P", B::Prev, V4),
    r("PP", B::Pp, V4),
    r("CONT", B::Contacts, U4),
    r("CTRL", B::Ctrl, U1),
    r("TRI", B::Tri, U4),
    w("DELTA", B::Delta, AtomicI),
];
const B_ENERGY: &[Bind] = &[r("X", B::X, V4), r("V", B::Vel, V4), w("PART", B::Partials, V2)];

const NONE: Includes = Includes {
    dispatch: false,
    grid: false,
    hash: false,
    delta: false,
    closest: false,
};

impl Pass {
    /// Every pass, in table order.
    pub const ALL: [Self; 23] = [
        Self::Gap,
        Self::Grav,
        Self::NormalClear,
        Self::NormalAccumulate,
        Self::Integrate,
        Self::Body,
        Self::Dist,
        Self::Sew,
        Self::Grab,
        Self::Guard,
        Self::BodyTri,
        Self::Apply,
        Self::Pins,
        Self::Vel,
        Self::HashClear,
        Self::HashCount,
        Self::Scan1,
        Self::Scan2,
        Self::Scan3,
        Self::Scatter,
        Self::Detect,
        Self::Resolve,
        Self::Energy,
    ];

    /// Index of the pass in [`Pass::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    /// The pass's table entry.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one table entry per pass")]
    pub fn spec(self) -> PassSpec {
        let s = |label, binds, includes, code| PassSpec {
            label,
            binds,
            includes,
            workgroup: WORKGROUP,
            code,
        };
        let grid = Includes { grid: true, ..NONE };
        let hash = Includes { hash: true, ..NONE };
        let dispatch = Includes {
            dispatch: true,
            ..NONE
        };
        match self {
            Self::Gap => s("gap", B_GAP, NONE, include_str!("shaders/gap.wgsl")),
            Self::Grav => PassSpec {
                workgroup: 1,
                ..s("grav", B_GRAV, NONE, include_str!("shaders/grav.wgsl"))
            },
            Self::NormalClear => s(
                "nclear",
                B_NORMAL_CLEAR,
                NONE,
                include_str!("shaders/nclear.wgsl"),
            ),
            Self::NormalAccumulate => s(
                "nacc",
                B_NORMAL_ACCUMULATE,
                NONE,
                include_str!("shaders/nacc.wgsl"),
            ),
            Self::Integrate => s(
                "integrate",
                B_INTEGRATE,
                NONE,
                include_str!("shaders/integrate.wgsl"),
            ),
            Self::Body => s("body", B_BODY, grid, include_str!("shaders/body.wgsl")),
            Self::Dist => s("dist", B_DIST, dispatch, include_str!("shaders/dist.wgsl")),
            Self::Sew => s("sew", B_SEW, dispatch, include_str!("shaders/sew.wgsl")),
            Self::Grab => s("grab", B_GRAB, NONE, include_str!("shaders/grab.wgsl")),
            Self::Guard => s("guard", B_GUARD, grid, include_str!("shaders/guard.wgsl")),
            Self::BodyTri => s(
                "bodytri",
                B_BODY_TRI,
                Includes {
                    delta: true,
                    hash: true,
                    ..NONE
                },
                include_str!("shaders/bodytri.wgsl"),
            ),
            Self::Apply => s("apply", B_APPLY, dispatch, include_str!("shaders/apply.wgsl")),
            Self::Pins => s("pins", B_PINS, NONE, include_str!("shaders/pins.wgsl")),
            Self::Vel => s("vel", B_VEL, NONE, include_str!("shaders/vel.wgsl")),
            Self::HashClear => s("hclear", B_HASH_CLEAR, NONE, include_str!("shaders/hclear.wgsl")),
            Self::HashCount => s("hcount", B_HASH_COUNT, hash, include_str!("shaders/hcount.wgsl")),
            Self::Scan1 => PassSpec {
                workgroup: SCAN_WORKGROUP,
                ..s("scan1", B_SCAN1, NONE, include_str!("shaders/scan1.wgsl"))
            },
            Self::Scan2 => PassSpec {
                workgroup: SCAN_WORKGROUP,
                ..s("scan2", B_SCAN2, NONE, include_str!("shaders/scan2.wgsl"))
            },
            Self::Scan3 => s("scan3", B_SCAN3, NONE, include_str!("shaders/scan3.wgsl")),
            Self::Scatter => s("scatter", B_SCATTER, hash, include_str!("shaders/scatter.wgsl")),
            Self::Detect => s(
                "detect",
                B_DETECT,
                Includes {
                    grid: true,
                    hash: true,
                    closest: true,
                    ..NONE
                },
                include_str!("shaders/detect.wgsl"),
            ),
            Self::Resolve => s(
                "resolve",
                B_RESOLVE,
                Includes {
                    delta: true,
                    closest: true,
                    ..NONE
                },
                include_str!("shaders/resolve.wgsl"),
            ),
            Self::Energy => s("energy", B_ENERGY, NONE, include_str!("shaders/energy.wgsl")),
        }
    }

    /// The pass's complete WGSL module.
    #[must_use]
    pub fn source(self) -> String {
        let spec = self.spec();
        let mut code = String::from(HEADER);
        for (i, b) in spec.binds.iter().enumerate() {
            let access = if b.write { "read_write" } else { "read" };
            let _ = writeln!(
                code,
                "@group(0) @binding({}) var<storage, {access}> {}: {};",
                i + 1,
                b.name,
                b.ty.wgsl()
            );
        }
        let inc = spec.includes;
        if inc.dispatch {
            code.push_str("@group(1) @binding(0) var<uniform> D: DP;\n");
        }
        for (on, text) in [
            (inc.grid, FN_GRID),
            (inc.hash, FN_HASH),
            (inc.delta, FN_DELTA),
            (inc.closest, FN_CLOSEST),
        ] {
            if on {
                code.push_str(text);
            }
        }
        code.push_str(spec.code);
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_order_matches_indices() {
        for (i, p) in Pass::ALL.iter().enumerate() {
            assert_eq!(p.index(), i);
        }
    }

    #[test]
    fn every_module_parses_and_validates() {
        for p in Pass::ALL {
            let src = p.source();
            let module = naga::front::wgsl::parse_str(&src)
                .unwrap_or_else(|e| panic!("{}: {}", p.spec().label, e.emit_to_string(&src)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}: {e:?}", p.spec().label));
        }
    }

    #[test]
    fn no_pass_exceeds_eight_storage_buffers() {
        for p in Pass::ALL {
            assert!(p.spec().binds.len() <= 8, "{}", p.spec().label);
        }
    }
}
