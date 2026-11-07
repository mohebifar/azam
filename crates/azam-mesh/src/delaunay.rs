//! Delaunay triangulation of the mesh vertices.
//!
//! A line-for-line port of Delaunator 5.0.1 (ISC licence, copyright Mapbox): a sweep-hull
//! algorithm that inserts points in order of distance from a seed triangle and legalises edges by
//! flipping. Triangle order, vertex order within each triangle and the choice among cocircular
//! configurations all follow the original exactly, so meshes match the recorded reference data
//! index for index. The `delaunator` crate produces the same triangulations in a different order
//! (it sizes its hull hash and sorts its sweep order differently), which is why it is not used.

use azam_geom::Vec2;
use robust::Coord;

/// Points closer than this in both x and y to the previously inserted point are skipped.
const DUPLICATE_EPSILON: f64 = f64::EPSILON;
/// Capacity of the edge stack used while legalising; edges beyond it are not re-checked. This is
/// only reachable for extremely degenerate input.
const EDGE_STACK_CAPACITY: usize = 512;
/// Partitions of at most this many elements are insertion-sorted.
const INSERTION_SORT_MAX: usize = 20;
/// Marks a missing half-edge or hash entry.
const NONE: usize = usize::MAX;

/// Delaunay-triangulates `points` and returns the triangles as index triples, each with a
/// negative cross product `(b - a) × (c - a)` (counter-clockwise on screen, y down).
///
/// Fewer than three distinct points, all points on one line, or any non-finite coordinate give no
/// triangles.
pub(crate) fn triangulate(points: &[Vec2]) -> Vec<usize> {
    if !points.iter().all(|p| p.is_finite()) {
        return Vec::new();
    }
    let Some(seed) = seed_triangle(points) else {
        return Vec::new();
    };
    let mut sweep = Sweep::new(points, seed);
    sweep.run();
    sweep.triangles
}

/// Orientation of `r` relative to the line `p` → `q`: positive when `p, q, r` turn clockwise in a
/// y-up frame, negative counter-clockwise, exactly zero when collinear.
fn orient(p: Vec2, q: Vec2, r: Vec2) -> f64 {
    -robust::orient2d(coord(p), coord(q), coord(r))
}

fn coord(p: Vec2) -> Coord<f64> {
    Coord { x: p.x, y: p.y }
}

fn dist2(a: Vec2, b: Vec2) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// Offset from `a` to the circumcentre of `a, b, c`.
fn circumdelta(a: Vec2, b: Vec2, c: Vec2) -> Vec2 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let ex = c.x - a.x;
    let ey = c.y - a.y;
    let bl = dx * dx + dy * dy;
    let cl = ex * ex + ey * ey;
    let d = 0.5 / (dx * ey - dy * ex);
    Vec2::new((ey * bl - dy * cl) * d, (dx * cl - ex * bl) * d)
}

fn circumradius2(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let v = circumdelta(a, b, c);
    v.x * v.x + v.y * v.y
}

fn circumcenter(a: Vec2, b: Vec2, c: Vec2) -> Vec2 {
    let v = circumdelta(a, b, c);
    Vec2::new(a.x + v.x, a.y + v.y)
}

/// Whether `p` lies inside the circumcircle of `a, b, c` (not exact; as in the original).
fn in_circle(a: Vec2, b: Vec2, c: Vec2, p: Vec2) -> bool {
    let dx = a.x - p.x;
    let dy = a.y - p.y;
    let ex = b.x - p.x;
    let ey = b.y - p.y;
    let fx = c.x - p.x;
    let fy = c.y - p.y;
    let ap = dx * dx + dy * dy;
    let bp = ex * ex + ey * ey;
    let cp = fx * fx + fy * fy;
    dx * (ey * cp - bp * fy) - dy * (ex * cp - bp * fx) + ap * (ex * fy - ey * fx) < 0.0
}

/// Monotonic in the angle of `(dx, dy)`, in `[0, 1]`, without trigonometry.
fn pseudo_angle(dx: f64, dy: f64) -> f64 {
    let p = dx / (dx.abs() + dy.abs());
    (if dy > 0.0 { 3.0 - p } else { 1.0 + p }) / 4.0
}

