//! Settings of the GPU contact passes that the shared solver parameters do not carry.

/// Contact settings of the GPU solver. The defaults are the reference solver's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuOptions {
    /// Extra contact search distance beyond the cloth thickness and 2 mm, in mm.
    pub contact_slack: f64,
    /// Fix the contact side between layers by layer order (the higher layer stays outside along
    /// the body normal) instead of keeping the side each particle came from.
    pub layer_order: bool,
    /// Between layers, contact the closest point of a triangle (edges and corners too) instead
    /// of its face region only.
    pub edge_contacts: bool,
    /// How much nearer the body (mm) an outer-layer particle must be than the inner surface it
    /// touches before the crossing is repaired (with [`azam_sim::SimParams::cc_repair`]).
    pub repair_margin: f64,
    /// Friction band (carried in the frame parameters; no pass reads it).
    pub friction_band: f64,
}

impl Default for GpuOptions {
    fn default() -> Self {
        Self {
            contact_slack: 8.0,
            layer_order: false,
            edge_contacts: false,
            repair_margin: 8.0,
            friction_band: 1.5,
        }
    }
}
