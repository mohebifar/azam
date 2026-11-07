//! The dispatch sequence of one screen frame (`frames` simulated frames of `substeps` substeps)
//! and its recording into a compute pass.

use crate::buffers::Buffers;
use crate::layout::{DISPATCH_STRIDE, SCAN_WORKGROUP, TRIANGLE_HASH_BUCKETS};
use crate::pack::{Batches, DESC_APPLY, DESC_APPLY_SHIFT, Groups};
use crate::passes::Pass;
use crate::pipelines::Pipelines;

/// One dispatch: a pass over `count` elements, with dispatch descriptor `desc` for passes that
/// read one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dispatch {
    /// The pass.
    pub pass: Pass,
    /// Elements (threads) to cover.
    pub count: u32,
    /// Dispatch descriptor index.
    pub desc: u32,
}

/// What decides a frame's dispatch sequence.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools, reason = "independent frame switches")]
pub struct FramePlan<'a> {
    /// Simulated frames.
    pub frames: u32,
    /// Substeps per simulated frame.
    pub substeps: u32,
    /// Stretch and sewing iterations per substep.
    pub iterations: u32,
    /// Contacts are re-detected every this many substeps.
    pub detect_every: u32,
    /// Contact resolution passes per substep.
    pub contact_iters: u32,
    /// Cloth-cloth contacts on.
    pub self_collision: bool,
    /// No frame has run since upload (contacts are detected on the first substep).
    pub first_frame: bool,
    /// Particles.
    pub particles: u32,
    /// Cloth triangles.
    pub triangles: u32,
    /// Sewing constraints.
    pub sewing: u32,
    /// Skin vertices.
    pub skin: u32,
    /// Pins.
    pub pins: u32,
    /// Grabbed particles.
    pub grab: u32,
    /// A piece has inflation pressure.
    pub pressure: bool,
    /// Contact slots.
    pub contact_cap: u32,
    /// Colour batches.
    pub groups: &'a Groups,
}

impl FramePlan<'_> {
    /// The dispatches, in order.
    #[must_use]
    pub fn dispatches(&self) -> Vec<Dispatch> {
        let mut out = Vec::new();
        let mut run = |pass, count: u32, desc| {
            if count > 0 {
                out.push(Dispatch { pass, count, desc });
            }
        };
        let batches = |run: &mut dyn FnMut(Pass, u32, u32), pass, b: &Batches| {
            for &(count, desc) in &b.0 {
                run(pass, count, desc);
            }
        };
        let n = self.particles;
        let hs = TRIANGLE_HASH_BUCKETS;
        let ke = self.detect_every.clamp(1, self.substeps.max(1));
        for f in 0..self.frames {
            run(Pass::Gap, self.sewing.div_ceil(2), 0);
            run(Pass::Grav, 1, 0);
            if self.pressure {
                run(Pass::NormalClear, n * 4, 0);
                run(Pass::NormalAccumulate, self.triangles, 0);
            }
            for s in 0..self.substeps {
                run(Pass::Integrate, n, 0);
                run(Pass::Body, n, 0);
                for _ in 0..self.iterations {
                    batches(&mut run, Pass::Dist, &self.groups.stretch);
                    batches(&mut run, Pass::Sew, &self.groups.sewing);
                }
                batches(&mut run, Pass::Dist, &self.groups.bend);
                batches(&mut run, Pass::Dist, &self.groups.fasteners);
                if self.self_collision && self.triangles > 0 {
                    if s % ke == ke - 1 || (self.first_frame && f == 0 && s == 0) {
                        run(Pass::HashClear, hs, 0);
                        run(Pass::HashCount, self.triangles, 0);
                        run(Pass::Scan1, hs, 0);
                        run(Pass::Scan2, SCAN_WORKGROUP, 0);
                        run(Pass::Scan3, hs, 0);
                        run(Pass::Scatter, self.triangles, 0);
                        run(Pass::Detect, n, 0);
                    }
                    for _ in 0..self.contact_iters {
                        run(Pass::Resolve, self.contact_cap, 0);
                        run(Pass::Apply, n, DESC_APPLY);
                    }
                }
                run(Pass::Grab, self.grab, 0);
                run(Pass::Guard, n, 0);
                if self.skin > 0 && (s + 1 == self.substeps || s + 1 == self.substeps / 2) {
                    run(Pass::BodyTri, self.triangles, 0);
                    run(Pass::Apply, n, DESC_APPLY_SHIFT);
                }
                run(Pass::Pins, self.pins, 0);
                run(Pass::Vel, n, 0);
            }
        }
        run(Pass::Energy, n, 0);
        out
    }
}

