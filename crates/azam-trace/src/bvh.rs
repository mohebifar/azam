//! A bounding volume hierarchy over triangles, built with the binned surface area heuristic.

use glam::{Vec3, Vec3A};

/// Bins per axis when searching for the best split.
const BINS: usize = 16;
/// Leaves never hold more triangles than this (unless they cannot be split at all).
const MAX_LEAF: u32 = 8;
/// Leaves are forced below this depth, so the traversal stack can never overflow.
const MAX_DEPTH: usize = 56;
/// Traversal stack size (deeper than [`MAX_DEPTH`] + 1).
const STACK: usize = 64;
/// Cost of one node visit relative to one triangle test, for the SAH.
const TRAVERSAL_COST: f32 = 1.0;

/// A ray: origin and unit direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    /// Where the ray starts.
    pub origin: Vec3,
    /// Unit direction.
    pub dir: Vec3,
}

impl Ray {
    /// A ray from `origin` along `dir` (which should be unit length).
    #[must_use]
    pub const fn new(origin: Vec3, dir: Vec3) -> Self {
        Self { origin, dir }
    }

    /// The point at distance `t`.
    #[must_use]
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.dir * t
    }
}

/// Where a ray meets a triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Distance along the ray.
    pub t: f32,
    /// Index of the triangle, as given to [`Bvh::new`].
    pub prim: u32,
    /// Barycentric weight of the triangle's second vertex.
    pub u: f32,
    /// Barycentric weight of the triangle's third vertex.
    pub v: f32,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    min: Vec3A,
    max: Vec3A,
    /// First triangle of a leaf, or the left child of an inner node (the right one follows it).
    start: u32,
    /// Triangles in a leaf; 0 for an inner node.
    count: u32,
}

/// A triangle prepared for the Möller–Trumbore test.
#[derive(Clone, Copy, Debug)]
struct Tri {
    v0: Vec3A,
    e1: Vec3A,
    e2: Vec3A,
}

/// A bounding volume hierarchy over a triangle soup.
#[derive(Clone, Debug)]
pub struct Bvh {
    nodes: Vec<Node>,
    tris: Vec<Tri>,
    ids: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
struct Aabb {
    min: Vec3A,
    max: Vec3A,
}

impl Aabb {
    const EMPTY: Self = Self {
        min: Vec3A::splat(f32::MAX),
        max: Vec3A::splat(f32::MIN),
    };

    fn grow(&mut self, p: Vec3A) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    fn merge(&mut self, b: &Self) {
        self.min = self.min.min(b.min);
        self.max = self.max.max(b.max);
    }

    fn area(&self) -> f32 {
        let d = (self.max - self.min).max(Vec3A::ZERO);
        d.x * d.y + d.y * d.z + d.z * d.x
    }
}

/// A build task: a node and its triangle range.
struct Task {
    node: usize,
    depth: usize,
}

impl Bvh {
    /// Builds the hierarchy over `triangles` (three corners each).
    #[must_use]
    pub fn new(triangles: &[[Vec3; 3]]) -> Self {
        let n = triangles.len();
        let mut boxes = Vec::with_capacity(n);
        let mut centroids = Vec::with_capacity(n);
        for t in triangles {
            let mut b = Aabb::EMPTY;
            for p in t {
                b.grow(Vec3A::from(*p));
            }
            centroids.push((b.min + b.max) * 0.5);
            boxes.push(b);
        }
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes = Vec::with_capacity(2 * n.max(1));
        nodes.push(Node {
            min: Vec3A::ZERO,
            max: Vec3A::ZERO,
            start: 0,
            count: n as u32,
        });
        let mut tasks = vec![Task { node: 0, depth: 0 }];
        while let Some(task) = tasks.pop() {
            let node = nodes[task.node];
            let range = node.start as usize..(node.start + node.count) as usize;
            let mut bounds = Aabb::EMPTY;
            for &i in &order[range.clone()] {
                bounds.merge(&boxes[i as usize]);
            }
            nodes[task.node].min = bounds.min;
            nodes[task.node].max = bounds.max;
            if node.count <= 2 || task.depth >= MAX_DEPTH {
                continue;
            }
            let Some(mid) = split(&mut order[range.clone()], &boxes, &centroids, &bounds) else {
                continue;
            };
            let left = nodes.len();
            nodes.push(Node {
                min: Vec3A::ZERO,
                max: Vec3A::ZERO,
                start: node.start,
                count: mid as u32,
            });
            nodes.push(Node {
                min: Vec3A::ZERO,
                max: Vec3A::ZERO,
                start: node.start + mid as u32,
                count: node.count - mid as u32,
            });
            nodes[task.node].start = left as u32;
            nodes[task.node].count = 0;
            tasks.push(Task {
                node: left,
                depth: task.depth + 1,
            });
            tasks.push(Task {
                node: left + 1,
                depth: task.depth + 1,
            });
        }
        let tris = order
            .iter()
            .map(|&i| {
                let [a, b, c] = triangles[i as usize];
                Tri {
                    v0: a.into(),
                    e1: (b - a).into(),
                    e2: (c - a).into(),
                }
            })
            .collect();
        Self {
            nodes,
            tris,
            ids: order,
        }
    }

