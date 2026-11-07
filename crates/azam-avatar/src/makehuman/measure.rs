//! Reading tailor measurements off a composed body.

use glam::DVec3;

use crate::math::{hypot2, hypot3};
use crate::measurements::{
    LOWER_ARM_LENGTH, MEASUREMENTS, MeasureKind, MeasurementDef, Measurements, UPPER_ARM_LENGTH,
};

use super::parse::MhModel;

/// A tape laid around a vertex loop.
#[derive(Clone, Debug, PartialEq)]
pub struct Tape {
    /// Tape length: perimeter of the loop's convex hull in its best-fit plane.
    pub length: f64,
    /// Hull corners in 3D, in order.
    pub hull: Vec<DVec3>,
    /// Centroid of the loop.
    pub center: DVec3,
}

/// Measures `points` the way a tape measure reads a girth: projects the loop onto its own plane
/// (Newell normal) and returns the perimeter of the convex hull.
#[must_use]
pub fn tape_measure(points: &[DVec3]) -> Tape {
    let n = points.len();
    let count = n as f64;
    let mut c = DVec3::ZERO;
    for p in points {
        c.x += p.x / count;
        c.y += p.y / count;
        c.z += p.z / count;
    }
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        nx += (a.y - b.y) * (a.z + b.z);
        ny += (a.z - b.z) * (a.x + b.x);
        nz += (a.x - b.x) * (a.y + b.y);
    }
    let mut len = hypot3(nx, ny, nz);
    if len < 1e-9 {
        (nx, ny, nz, len) = (0.0, 1.0, 0.0, 1.0);
    }
    let normal = DVec3::new(nx / len, ny / len, nz / len);
    let reference = if normal.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
    let e1 = normalize(cross(reference, normal));
    let e2 = cross(normal, e1);
    let dot = |d: DVec3, e: DVec3| d.x * e.x + d.y * e.y + d.z * e.z;
    let mut q: Vec<[f64; 2]> = points
        .iter()
        .map(|p| {
            let d = DVec3::new(p.x - c.x, p.y - c.y, p.z - c.z);
            [dot(d, e1), dot(d, e2)]
        })
        .collect();
    q.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    let turn =
        |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let half_hull = |pts: &mut dyn Iterator<Item = [f64; 2]>| {
        let mut h: Vec<[f64; 2]> = Vec::new();
        for p in pts {
            while h.len() >= 2 && turn(h[h.len() - 2], h[h.len() - 1], p) <= 0.0 {
                h.pop();
            }
            h.push(p);
        }
        h.pop();
        h
    };
    let mut hull = half_hull(&mut q.iter().copied());
    hull.extend(half_hull(&mut q.iter().rev().copied()));
    let length = (0..hull.len())
        .map(|i| {
            let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
            hypot2(a[0] - b[0], a[1] - b[1])
        })
        .fold(0.0, |s, l| s + l);
    let hull = hull
        .iter()
        .map(|h| {
            DVec3::new(
                c.x + e1.x * h[0] + e2.x * h[1],
                c.y + e1.y * h[0] + e2.y * h[1],
                c.z + e1.z * h[0] + e2.z * h[1],
            )
        })
        .collect();
    Tape {
        length,
        hull,
        center: c,
    }
}