/// Records `dispatches` into one compute pass of `encoder`.
pub fn record(
    encoder: &mut wgpu::CommandEncoder,
    pipelines: &Pipelines,
    buffers: &Buffers,
    dispatches: &[Dispatch],
) {
    let mut cp = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("azam-sim-gpu frame"),
        timestamp_writes: None,
    });
    for d in dispatches {
        let p = pipelines.get(d.pass);
        cp.set_pipeline(&p.pipeline);
        cp.set_bind_group(0, &buffers.groups[d.pass.index()], &[]);
        if d.pass.spec().includes.dispatch {
            cp.set_bind_group(1, &buffers.dispatch_group, &[d.desc * DISPATCH_STRIDE as u32]);
        }
        cp.dispatch_workgroups(d.count.div_ceil(p.workgroup), 1, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(groups: &Groups) -> FramePlan<'_> {
        FramePlan {
            frames: 1,
            substeps: 4,
            iterations: 2,
            detect_every: 2,
            contact_iters: 2,
            self_collision: true,
            first_frame: true,
            particles: 100,
            triangles: 150,
            sewing: 10,
            skin: 50,
            pins: 1,
            grab: 0,
            pressure: false,
            contact_cap: 65536,
            groups,
        }
    }

    fn count(ds: &[Dispatch], pass: Pass) -> usize {
        ds.iter().filter(|d| d.pass == pass).count()
    }

    #[test]
    fn substep_schedule_follows_the_reference_order() {
        let groups = Groups {
            stretch: Batches(vec![(5, 2), (4, 3)]),
            bend: Batches(vec![(3, 4)]),
            fasteners: Batches(Vec::new()),
            sewing: Batches(vec![(2, 5)]),
        };
        let ds = plan(&groups).dispatches();
        assert_eq!(ds[0].pass, Pass::Gap);
        assert_eq!(ds[0].count, 5);
        assert_eq!(ds[1].pass, Pass::Grav);
        assert_eq!(ds[2].pass, Pass::Integrate);
        assert_eq!(ds[3].pass, Pass::Body);
        // two iterations of two stretch colours and one sewing colour, then bending
        let solve: Vec<_> = ds[4..11].iter().map(|d| (d.pass, d.desc)).collect();
        assert_eq!(
            solve,
            vec![
                (Pass::Dist, 2),
                (Pass::Dist, 3),
                (Pass::Sew, 5),
                (Pass::Dist, 2),
                (Pass::Dist, 3),
                (Pass::Sew, 5),
                (Pass::Dist, 4)
            ]
        );
        // detection on the first substep (first frame) and on substeps 1 and 3
        assert_eq!(count(&ds, Pass::Detect), 3);
        assert_eq!(count(&ds, Pass::Resolve), 8);
        // skin collision on the last and the middle substep, each followed by a shifting apply
        assert_eq!(count(&ds, Pass::BodyTri), 2);
        let shifts = ds
            .iter()
            .filter(|d| d.pass == Pass::Apply && d.desc == DESC_APPLY_SHIFT)
            .count();
        assert_eq!(shifts, 2);
        assert_eq!(count(&ds, Pass::Grab), 0);
        assert_eq!(count(&ds, Pass::Pins), 4);
        assert_eq!(ds.last().map(|d| d.pass), Some(Pass::Energy));
    }

    #[test]
    fn later_frames_detect_only_on_schedule() {
        let groups = Groups::default();
        let p = FramePlan {
            first_frame: false,
            frames: 2,
            ..plan(&groups)
        };
        let ds = p.dispatches();
        assert_eq!(count(&ds, Pass::Detect), 4);
        assert_eq!(count(&ds, Pass::Grav), 2);
        assert_eq!(count(&ds, Pass::Energy), 1);
    }
}
