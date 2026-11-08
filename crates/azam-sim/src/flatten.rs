//! Flattening a draped piece (CLO's 3D-to-2D): its mesh laid flat again so that every edge keeps
//! the length it has on the body. Where the cloth stretched the flat shape grows, where it eased
//! it shrinks, so the result is the pattern the drape implies.

use azam_geom::Vec2;

use crate::output::PieceView;

/// Relaxation sweeps over the edges.
const SWEEPS: usize = 300;

/// The vertices of `view` laid flat: each edge (pattern-space mesh edges) takes its length in 3D,
/// starting from the piece's pattern and relaxing edge by edge, then turned and moved to sit best
/// over the pattern (so it compares in place). One point per mesh vertex, in pattern mm.
#[must_use]
pub fn flatten_piece(view: &PieceView) -> Vec<Vec2> {
    let mesh = &view.record.mesh;
    let n = view.record.count;
    let flat: Vec<Vec2> = (0..n)
        .map(|i| Vec2::new(mesh.xy[2 * i], mesh.xy[2 * i + 1]))
        .collect();
    let p3 = |i: usize| {
        glam::DVec3::new(
            f64::from(view.positions[3 * i]),
            f64::from(view.positions[3 * i + 1]),
            f64::from(view.positions[3 * i + 2]),
        )
    };
    let edges: Vec<(usize, usize, f64)> = view
        .record
        .topology
        .edges
        .iter()
        .map(|e| {
            (
                e.a as usize,
                e.b as usize,
                p3(e.a as usize).distance(p3(e.b as usize)),
            )
        })
        .collect();
    let mut x = flat.clone();
    for _ in 0..SWEEPS {
        for &(a, b, rest) in &edges {
            let d = x[b] - x[a];
            let len = d.length();
            if len < 1e-9 {
                continue;
            }
            let push = d * (0.5 * (len - rest) / len);
            x[a] += push;
            x[b] -= push;
        }
    }
    fit_onto(&x, &flat)
}

/// `points` turned and moved to lie best over `target` (least squares, no scaling).
fn fit_onto(points: &[Vec2], target: &[Vec2]) -> Vec<Vec2> {
    let n = points.len().max(1) as f64;
    let (cp, ct) = (
        points.iter().copied().sum::<Vec2>() / n,
        target.iter().copied().sum::<Vec2>() / n,
    );
    let (mut sxx, mut sxy) = (0.0, 0.0);
    for (p, t) in points.iter().zip(target) {
        let (a, b) = (*p - cp, *t - ct);
        sxx += a.x * b.x + a.y * b.y;
        sxy += a.x * b.y - a.y * b.x;
    }
    let angle = sxy.atan2(sxx);
    let (s, c) = angle.sin_cos();
    points
        .iter()
        .map(|p| {
            let a = *p - cp;
            ct + Vec2::new(a.x * c - a.y * s, a.x * s + a.y * c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::fit_onto;
    use azam_geom::Vec2;

    #[test]
    fn a_turned_copy_fits_back_onto_its_original() {
        let target = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(0.0, 5.0)];
        let turned: Vec<Vec2> = target
            .iter()
            .map(|p| Vec2::new(-p.y + 100.0, p.x - 3.0))
            .collect();
        let back = fit_onto(&turned, &target);
        for (b, t) in back.iter().zip(&target) {
            assert!(b.distance(*t) < 1e-9);
        }
    }
}
