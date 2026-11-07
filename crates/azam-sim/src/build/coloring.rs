//! Greedy graph colouring of constraints: constraints of one colour share no particle, so a whole
//! colour can be solved in parallel (parallel Gauss–Seidel).

use crate::state::{BendConstraints, Fasteners, SewingConstraints, StretchConstraints};

/// Number of colours tried; the last one collects the constraints no other colour could take.
const COLORS: usize = 128;
/// Index of the overflow colour.
const OVERFLOW: usize = COLORS - 1;
/// Bits per mask word.
const WORD_BITS: usize = 32;
/// Mask words per particle.
const WORDS: usize = COLORS / WORD_BITS;

/// Constraints reordered into batches that can each run in parallel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColorBatches {
    /// Constraint indices, grouped by colour.
    pub order: Vec<u32>,
    /// `(start, count)` ranges of `order`, one per batch. Constraints of the overflow colour may
    /// conflict, so each forms a batch of its own.
    pub ranges: Vec<(u32, u32)>,
}

/// Colour batches of every constraint kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coloring {
    /// Stretch constraints.
    pub stretch: ColorBatches,
    /// Bending constraints.
    pub bend: ColorBatches,
    /// Fasteners.
    pub fasteners: ColorBatches,
    /// Sewing constraints.
    pub sewing: ColorBatches,
}

impl Coloring {
    /// Colours every constraint kind over `particles` particles.
    #[must_use]
    pub fn new(
        particles: usize,
        stretch: &StretchConstraints,
        bend: &BendConstraints,
        fasteners: &Fasteners,
        sewing: &SewingConstraints,
    ) -> Self {
        Self {
            stretch: colorize(particles, stretch.i.len(), |c| {
                [stretch.i[c], stretch.j[c], u32::MAX]
            }),
            bend: colorize(particles, bend.i.len(), |c| [bend.i[c], bend.j[c], u32::MAX]),
            fasteners: colorize(particles, fasteners.a.len(), |c| {
                [fasteners.a[c], fasteners.b[c], u32::MAX]
            }),
            sewing: colorize(particles, sewing.a.len(), |c| {
                [sewing.a[c], sewing.b0[c], sewing.b1[c]]
            }),
        }
    }
}

/// Greedy colouring of `n` constraints; `verts_of(c)` gives constraint `c`'s particles
/// (`u32::MAX` for unused slots). Each constraint takes the lowest colour none of its particles
/// has yet.
#[must_use]
pub fn colorize(particles: usize, n: usize, verts_of: impl Fn(usize) -> [u32; 3]) -> ColorBatches {
    let mut used = vec![0_u32; particles * WORDS];
    let mut color = vec![0_usize; n];
    let mut counts = [0_u32; COLORS];
    for (c, slot) in color.iter_mut().enumerate() {
        let verts = verts_of(c);
        let mut mask = [0_u32; WORDS];
        for &v in verts.iter().filter(|&&v| v != u32::MAX) {
            for (w, m) in mask.iter_mut().enumerate() {
                *m |= used[v as usize * WORDS + w];
            }
        }
        let cc = (0..COLORS)
            .find(|&b| mask[b / WORD_BITS] & (1 << (b % WORD_BITS)) == 0)
            .unwrap_or(OVERFLOW);
        *slot = cc;
        counts[cc] += 1;
        for &v in verts.iter().filter(|&&v| v != u32::MAX) {
            used[v as usize * WORDS + cc / WORD_BITS] |= 1 << (cc % WORD_BITS);
        }
    }
    let mut start = [0_u32; COLORS];
    let mut ranges = Vec::new();
    let mut o = 0;
    for b in 0..COLORS {
        start[b] = o;
        if counts[b] > 0 {
            ranges.push((o, counts[b]));
        }
        o += counts[b];
    }
    let mut order = vec![0_u32; n];
    let mut fill = start;
    for (c, &cc) in color.iter().enumerate() {
        order[fill[cc] as usize] = c as u32;
        fill[cc] += 1;
    }
    if counts[OVERFLOW] > 0 {
        ranges.pop();
        ranges.extend((0..counts[OVERFLOW]).map(|i| (start[OVERFLOW] + i, 1)));
    }
    ColorBatches { order, ranges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_alternates_two_colours() {
        let edges = [(0, 1), (1, 2), (2, 3), (3, 4)];
        let b = colorize(5, edges.len(), |c| [edges[c].0, edges[c].1, u32::MAX]);
        assert_eq!(b.ranges, vec![(0, 2), (2, 2)]);
        assert_eq!(b.order, vec![0, 2, 1, 3]);
    }
}
