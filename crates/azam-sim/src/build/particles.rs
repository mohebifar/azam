//! Per-piece material constants and per-particle properties (masses, damping, clearances).

use azam_doc::{FabricPhysics, Piece, PieceState, Project, r_bend, r_stretch};

use crate::numeric::{clamp, or_else};
use crate::state::{Particles, PieceRecord};

/// Compliance multiplier of an interfaced (strengthened) piece.
const STRENGTHEN_FACTOR: f64 = 0.02;
/// Buckling ratio used when a fabric gives none.
const DEFAULT_BUCKLING: f64 = 50.0;
/// How much softer than stretching a fully buckling fabric is under compression.
const BUCKLING_SPAN: f64 = 24.0;
/// Density used when a fabric gives none, in g/m².
const DEFAULT_DENSITY_G_M2: f64 = 150.0;
/// g/m² to g/mm².
const PER_M2_TO_PER_MM2: f64 = 1e-6;
/// Thickness used when a fabric gives none, in mm.
const DEFAULT_THICKNESS_MM: f64 = 0.5;
/// Damping rate of a fabric with internal damping 0, per second.
const BASE_DAMPING: f64 = 0.4;
/// Extra damping rate per unit of the 0–1 internal damping, per second.
const DAMPING_SPAN: f64 = 3.0;
/// Body clearance of every particle, before thickness and layer, in mm.
const BASE_MARGIN_MM: f64 = 3.0;
/// Extra clearance per layer, on top of one fabric thickness, in mm.
const LAYER_MARGIN_MM: f64 = 3.0;
/// Particle collision radius as a fraction of the mesh spacing.
const RADIUS_FACTOR: f64 = 0.42;
/// Collision radius limits, in mm.
const RADIUS_RANGE_MM: (f64, f64) = (3.0, 10.0);
/// Largest friction coefficient.
const MAX_FRICTION: f64 = 0.9;
/// Shrinkage stored as a percentage; 100 means none.
const NO_SHRINK_PERCENT: f64 = 100.0;

/// Constants of one piece's cloth.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Material {
    /// Stretch compliance across, along and on the bias of the grain.
    pub weft: f64,
    pub warp: f64,
    pub shear: f64,
    /// Bending compliance across and along the grain.
    pub bend_weft: f64,
    pub bend_warp: f64,
    /// Rest-length factors across and along the grain.
    pub shrink_weft: f64,
    pub shrink_warp: f64,
    /// Compliance multiplier under compression.
    pub compression: f64,
    /// Areal density, g/mm².
    pub density: f64,
    /// Thickness, mm.
    pub thickness: f64,
    /// Damping rate, per second.
    pub damping: f64,
    /// Friction coefficient.
    pub friction: f64,
    /// Grain direction (unit vector in the pattern frame).
    pub grain: (f64, f64),
    /// Layer of the piece.
    pub layer: i32,
    /// Inflation pressure.
    pub pressure: f64,
}

fn extra_number(piece: &Piece, key: &str) -> f64 {
    piece
        .extra
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
}

/// The fabric physics of `piece` (poplin when its fabric is missing).
pub(crate) fn physics(project: &Project, piece: &Piece) -> FabricPhysics {
    project
        .fabrics
        .get(&piece.fabric)
        .map(|f| f.phys.clone())
        .unwrap_or_default()
}