/// Picks the seed triangle: the point nearest the bounding-box centre, its nearest neighbour, and
/// the point making the smallest circumcircle with them, ordered counter-clockwise (y-up).
/// `None` if the points are all coincident or collinear.
fn seed_triangle(points: &[Vec2]) -> Option<[usize; 3]> {
    let first = *points.first()?;
    let (min, max) = points
        .iter()
        .fold((first, first), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    let center = Vec2::new(min.x.midpoint(max.x), min.y.midpoint(max.y));

    let mut i0 = 0;
    let mut min_dist = f64::INFINITY;
    for (i, &p) in points.iter().enumerate() {
        let d = dist2(center, p);
        if d < min_dist {
            i0 = i;
            min_dist = d;
        }
    }
    let p0 = points[i0];

    let mut i1 = None;
    let mut min_dist = f64::INFINITY;
    for (i, &p) in points.iter().enumerate() {
        if i == i0 {
            continue;
        }
        let d = dist2(p0, p);
        if d < min_dist && d > 0.0 {
            i1 = Some(i);
            min_dist = d;
        }
    }
    let i1 = i1?;
    let p1 = points[i1];

    let mut i2 = 0;
    let mut min_radius = f64::INFINITY;
    for (i, &p) in points.iter().enumerate() {
        if i == i0 || i == i1 {
            continue;
        }
        let r = circumradius2(p0, p1, p);
        if r < min_radius {
            i2 = i;
            min_radius = r;
        }
    }
    if min_radius == f64::INFINITY {
        return None;
    }
    Some(if orient(p0, p1, points[i2]) < 0.0 {
        [i0, i2, i1]
    } else {
        [i0, i1, i2]
    })
}

/// State of the sweep: the triangles so far and the advancing convex hull.
struct Sweep<'a> {
    points: &'a [Vec2],
    /// Vertex index triples.
    triangles: Vec<usize>,
    /// Twin of each half-edge, or [`NONE`] on the hull.
    halfedges: Vec<usize>,
    /// Hull vertex before each hull vertex.
    hull_prev: Vec<usize>,
    /// Hull vertex after each hull vertex; a removed vertex points to itself.
    hull_next: Vec<usize>,
    /// Half-edge of the triangle adjacent to the hull edge starting at each hull vertex.
    hull_tri: Vec<usize>,
    /// Hull vertices by pseudo-angle around `center`, or [`NONE`].
    hull_hash: Vec<usize>,
    hull_start: usize,
    /// Circumcentre of the seed triangle.
    center: Vec2,
    /// Insertion order: point indices by distance from `center`.
    order: Vec<usize>,
    seed: [usize; 3],
    edge_stack: Vec<usize>,
}

impl<'a> Sweep<'a> {
    fn new(points: &'a [Vec2], seed: [usize; 3]) -> Self {
        let n = points.len();
        let [i0, i1, i2] = seed;
        let center = circumcenter(points[i0], points[i1], points[i2]);
        let dists: Vec<f64> = points.iter().map(|&p| dist2(p, center)).collect();
        let mut order: Vec<usize> = (0..n).collect();
        quicksort(&mut order, &dists, 0, n - 1);

        let max_triangles = (2 * n).saturating_sub(5);
        let mut sweep = Self {
            points,
            triangles: Vec::with_capacity(max_triangles * 3),
            halfedges: Vec::with_capacity(max_triangles * 3),
            hull_prev: vec![0; n],
            hull_next: vec![0; n],
            hull_tri: vec![0; n],
            hull_hash: vec![NONE; (n as f64).sqrt().ceil() as usize],
            hull_start: i0,
            center,
            order,
            seed,
            edge_stack: Vec::with_capacity(EDGE_STACK_CAPACITY),
        };

        sweep.hull_next[i0] = i1;
        sweep.hull_prev[i2] = i1;
        sweep.hull_next[i1] = i2;
        sweep.hull_prev[i0] = i2;
        sweep.hull_next[i2] = i0;
        sweep.hull_prev[i1] = i0;
        sweep.hull_tri[i0] = 0;
        sweep.hull_tri[i1] = 1;
        sweep.hull_tri[i2] = 2;
        for i in seed {
            sweep.hash_vertex(i);
        }
        sweep.add_triangle(i0, i1, i2, NONE, NONE, NONE);
        sweep
    }

    /// Inserts every point in sweep order.
    fn run(&mut self) {
        let mut previous: Option<Vec2> = None;
        for k in 0..self.order.len() {
            let i = self.order[k];
            let p = self.points[i];
            if previous.is_some_and(|q| {
                (p.x - q.x).abs() <= DUPLICATE_EPSILON && (p.y - q.y).abs() <= DUPLICATE_EPSILON
            }) {
                continue;
            }
            previous = Some(p);
            if self.seed.contains(&i) {
                continue;
            }
            self.insert(i);
        }
    }