    /// Number of triangles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tris.len()
    }

    /// Whether there are no triangles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// Number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The bounds of every triangle, or `None` when there are none.
    #[must_use]
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let root = self.nodes.first()?;
        (!self.is_empty()).then(|| (root.min.into(), root.max.into()))
    }

    /// The nearest hit along `ray` in `(0, t_max)` whose triangle `accept` agrees to (a filter for
    /// cut-out or transparent surfaces).
    pub fn intersect(&self, ray: &Ray, t_max: f32, mut accept: impl FnMut(u32) -> bool) -> Option<Hit> {
        if self.is_empty() {
            return None;
        }
        let traversal = Traversal::new(ray);
        let mut best: Option<Hit> = None;
        let mut t_best = t_max;
        let mut stack = [0u32; STACK];
        let mut sp = 0;
        let mut node = 0usize;
        traversal.slab(&self.nodes[0], t_best)?;
        loop {
            let n = self.nodes[node];
            if n.count > 0 {
                for i in n.start..n.start + n.count {
                    if let Some((t, u, v)) = traversal.triangle(&self.tris[i as usize], t_best) {
                        let prim = self.ids[i as usize];
                        if accept(prim) {
                            t_best = t;
                            best = Some(Hit { t, prim, u, v });
                        }
                    }
                }
            } else {
                let (l, r) = (n.start as usize, n.start as usize + 1);
                let tl = traversal.slab(&self.nodes[l], t_best);
                let tr = traversal.slab(&self.nodes[r], t_best);
                match (tl, tr) {
                    (Some(a), Some(b)) => {
                        let (near, far) = if a <= b { (l, r) } else { (r, l) };
                        stack[sp] = far as u32;
                        sp += 1;
                        node = near;
                        continue;
                    }
                    (Some(_), None) => {
                        node = l;
                        continue;
                    }
                    (None, Some(_)) => {
                        node = r;
                        continue;
                    }
                    (None, None) => {}
                }
            }
            // Pop the next node that is still nearer than the best hit.
            loop {
                if sp == 0 {
                    return best;
                }
                sp -= 1;
                let next = stack[sp] as usize;
                if traversal.slab(&self.nodes[next], t_best).is_some() {
                    node = next;
                    break;
                }
            }
        }
    }

    /// Whether anything that `accept` agrees to blocks `ray` in `(0, t_max)`.
    pub fn occluded(&self, ray: &Ray, t_max: f32, mut accept: impl FnMut(u32) -> bool) -> bool {
        if self.is_empty() {
            return false;
        }
        let traversal = Traversal::new(ray);
        let mut stack = [0u32; STACK];
        let mut sp = 0;
        stack[sp] = 0;
        sp += 1;
        while sp > 0 {
            sp -= 1;
            let n = self.nodes[stack[sp] as usize];
            if traversal.slab(&n, t_max).is_none() {
                continue;
            }
            if n.count > 0 {
                for i in n.start..n.start + n.count {
                    if traversal.triangle(&self.tris[i as usize], t_max).is_some()
                        && accept(self.ids[i as usize])
                    {
                        return true;
                    }
                }
            } else {
                stack[sp] = n.start;
                stack[sp + 1] = n.start + 1;
                sp += 2;
            }
        }
        false
    }

    /// The nearest hit by testing every triangle (for checking the hierarchy).
    #[must_use]
    pub fn intersect_brute_force(&self, ray: &Ray, t_max: f32) -> Option<Hit> {
        let traversal = Traversal::new(ray);
        let mut best: Option<Hit> = None;
        let mut t_best = t_max;
        for (tri, &prim) in self.tris.iter().zip(&self.ids) {
            if let Some((t, u, v)) = traversal.triangle(tri, t_best) {
                t_best = t;
                best = Some(Hit { t, prim, u, v });
            }
        }
        best
    }
}

