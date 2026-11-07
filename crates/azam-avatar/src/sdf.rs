//! Signed distance primitives of the parametric body. Distances are negative inside.

use glam::DVec3;

use crate::math::{sign, smooth_min};

/// A capsule whose radius changes linearly from `r1` at `a` to `r2` at `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundCone {
    /// Centre of the first end sphere.
    pub a: DVec3,
    /// Centre of the second end sphere.
    pub b: DVec3,
    /// Radius at `a`.
    pub r1: f64,
    /// Radius at `b`.
    pub r2: f64,
}

impl RoundCone {
    /// Exact signed distance from `p`.
    #[must_use]
    pub fn distance(&self, p: DVec3) -> f64 {
        let (ba_x, ba_y, ba_z) = (self.b.x - self.a.x, self.b.y - self.a.y, self.b.z - self.a.z);
        let l2 = ba_x * ba_x + ba_y * ba_y + ba_z * ba_z;
        let rr = self.r1 - self.r2;
        let a2 = l2 - rr * rr;
        let il2 = 1.0 / l2;
        let (pax, pay, paz) = (p.x - self.a.x, p.y - self.a.y, p.z - self.a.z);
        let y = pax * ba_x + pay * ba_y + paz * ba_z;
        let z = y - l2;
        let (qx, qy, qz) = (pax * l2 - ba_x * y, pay * l2 - ba_y * y, paz * l2 - ba_z * y);
        let x2 = qx * qx + qy * qy + qz * qz;
        let y2 = y * y * l2;
        let z2 = z * z * l2;
        let k = sign(rr) * rr * rr * x2;
        if sign(z) * a2 * z2 > k {
            return (x2 + z2).sqrt() * il2 - self.r2;
        }
        if sign(y) * a2 * y2 < k {
            return (x2 + y2).sqrt() * il2 - self.r1;
        }
        ((x2 * a2 * il2).sqrt() + y * rr) * il2 - self.r1
    }
}

/// An axis-aligned ellipsoid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ellipsoid {
    /// Centre.
    pub center: DVec3,
    /// Semi-axes along x, y and z.
    pub radii: DVec3,
}

impl Ellipsoid {
    /// Approximate signed distance from `p` (exact on the surface, a bound elsewhere).
    #[must_use]
    pub fn distance(&self, p: DVec3) -> f64 {
        let (x, y, z) = (p.x - self.center.x, p.y - self.center.y, p.z - self.center.z);
        let (rx, ry, rz) = (self.radii.x, self.radii.y, self.radii.z);
        let k0 = ((x / rx) * (x / rx) + (y / ry) * (y / ry) + (z / rz) * (z / rz)).sqrt();
        let k1 = ((x / (rx * rx)) * (x / (rx * rx))
            + (y / (ry * ry)) * (y / (ry * ry))
            + (z / (rz * rz)) * (z / (rz * rz)))
            .sqrt();
        if k1 > 1e-9 {
            k0 * (k0 - 1.0) / k1
        } else {
            -rx.min(ry).min(rz)
        }
    }
}

/// Shape of a body primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Limb segment or neck.
    RoundCone(RoundCone),
    /// Hand, foot or head.
    Ellipsoid(Ellipsoid),
}

/// Extra margin around a primitive's blend zone that its bounding box covers, in millimetres.
const BOUNDS_MARGIN_MM: f64 = 12.0;

/// A primitive blended into the body with a smooth minimum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPrimitive {
    /// The shape.
    pub shape: Shape,
    /// Smooth-minimum blend radius in millimetres.
    pub blend: f64,
    /// Box `[x0, y0, z0, x1, y1, z1]` outside which the primitive cannot change the body distance.
    pub bounds: [f64; 6],
}

impl BodyPrimitive {
    /// A round cone with blend radius `blend`.
    #[must_use]
    pub fn round_cone(cone: RoundCone, blend: f64) -> Self {
        let r = cone.r1.max(cone.r2) + blend + BOUNDS_MARGIN_MM;
        let (a, b) = (cone.a, cone.b);
        let bounds = [
            a.x.min(b.x) - r,
            a.y.min(b.y) - r,
            a.z.min(b.z) - r,
            a.x.max(b.x) + r,
            a.y.max(b.y) + r,
            a.z.max(b.z) + r,
        ];
        Self {
            shape: Shape::RoundCone(cone),
            blend,
            bounds,
        }
    }

    /// An ellipsoid with blend radius `blend`.
    #[must_use]
    pub fn ellipsoid(ellipsoid: Ellipsoid, blend: f64) -> Self {
        let r = blend + BOUNDS_MARGIN_MM;
        let (c, e) = (ellipsoid.center, ellipsoid.radii);
        let bounds = [
            c.x - e.x - r,
            c.y - e.y - r,
            c.z - e.z - r,
            c.x + e.x + r,
            c.y + e.y + r,
            c.z + e.z + r,
        ];
        Self {
            shape: Shape::Ellipsoid(ellipsoid),
            blend,
            bounds,
        }
    }

    /// Signed distance of the shape alone.
    #[must_use]
    pub fn distance(&self, p: DVec3) -> f64 {
        match &self.shape {
            Shape::RoundCone(c) => c.distance(p),
            Shape::Ellipsoid(e) => e.distance(p),
        }
    }

    /// Blends this primitive into the body distance `d` at `p`.
    #[must_use]
    pub fn blend_into(&self, d: f64, p: DVec3) -> f64 {
        let b = &self.bounds;
        if p.x < b[0] || p.y < b[1] || p.z < b[2] || p.x > b[3] || p.y > b[4] || p.z > b[5] {
            return d;
        }
        smooth_min(d, self.distance(p), self.blend)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn round_cone_is_zero_on_its_end_spheres() {
        let c = RoundCone {
            a: DVec3::ZERO,
            b: DVec3::new(0.0, 100.0, 0.0),
            r1: 20.0,
            r2: 10.0,
        };
        assert!(c.distance(DVec3::new(0.0, -20.0, 0.0)).abs() < 1e-9);
        assert!(c.distance(DVec3::new(0.0, 110.0, 0.0)).abs() < 1e-9);
        assert!(c.distance(DVec3::new(0.0, 50.0, 0.0)) < 0.0);
    }

    #[test]
    fn ellipsoid_is_zero_on_its_surface() {
        let e = Ellipsoid {
            center: DVec3::ONE,
            radii: DVec3::new(10.0, 20.0, 30.0),
        };
        assert!(e.distance(DVec3::new(1.0, 21.0, 1.0)).abs() < 1e-9);
        assert_eq!(e.distance(DVec3::ONE), -10.0);
    }
}