    /// Adds point `i`, which lies outside the current hull, connecting it to every hull edge it
    /// sees.
    fn insert(&mut self, i: usize) {
        let p = self.points[i];
        let Some(start) = self.visible_search_start(p) else {
            return;
        };
        let Some(mut e) = self.first_visible_edge(p, start) else {
            return; // likely a near-duplicate point
        };

        let t = self.add_triangle(e, i, self.hull_next[e], NONE, NONE, self.hull_tri[e]);
        self.hull_tri[i] = self.legalize(t + 2);
        self.hull_tri[e] = t;

        // Walk forward through the hull, adding triangles while the edges are visible.
        let mut n = self.hull_next[e];
        loop {
            let q = self.hull_next[n];
            if orient(p, self.points[n], self.points[q]) >= 0.0 {
                break;
            }
            let t = self.add_triangle(n, i, q, self.hull_tri[i], NONE, self.hull_tri[n]);
            self.hull_tri[i] = self.legalize(t + 2);
            self.hull_next[n] = n; // removed
            n = q;
        }

        // Walk backward from the other side.
        if e == start {
            loop {
                let q = self.hull_prev[e];
                if orient(p, self.points[q], self.points[e]) >= 0.0 {
                    break;
                }
                let t = self.add_triangle(q, i, e, NONE, self.hull_tri[e], self.hull_tri[q]);
                self.legalize(t + 2);
                self.hull_tri[q] = t;
                self.hull_next[e] = e; // removed
                e = q;
            }
        }

        self.hull_start = e;
        self.hull_prev[i] = e;
        self.hull_next[e] = i;
        self.hull_prev[n] = i;
        self.hull_next[i] = n;
        self.hash_vertex(i);
        self.hash_vertex(e);
    }

    /// A live hull vertex near `p` in angle, stepped back by one; the visible-edge walk starts
    /// there. `None` only for a point exactly at the sweep centre.
    fn visible_search_start(&self, p: Vec2) -> Option<usize> {
        let key = self.hash_key(p)?;
        let size = self.hull_hash.len();
        let mut start = NONE;
        for j in 0..size {
            start = self.hull_hash[(key + j) % size];
            if start != NONE && start != self.hull_next[start] {
                break;
            }
        }
        (start != NONE).then(|| self.hull_prev[start])
    }

    /// Walks the hull forward from `start` to the first edge that `p` sees.
    fn first_visible_edge(&self, p: Vec2, start: usize) -> Option<usize> {
        let mut e = start;
        loop {
            let q = self.hull_next[e];
            if orient(p, self.points[e], self.points[q]) < 0.0 {
                return Some(e);
            }
            e = q;
            if e == start {
                return None;
            }
        }
    }

    fn hash_key(&self, p: Vec2) -> Option<usize> {
        let angle = pseudo_angle(p.x - self.center.x, p.y - self.center.y);
        let size = self.hull_hash.len();
        (!angle.is_nan()).then(|| ((angle * size as f64).floor() as usize) % size)
    }

    fn hash_vertex(&mut self, i: usize) {
        if let Some(key) = self.hash_key(self.points[i]) {
            self.hull_hash[key] = i;
        }
    }

    fn link(&mut self, a: usize, b: usize) {
        self.halfedges[a] = b;
        if b != NONE {
            self.halfedges[b] = a;
        }
    }

    /// Adds triangle `i0, i1, i2` whose half-edges have twins `a, b, c`; returns its first
    /// half-edge.
    fn add_triangle(&mut self, i0: usize, i1: usize, i2: usize, a: usize, b: usize, c: usize) -> usize {
        let t = self.triangles.len();
        self.triangles.extend([i0, i1, i2]);
        self.halfedges.extend([NONE; 3]);
        self.link(t, a);
        self.link(t + 1, b);
        self.link(t + 2, c);
        t
    }

    /// Flips edges from half-edge `a` until the Delaunay condition holds; returns the half-edge
    /// that ends at the new point's hull edge.
    ///
    /// ```text
    ///           pl                    pl
    ///          /||\                  /  \
    ///       al/ || \bl            al/    \a
    ///        /  ||  \              /      \
    ///       /  a||b  \    flip    /___ar___\
    ///     p0\   ||   /p1   =>   p0\---bl---/p1
    ///        \  ||  /              \      /
    ///       ar\ || /br             b\    /br
    ///          \||/                  \  /
    ///           pr                    pr
    /// ```
    fn legalize(&mut self, mut a: usize) -> usize {
        loop {
            let b = self.halfedges[a];
            let a0 = a - a % 3;
            let ar = a0 + (a + 2) % 3;

            let flipped = b != NONE && self.flip_if_illegal(a, b, ar);
            if !flipped {
                match self.edge_stack.pop() {
                    Some(next) => a = next,
                    None => return ar,
                }
            }
        }
    }

