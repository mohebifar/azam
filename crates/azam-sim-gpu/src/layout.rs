//! Memory layout shared with the shaders: constants, the frame-parameter uniform, constraint rows
//! and dispatch descriptors. Each `#[repr(C)]` struct mirrors a WGSL struct of `header.wgsl`.

use bytemuck::{Pod, Zeroable};

/// Fixed-point scale of the atomic correction accumulators (corrections in 1/4096 mm).
pub const FIXED_POINT_SCALE: f32 = 4096.0;
/// Buckets of the cloth-triangle hash (a power of two, 256 blocks of 256 for the prefix sum).
pub const TRIANGLE_HASH_BUCKETS: u32 = 65536;
/// Buckets of the skin-vertex hash (a power of two).
pub const SKIN_HASH_BUCKETS: u32 = 65536;
/// Cell size of the skin-vertex hash, mm.
pub const SKIN_CELL_MM: f32 = 24.0;
/// Workgroup size of most passes.
pub const WORKGROUP: u32 = 128;
/// Workgroup size of the prefix-sum passes.
pub const SCAN_WORKGROUP: u32 = 256;
/// Most grabbed particles uploaded.
pub const GRAB_CAP: usize = 4096;
/// Pieces with their own gravity hold-back (later pieces share the last slot).
pub const MAX_PIECES: usize = 64;
/// Values in the solver-state buffer: hold-back scale and seam gap, per-piece hold-back from 8,
/// per-piece vertical acceleration from 80.
pub const STATE_LEN: usize = 160;
/// Index of the first per-piece hold-back in the solver state.
pub const STATE_PIECE_GRAVITY: usize = 8;
/// Index of the first per-piece vertical acceleration in the solver state.
pub const STATE_PIECE_ACCEL: usize = 80;
/// Words of the control buffer: contact count, seam-gap sum and count, per-piece sums from 4 and
/// counts from 68.
pub const CONTROL_LEN: usize = 136;
/// Byte stride of the dispatch-descriptor table (the dynamic uniform offset alignment).
pub const DISPATCH_STRIDE: u64 = 256;
/// Largest speed, mm/s.
pub const MAX_SPEED_MM_S: f32 = 6000.0;
/// Fewest contact slots.
pub const MIN_CONTACT_CAP: u32 = 65536;
/// Contact slots per particle.
pub const CONTACTS_PER_PARTICLE: u32 = 24;
/// Fewest triangle-hash entries.
pub const MIN_HASH_ENTRIES: u32 = 131072;
/// Triangle-hash entries per triangle.
pub const HASH_ENTRIES_PER_TRIANGLE: u32 = 48;

/// The frame parameters (WGSL `FP`), uploaded as a uniform before every frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
#[allow(clippy::struct_field_names, reason = "names mirror the shader struct")]
pub struct FrameParams {
    /// Substep, s.
    pub h: f32,
    /// `1 / h²`.
    pub ih2: f32,
    /// Gravity, mm/s².
    pub gravity: f32,
    /// Air damping, per second.
    pub air: f32,
    /// Largest seam closing per pass, mm.
    pub max_sew: f32,
    /// Largest speed, mm/s.
    pub vmax: f32,
    /// Distance kept between cloth surfaces, mm.
    pub h0: f32,
    /// Largest normal correction per contact and substep, mm.
    pub max_step: f32,
    /// Origin of the body distance grid, mm.
    pub grid_origin: [f32; 3],
    /// Cell of the body distance grid, mm.
    pub grid_cell: f32,
    /// Values of the body distance grid along x, y and z.
    pub grid_dims: [u32; 3],
    /// Particles.
    pub n: u32,
    /// Cloth triangles.
    pub n_tri: u32,
    /// Skin vertices.
    pub n_skin: u32,
    /// Triangle-hash buckets.
    pub hash_buckets: u32,
    /// Contact slots.
    pub contact_cap: u32,
    /// Triangle-hash cell, mm.
    pub cell: f32,
    /// Contact search radius, mm.
    pub radius: f32,
    /// Triangle-hash entry slots.
    pub entry_cap: u32,
    /// Sewing constraints.
    pub n_sew: u32,
    /// Skin-hash cell, mm.
    pub skin_cell: f32,
    /// Skin-hash buckets.
    pub skin_buckets: u32,
    /// Pins.
    pub n_pin: u32,
    /// Grabbed particles.
    pub n_grab: u32,
    /// Grab point, mm.
    pub grab_point: [f32; 3],
    /// 1 when a piece has inflation pressure.
    pub has_pressure: u32,
    /// Pieces.
    pub n_pieces: u32,
    /// Triangles in the triangle hash.
    pub n_tri_hash: u32,
    /// 1 to fix the contact side between layers by layer order.
    pub layer_order: u32,
    /// Largest extra gap between layers, mm.
    pub hx_max: f32,
    /// 1 to use closest-point (edge and corner) contacts between layers.
    pub edge_contacts: u32,
    /// Cloth-on-cloth friction as a multiple of the fabric friction.
    pub friction_scale: f32,
    /// Friction band (unused by the passes; kept for the layout).
    pub friction_band: f32,
    /// 1 to repair clearly crossed layers.
    pub layer_repair: u32,
    /// Distance difference that counts as a clear layer crossing, mm.
    pub repair_margin: f32,
    /// 1 when the body moves: the body pass reads its surface velocity (`BVEL`).
    pub body_motion: u32,
    /// Friction model: 0 Coulomb (sliding), 1 slip fraction (grip), 2 hybrid.
    pub friction_mode: u32,
    /// Cap on the cloth-on-cloth friction coefficient.
    pub mu_cap: f32,
}