/// Splits `order` (the triangles of a node with `bounds`) by the binned SAH; returns how many
/// triangles go left, or `None` when a leaf is cheaper.
fn split(order: &mut [u32], boxes: &[Aabb], centroids: &[Vec3A], bounds: &Aabb) -> Option<usize> {
    let count = order.len();
    let mut cb = Aabb::EMPTY;
    for &i in order.iter() {
        cb.grow(centroids[i as usize]);
    }
    let extent = cb.max - cb.min;
    let mut best: Option<(usize, usize, f32)> = None;
    for axis in 0..3 {
        if extent[axis] <= 1e-9 {
            continue;
        }
        let scale = BINS as f32 / extent[axis];
        let bin_of = |i: u32| (((centroids[i as usize][axis] - cb.min[axis]) * scale) as usize).min(BINS - 1);
        let mut bins = [(Aabb::EMPTY, 0usize); BINS];
        for &i in order.iter() {
            let b = &mut bins[bin_of(i)];
            b.0.merge(&boxes[i as usize]);
            b.1 += 1;
        }
        // Sweep from the right to know each plane's right-hand side, then from the left.
        let mut right_area = [0.0f32; BINS];
        let mut right_count = [0usize; BINS];
        let mut acc = Aabb::EMPTY;
        let mut n = 0;
        for k in (1..BINS).rev() {
            acc.merge(&bins[k].0);
            n += bins[k].1;
            right_area[k] = acc.area();
            right_count[k] = n;
        }
        let mut acc = Aabb::EMPTY;
        let mut n = 0;
        for k in 0..BINS - 1 {
            acc.merge(&bins[k].0);
            n += bins[k].1;
            if n == 0 || right_count[k + 1] == 0 {
                continue;
            }
            let cost = n as f32 * acc.area() + right_count[k + 1] as f32 * right_area[k + 1];
            if best.is_none_or(|(_, _, c)| cost < c) {
                best = Some((axis, k + 1, cost));
            }
        }
    }
    let leaf_cost = count as f32 * bounds.area();
    let split_cost = best.map(|(_, _, c)| TRAVERSAL_COST * bounds.area() + c);
    match (best, split_cost) {
        (Some((axis, plane, _)), Some(cost)) if cost < leaf_cost || count > MAX_LEAF as usize => {
            let scale = BINS as f32 / extent[axis];
            let goes_left = |i: u32| {
                ((((centroids[i as usize][axis] - cb.min[axis]) * scale) as usize).min(BINS - 1)) < plane
            };
            let mid = partition(order, goes_left);
            (mid > 0 && mid < count).then_some(mid)
        }
        (None, _) if count > MAX_LEAF as usize => {
            // Every centroid coincides: split the list in half so leaves stay small.
            Some(count / 2)
        }
        _ => None,
    }
}

/// Moves the items `pred` holds for to the front; returns how many there are.
fn partition(items: &mut [u32], pred: impl Fn(u32) -> bool) -> usize {
    let mut first = 0;
    for i in 0..items.len() {
        if pred(items[i]) {
            items.swap(first, i);
            first += 1;
        }
    }
    first
}

/// Per-ray constants for the box and triangle tests.
struct Traversal {
    origin: Vec3A,
    dir: Vec3A,
    inv_dir: Vec3A,
}

impl Traversal {
    fn new(ray: &Ray) -> Self {
        // Keep the reciprocal finite, so 0 × ∞ never makes a NaN in the slab test.
        let safe = |d: f32| if d.abs() < 1e-20 { 1e-20f32.copysign(d) } else { d };
        let d = Vec3A::from(ray.dir);
        Self {
            origin: ray.origin.into(),
            dir: d,
            inv_dir: Vec3A::new(1.0 / safe(d.x), 1.0 / safe(d.y), 1.0 / safe(d.z)),
        }
    }

    /// Where the ray enters the node's box, if it does before `t_max`.
    #[inline]
    fn slab(&self, n: &Node, t_max: f32) -> Option<f32> {
        let t1 = (n.min - self.origin) * self.inv_dir;
        let t2 = (n.max - self.origin) * self.inv_dir;
        let near = t1.min(t2).max_element().max(0.0);
        let far = t1.max(t2).min_element().min(t_max);
        (near <= far).then_some(near)
    }