    /// Flips the edge between half-edges `a` and `b` if the opposite point lies in the
    /// circumcircle, queueing the outer edge for another check. Returns whether it flipped.
    fn flip_if_illegal(&mut self, a: usize, b: usize, ar: usize) -> bool {
        let a0 = a - a % 3;
        let b0 = b - b % 3;
        let al = a0 + (a + 1) % 3;
        let bl = b0 + (b + 2) % 3;

        let p0 = self.triangles[ar];
        let pr = self.triangles[a];
        let pl = self.triangles[al];
        let p1 = self.triangles[bl];
        let pts = self.points;
        if !in_circle(pts[p0], pts[pr], pts[pl], pts[p1]) {
            return false;
        }

        self.triangles[a] = p1;
        self.triangles[b] = p0;
        let hbl = self.halfedges[bl];

        // The edge was swapped on the other side of the hull (rare): fix the hull reference.
        if hbl == NONE {
            let mut e = self.hull_start;
            loop {
                if self.hull_tri[e] == bl {
                    self.hull_tri[e] = a;
                    break;
                }
                e = self.hull_prev[e];
                if e == self.hull_start {
                    break;
                }
            }
        }
        self.link(a, hbl);
        self.link(b, self.halfedges[ar]);
        self.link(ar, bl);

        let br = b0 + (b + 1) % 3;
        if self.edge_stack.len() < EDGE_STACK_CAPACITY {
            self.edge_stack.push(br);
        }
        true
    }
}

/// Sorts `ids[left..=right]` by `dists[id]` with the original's quicksort, so points at equal
/// distances end up in the same order.
fn quicksort(ids: &mut [usize], dists: &[f64], left: usize, right: usize) {
    if right <= left + INSERTION_SORT_MAX {
        for i in left + 1..=right {
            let temp = ids[i];
            let temp_dist = dists[temp];
            let mut j = i;
            while j > left && dists[ids[j - 1]] > temp_dist {
                ids[j] = ids[j - 1];
                j -= 1;
            }
            ids[j] = temp;
        }
        return;
    }

    let median = left.midpoint(right);
    let mut i = left + 1;
    let mut j = right;
    ids.swap(median, i);
    if dists[ids[left]] > dists[ids[right]] {
        ids.swap(left, right);
    }
    if dists[ids[i]] > dists[ids[right]] {
        ids.swap(i, right);
    }
    if dists[ids[left]] > dists[ids[i]] {
        ids.swap(left, i);
    }

    let temp = ids[i];
    let temp_dist = dists[temp];
    loop {
        i += 1;
        while dists[ids[i]] < temp_dist {
            i += 1;
        }
        j -= 1;
        while dists[ids[j]] > temp_dist {
            j -= 1;
        }
        if j < i {
            break;
        }
        ids.swap(i, j);
    }
    ids[left + 1] = ids[j];
    ids[j] = temp;

    // `j >= left + 1` here, so `j - 1` cannot underflow.
    if right - i + 1 >= j - left {
        quicksort(ids, dists, i, right);
        quicksort(ids, dists, left, j - 1);
    } else {
        quicksort(ids, dists, left, j - 1);
        quicksort(ids, dists, i, right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_gives_two_triangles() {
        let pts = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let tris = triangulate(&pts);
        assert_eq!(tris.len(), 6);
        for t in tris.chunks_exact(3) {
            let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
            assert!((b - a).perp_dot(c - a) < 0.0);
        }
    }

    #[test]
    fn degenerate_input_gives_no_triangles() {
        assert!(triangulate(&[]).is_empty());
        assert!(triangulate(&[Vec2::ZERO, Vec2::ONE]).is_empty());
        assert!(triangulate(&[Vec2::ZERO, Vec2::ONE, Vec2::splat(2.0)]).is_empty());
        assert!(triangulate(&[Vec2::ONE; 4]).is_empty());
        assert!(triangulate(&[Vec2::ZERO, Vec2::X, Vec2::new(f64::NAN, 1.0)]).is_empty());
    }

    #[test]
    fn quicksort_sorts_large_inputs() {
        let dists: Vec<f64> = (0..200).map(|i| f64::from((i * 37) % 101)).collect();
        let mut ids: Vec<usize> = (0..dists.len()).collect();
        let last = ids.len() - 1;
        quicksort(&mut ids, &dists, 0, last);
        assert!(ids.windows(2).all(|w| dists[w[0]] <= dists[w[1]]));
    }
}