/// One distance-constraint row (WGSL `Con`): stretch (kind 0), bending (kind 1) or fastener
/// (kind 2).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ConRow {
    /// First particle.
    pub i: u32,
    /// Second particle.
    pub j: u32,
    /// Rest length, mm.
    pub rest: f32,
    /// Compliance.
    pub compliance: f32,
    /// Compliance multiplier under compression.
    pub compression: f32,
    /// Kind.
    pub kind: u32,
    /// Padding.
    pub pad: [u32; 2],
}

/// Constraint kind of stretch rows.
pub const KIND_STRETCH: u32 = 0;
/// Constraint kind of bending rows.
pub const KIND_BEND: u32 = 1;
/// Constraint kind of fastener rows.
pub const KIND_FASTENER: u32 = 2;
/// Rest gap of fastener rows, mm.
pub const FASTENER_GAP_MM: f32 = 2.0;

/// One dispatch descriptor (WGSL `DP`): a range of a constraint buffer, or the apply flag.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct DispatchDesc {
    /// First row.
    pub offset: u32,
    /// Rows.
    pub count: u32,
    /// Unused.
    pub kind: u32,
    /// 1: the apply pass also moves the substep start.
    pub flag: u32,
}

/// Rounds a byte size up to whole 16-byte rows, at least one.
#[must_use]
pub fn padded(size: u64) -> u64 {
    size.div_ceil(16).max(1) * 16
}

/// A position correction in the accumulators' fixed point (as the shaders round it).
#[cfg(test)]
#[must_use]
pub fn to_fixed(d: f32) -> i32 {
    (d * FIXED_POINT_SCALE).round() as i32
}

/// The mean correction of `count` accumulated fixed-point values summing to `sum` (as the apply
/// pass decodes it).
#[cfg(test)]
#[must_use]
pub fn from_fixed(sum: i32, count: i32) -> f32 {
    sum as f32 / (FIXED_POINT_SCALE * count as f32)
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "the layout test checks exact bit patterns")]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_the_shader_structs() {
        assert_eq!(std::mem::size_of::<FrameParams>(), 176);
        assert_eq!(std::mem::size_of::<ConRow>(), 32);
        assert_eq!(std::mem::size_of::<DispatchDesc>(), 16);
        // spot-check field offsets against the WGSL member order
        let f = FrameParams {
            n: 7,
            mu_cap: 0.5,
            grab_point: [1.0, 2.0, 3.0],
            ..FrameParams::default()
        };
        let words: &[u32] = bytemuck::cast_slice(std::slice::from_ref(&f));
        assert_eq!(words[15], 7);
        assert_eq!(f32::from_bits(words[43]), 0.5);
        assert_eq!(f32::from_bits(words[29]), 2.0);
    }

    #[test]
    fn fixed_point_round_trips_within_half_a_step() {
        for d in [0.0_f32, 1.0, -2.5, 0.123_456, -17.031_25, 1e-5] {
            let back = from_fixed(to_fixed(d), 1);
            assert!((back - d).abs() <= 0.5 / FIXED_POINT_SCALE, "{d} -> {back}");
        }
        // three contributions averaged
        let parts = [0.25_f32, -0.75, 1.5];
        let sum: i32 = parts.iter().map(|&d| to_fixed(d)).sum();
        assert!((from_fixed(sum, 3) - (0.25 - 0.75 + 1.5) / 3.0).abs() < 1e-6);
    }
}