    /// Möller–Trumbore: distance and barycentrics of a hit in `(0, t_max)`.
    #[inline]
    fn triangle(&self, tri: &Tri, t_max: f32) -> Option<(f32, f32, f32)> {
        let p = self.dir.cross(tri.e2);
        let det = tri.e1.dot(p);
        if det.abs() < 1e-14 {
            return None;
        }
        let inv = 1.0 / det;
        let s = self.origin - tri.v0;
        let u = s.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(tri.e1);
        let v = self.dir.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = tri.e2.dot(q) * inv;
        (t > 0.0 && t < t_max).then_some((t, u, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rng;

    fn random_soup(rng: &mut Rng, n: usize) -> Vec<[Vec3; 3]> {
        let mut v = |s: f32| Vec3::new(rng.next_f32(), rng.next_f32(), rng.next_f32()) * s;
        (0..n)
            .map(|_| {
                let c = v(1000.0);
                [c + v(60.0), c + v(60.0), c + v(60.0)]
            })
            .collect()
    }

    fn random_dir(rng: &mut Rng) -> Vec3 {
        loop {
            let d = Vec3::new(rng.next_f32(), rng.next_f32(), rng.next_f32()) * 2.0 - Vec3::ONE;
            if d.length_squared() > 1e-3 && d.length_squared() <= 1.0 {
                return d.normalize();
            }
        }
    }

    #[test]
    fn matches_brute_force_on_random_rays() {
        let mut rng = Rng::new(42, 1);
        let soup = random_soup(&mut rng, 3000);
        let bvh = Bvh::new(&soup);
        assert_eq!(bvh.len(), 3000);
        let mut hits = 0;
        for _ in 0..4000 {
            let origin =
                Vec3::new(rng.next_f32(), rng.next_f32(), rng.next_f32()) * 1400.0 - Vec3::splat(200.0);
            let ray = Ray::new(origin, random_dir(&mut rng));
            let fast = bvh.intersect(&ray, f32::INFINITY, |_| true);
            let slow = bvh.intersect_brute_force(&ray, f32::INFINITY);
            match (fast, slow) {
                (Some(a), Some(b)) => {
                    hits += 1;
                    assert!((a.t - b.t).abs() < 1e-3, "{a:?} vs {b:?}");
                    assert_eq!(a.prim, b.prim);
                }
                (None, None) => {}
                other => panic!("mismatch {other:?}"),
            }
            assert_eq!(bvh.occluded(&ray, f32::INFINITY, |_| true), slow.is_some());
            if let Some(b) = slow {
                assert!(!bvh.occluded(&ray, b.t * 0.999, |_| true));
            }
        }
        assert!(hits > 500, "too few hits to be a test: {hits}");
    }

    #[test]
    fn filter_skips_rejected_triangles() {
        // Two parallel quads; rejecting the near one exposes the far one.
        let quad = |z: f32| {
            let (a, b, c, d) = (
                Vec3::new(-1.0, -1.0, z),
                Vec3::new(1.0, -1.0, z),
                Vec3::new(1.0, 1.0, z),
                Vec3::new(-1.0, 1.0, z),
            );
            [[a, b, c], [a, c, d]]
        };
        let mut soup = quad(1.0).to_vec();
        soup.extend(quad(2.0));
        let bvh = Bvh::new(&soup);
        let ray = Ray::new(Vec3::new(0.1, 0.2, 0.0), Vec3::Z);
        let near = bvh.intersect(&ray, f32::INFINITY, |_| true).expect("hit");
        assert!((near.t - 1.0).abs() < 1e-5);
        let far = bvh.intersect(&ray, f32::INFINITY, |p| p >= 2).expect("hit");
        assert!((far.t - 2.0).abs() < 1e-5);
        assert!(!bvh.occluded(&ray, f32::INFINITY, |_| false));
    }

    #[test]
    fn degenerate_input_builds() {
        // Many triangles with the same centroid, plus an empty hierarchy.
        let t = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let bvh = Bvh::new(&vec![t; 100]);
        let ray = Ray::new(Vec3::new(0.2, 0.2, -1.0), Vec3::Z);
        assert!(bvh.intersect(&ray, f32::INFINITY, |_| true).is_some());
        let empty = Bvh::new(&[]);
        assert!(empty.intersect(&ray, f32::INFINITY, |_| true).is_none());
        assert!(empty.bounds().is_none());
    }
}