pub(crate) fn cross(a: DVec3, b: DVec3) -> DVec3 {
    DVec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

pub(crate) fn normalize(a: DVec3) -> DVec3 {
    let l = hypot3(a.x, a.y, a.z);
    let l = if l > 0.0 { l } else { 1.0 };
    DVec3::new(a.x / l, a.y / l, a.z / l)
}

/// Composed positions of either the whole model or only its fitting probe.
#[derive(Clone, Copy)]
pub(crate) enum Positions<'a> {
    /// One entry per model vertex.
    Full(&'a [f32]),
    /// One entry per probe vertex.
    Probe(&'a [f32]),
}

impl MhModel {
    /// Position of model vertex `g`.
    pub(crate) fn vertex(&self, pos: Positions<'_>, g: u32) -> DVec3 {
        let (p, i) = match pos {
            Positions::Full(p) => (p, g as usize),
            Positions::Probe(p) => (p, self.probe.local[g as usize].max(0) as usize),
        };
        DVec3::new(
            f64::from(p[i * 3]),
            f64::from(p[i * 3 + 1]),
            f64::from(p[i * 3 + 2]),
        )
    }

    /// Lowest and highest body vertex height.
    pub(crate) fn height_range(&self, pos: Positions<'_>) -> [f64; 2] {
        let (mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut see = |y: f32| {
            let y = f64::from(y);
            if y < y0 {
                y0 = y;
            }
            if y > y1 {
                y1 = y;
            }
        };
        match pos {
            Positions::Probe(p) => {
                for (i, &body) in self.probe.body.iter().enumerate() {
                    if body {
                        see(p[i * 3 + 1]);
                    }
                }
            }
            Positions::Full(p) => {
                for v in 0..self.body_vertex_count {
                    see(p[v * 3 + 1]);
                }
            }
        }
        [y0, y1]
    }

    fn distance(&self, pos: Positions<'_>, a: u32, b: u32) -> f64 {
        let (p, q) = (self.vertex(pos, a), self.vertex(pos, b));
        hypot3(p.x - q.x, p.y - q.y, p.z - q.z)
    }

    fn pair_distance(&self, pos: Positions<'_>, name: &str) -> f64 {
        let l = self.measure_vertices(name);
        self.distance(pos, l[0], l[1])
    }

    /// Points of a named vertex loop.
    pub(crate) fn loop_points(&self, pos: Positions<'_>, name: &str) -> Vec<DVec3> {
        self.measure_vertices(name)
            .iter()
            .map(|&g| self.vertex(pos, g))
            .collect()
    }

    /// One measurement; `range` is the body height range.
    pub(crate) fn measure_one(&self, pos: Positions<'_>, def: &MeasurementDef, range: [f64; 2]) -> f64 {
        match def.kind {
            MeasureKind::Height => range[1] - range[0],
            MeasureKind::Circumference(name) => tape_measure(&self.loop_points(pos, name)).length,
            MeasureKind::Distance(name) => self.pair_distance(pos, name),
            MeasureKind::Shoulder => {
                self.distance(pos, self.landmark("shoulderL"), self.landmark("shoulderR"))
            }
            MeasureKind::Arm => {
                self.pair_distance(pos, UPPER_ARM_LENGTH) + self.pair_distance(pos, LOWER_ARM_LENGTH)
            }
            MeasureKind::Crotch => self.vertex(pos, self.landmark("crotch")).y - range[0],
        }
    }

    /// Every tailor measurement of the full body `positions` (`x, y, z` per vertex).
    ///
    /// # Panics
    ///
    /// Panics if `positions` holds fewer than `3 * vertex_count` values.
    #[must_use]
    pub fn measure_all(&self, positions: &[f32]) -> Measurements {
        assert!(
            positions.len() >= self.vertex_count * 3,
            "positions do not cover the model"
        );
        self.measure_all_in(Positions::Full(positions))
    }

    pub(crate) fn measure_all_in(&self, pos: Positions<'_>) -> Measurements {
        let range = self.height_range(pos);
        Measurements(MEASUREMENTS.map(|d| self.measure_one(pos, &d, range)))
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn tape_reads_the_convex_hull_of_a_dented_loop() {
        // a square with a dent: the tape bridges it
        let pts: Vec<DVec3> = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [5.0, 8.0], [0.0, 10.0]]
            .iter()
            .map(|p| DVec3::new(p[0], 100.0, p[1]))
            .collect();
        let t = tape_measure(&pts);
        assert!((t.length - 40.0).abs() < 1e-9, "{}", t.length);
        assert_eq!(t.hull.len(), 4);
        assert!((t.center.y - 100.0).abs() < 1e-12);
    }

    #[test]
    fn tape_of_a_circle_is_its_perimeter() {
        let n = 720;
        let pts: Vec<DVec3> = (0..n)
            .map(|i| {
                let a = f64::from(i) / f64::from(n) * std::f64::consts::TAU;
                DVec3::new(50.0 * a.cos(), 3.0 + 0.1 * a.cos(), 50.0 * a.sin())
            })
            .collect();
        let t = tape_measure(&pts);
        assert!(
            (t.length - std::f64::consts::TAU * 50.0).abs() < 0.05,
            "{}",
            t.length
        );
    }
}