impl Material {
    pub(crate) fn new(project: &Project, piece: &Piece) -> Self {
        let ph = physics(project, piece);
        let strong = if piece.strengthen == Some(true) {
            STRENGTHEN_FACTOR
        } else {
            1.0
        };
        let g = piece.grain * azam_avatar::DEG;
        Self {
            weft: r_stretch(ph.stretch_weft) * strong,
            warp: r_stretch(ph.stretch_warp) * strong,
            shear: r_stretch(ph.shear) * strong,
            bend_weft: r_bend(ph.bend_weft) * strong,
            bend_warp: r_bend(ph.bend_warp) * strong,
            shrink_weft: or_else(extra_number(piece, "shrinkWeft"), NO_SHRINK_PERCENT) / NO_SHRINK_PERCENT,
            shrink_warp: or_else(extra_number(piece, "shrinkWarp"), NO_SHRINK_PERCENT) / NO_SHRINK_PERCENT,
            compression: 1.0 + nan_or(ph.buckling, DEFAULT_BUCKLING) / 100.0 * BUCKLING_SPAN,
            density: or_else(ph.density, DEFAULT_DENSITY_G_M2) * PER_M2_TO_PER_MM2,
            thickness: or_else(ph.thickness, DEFAULT_THICKNESS_MM),
            damping: BASE_DAMPING + ph.damping / 100.0 * DAMPING_SPAN,
            friction: clamp(ph.friction / 100.0, 0.0, MAX_FRICTION),
            grain: (g.cos(), g.sin()),
            layer: piece.layer,
            pressure: extra_number(piece, "pressure"),
        }
    }
}

fn nan_or(x: f64, fallback: f64) -> f64 {
    if x.is_nan() { fallback } else { x }
}

/// Particles with room for `n` particles, all zero.
pub(crate) fn allocate(n: usize) -> Particles {
    Particles {
        x: vec![0.0; n * 3],
        prev: vec![0.0; n * 3],
        v: vec![0.0; n * 3],
        inv_mass: vec![0.0; n],
        inv_mass_free: vec![0.0; n],
        damping: vec![0.0; n],
        margin: vec![0.0; n],
        radius: vec![0.0; n],
        friction: vec![0.0; n],
        layer: vec![0; n],
        spacing: vec![0.0; n],
        thickness: vec![0.0; n],
        piece: vec![0; n],
        active: vec![false; n],
        pressure: vec![0.0; n],
        sewn: vec![false; n],
    }
}

/// Lumped vertex masses of a mesh: each triangle's mass split evenly between its corners.
fn lumped_masses(xy: &[f64], tris: &[u32], n: usize, density: f64) -> Vec<f32> {
    let mut mass = vec![0.0_f32; n];
    for t in tris.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let area = ((xy[2 * b] - xy[2 * a]) * (xy[2 * c + 1] - xy[2 * a + 1])
            - (xy[2 * b + 1] - xy[2 * a + 1]) * (xy[2 * c] - xy[2 * a]))
            .abs()
            / 2.0;
        let share = area * density / 3.0;
        for v in [a, b, c] {
            mass[v] = (f64::from(mass[v]) + share) as f32;
        }
    }
    mass
}

/// Writes the per-particle properties of record `ri`.
pub(crate) fn fill(p: &mut Particles, ri: usize, record: &PieceRecord, m: &Material) {
    let mesh = &record.mesh;
    let mass = lumped_masses(&mesh.xy, &mesh.tris, record.count, m.density);
    let active = record.state != PieceState::Off;
    let free = record.state == PieceState::Active;
    let margin = BASE_MARGIN_MM + m.thickness * 0.5 + f64::from(m.layer) * (m.thickness + LAYER_MARGIN_MM);
    let radius = clamp(mesh.spacing * RADIUS_FACTOR, RADIUS_RANGE_MM.0, RADIUS_RANGE_MM.1);
    for (i, &mi) in mass.iter().enumerate() {
        let q = record.offset + i;
        let w = if free && mi > 0.0 {
            (1.0 / f64::from(mi)) as f32
        } else {
            0.0
        };
        p.inv_mass[q] = w;
        p.inv_mass_free[q] = w;
        p.damping[q] = m.damping as f32;
        p.margin[q] = margin as f32;
        p.piece[q] = ri as u32;
        p.active[q] = active;
        p.radius[q] = radius as f32;
        p.layer[q] = m.layer as i8;
        p.spacing[q] = mesh.spacing as f32;
        p.thickness[q] = m.thickness as f32;
        p.friction[q] = m.friction as f32;
        p.pressure[q] = m.pressure as f32;
    }
}
