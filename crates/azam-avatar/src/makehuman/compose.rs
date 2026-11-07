//! Composing body positions from morph-target weights.

use std::collections::HashMap;

use crate::math::clamp;
use crate::measurements::{FIT_TARGET_COUNT, FIT_TARGETS};

use super::parse::{MhModel, MorphTarget};

/// Morph-target weights of a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MhShape {
    /// 0 = female, 1 = male.
    pub gender: f64,
    /// Muscle 0..1 (0.5 average).
    pub muscle: f64,
    /// Weight 0..1 (0.5 average).
    pub weight: f64,
    /// Weight of each [`FIT_TARGETS`] entry, -1.6..1.6; positive uses the `-incr` target, negative
    /// the `-decr` one.
    pub measures: [f64; FIT_TARGET_COUNT],
    /// Strength of the bust smoothing targets (flattens nipple detail finer than the cloth).
    pub smooth: f64,
}

impl Default for MhShape {
    fn default() -> Self {
        Self {
            gender: 0.0,
            muscle: 0.5,
            weight: 0.5,
            measures: [0.0; FIT_TARGET_COUNT],
            smooth: 1.0,
        }
    }
}

/// Weights of the minimum, average and maximum variants for a 0..1 slider.
fn tri_weights(v: f64) -> [f64; 3] {
    let mx = (v * 2.0 - 1.0).max(0.0);
    let mn = (1.0 - v * 2.0).max(0.0);
    [mn, 1.0 - (mx + mn), mx]
}

const MUSCLE_NAMES: [&str; 3] = ["minmuscle", "averagemuscle", "maxmuscle"];
const WEIGHT_NAMES: [&str; 3] = ["minweight", "averageweight", "maxweight"];

/// Adds weighted targets onto base positions, rounding to `f32` after every addition.
struct Composer<'a> {
    out: Vec<f32>,
    targets: &'a HashMap<String, MorphTarget>,
    quantum: f64,
}

impl Composer<'_> {
    #[allow(clippy::float_cmp)]
    fn add(&mut self, name: &str, f: f64) {
        if f == 0.0 || f.is_nan() {
            return;
        }
        let Some(t) = self.targets.get(name) else {
            return;
        };
        let s = self.quantum * f;
        for (e, &i) in t.indices.iter().enumerate() {
            let k = i as usize * 3;
            for c in 0..3 {
                self.out[k + c] = (f64::from(self.out[k + c]) + f64::from(t.deltas[e * 3 + c]) * s) as f32;
            }
        }
    }
}

impl MhModel {
    /// Full body positions (`x, y, z` per vertex, unscaled and ungrounded) for `shape`.
    #[must_use]
    pub fn compose(&self, shape: &MhShape) -> Vec<f32> {
        compose_with(&self.base, &self.targets, self.quantum, shape)
    }

    /// Positions of the fitting probe vertices only.
    pub(crate) fn compose_probe(&self, shape: &MhShape) -> Vec<f32> {
        compose_with(&self.probe.base, &self.probe.targets, self.quantum, shape)
    }
}

#[allow(clippy::float_cmp)]
fn compose_with(
    base: &[f32],
    targets: &HashMap<String, MorphTarget>,
    quantum: f64,
    shape: &MhShape,
) -> Vec<f32> {
    let mut c = Composer {
        out: base.to_vec(),
        targets,
        quantum,
    };
    let g = clamp(shape.gender, 0.0, 1.0);
    c.add("g_female", 1.0 - g);
    c.add("g_male", g);
    let mu = tri_weights(clamp(shape.muscle, 0.0, 1.0));
    let we = tri_weights(clamp(shape.weight, 0.0, 1.0));
    for (gender, gw) in [("female", 1.0 - g), ("male", g)] {
        if gw == 0.0 {
            continue;
        }
        for (i, m) in MUSCLE_NAMES.iter().enumerate() {
            for (j, w) in WEIGHT_NAMES.iter().enumerate() {
                c.add(&format!("universal-{gender}-young-{m}-{w}"), gw * mu[i] * we[j]);
            }
        }
    }
    for (name, &w) in FIT_TARGETS.iter().zip(&shape.measures) {
        if w != 0.0 {
            c.add(
                &format!("{name}{}", if w > 0.0 { "-incr" } else { "-decr" }),
                w.abs(),
            );
        }
    }
    if shape.smooth != 0.0 {
        c.add("nipple-point-decr", shape.smooth);
        c.add("nipple-size-decr", shape.smooth);
    }
    c.out
}
