//! The untangler: finds cloth that has passed through cloth and marks the particles to push back.
//!
//! [`UntangleSetup::run`] is the synchronous core (deterministic, for tests and single-threaded
//! use). [`UntangleWorker`] runs it on a background thread on a copy of the positions, at most
//! every few frames, and hands back the latest result; the solver that consumes the tangle codes
//! uploads them as per-particle flags.

mod core;

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

pub use self::core::{DEFAULT_REGION_CAP, UntangleResult, UntangleSetup};
use crate::state::Simulation;

/// Frames between two untangler runs.
pub const UNTANGLE_INTERVAL_FRAMES: u64 = 6;

/// Running totals over a worker's runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UntangleTotals {
    /// Completed runs.
    pub runs: usize,
    /// Tangle spots resolved.
    pub resolved: usize,
    /// Tangle spots left ambiguous.
    pub ambiguous: usize,
    /// Particles marked.
    pub marked: usize,
}

/// A background untangler for one simulation.
#[derive(Debug)]
pub struct UntangleWorker {
    jobs: Option<Sender<Vec<f32>>>,
    results: Receiver<UntangleResult>,
    thread: Option<JoinHandle<()>>,
    busy: bool,
    last_frame: Option<u64>,
    dead: bool,
    /// Totals over the completed runs.
    pub totals: UntangleTotals,
}

impl UntangleWorker {
    /// Starts a worker thread for `sim` (its static data is copied now).
    #[must_use]
    pub fn spawn(sim: &Simulation) -> Self {
        let setup = UntangleSetup::new(sim);
        let (job_tx, job_rx) = mpsc::channel::<Vec<f32>>();
        let (res_tx, res_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("azam-untangle".to_owned())
            .spawn(move || {
                while let Ok(x) = job_rx.recv() {
                    if res_tx.send(setup.run(&x)).is_err() {
                        break;
                    }
                }
            })
            .ok();
        let dead = thread.is_none();
        Self {
            jobs: Some(job_tx),
            results: res_rx,
            thread,
            busy: false,
            last_frame: None,
            dead,
            totals: UntangleTotals::default(),
        }
    }

    /// Call once per frame: returns a finished result if one arrived, and starts a new run on the
    /// current positions when the worker is idle and enough frames have passed. Does nothing when
    /// untangling is switched off in the simulation's parameters.
    pub fn tick(&mut self, sim: &Simulation) -> Option<UntangleResult> {
        if !sim.params.untangle || self.dead {
            return None;
        }
        let result = match self.results.try_recv() {
            Ok(r) => {
                self.busy = false;
                self.totals.runs += 1;
                self.totals.resolved += r.resolved;
                self.totals.ambiguous += r.ambiguous;
                self.totals.marked += r.marked;
                Some(r)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.dead = true;
                return None;
            }
        };
        let due = self
            .last_frame
            .is_none_or(|f| sim.frames.saturating_sub(f) >= UNTANGLE_INTERVAL_FRAMES);
        if !self.busy && due {
            if let Some(tx) = &self.jobs
                && tx.send(sim.particles.x.clone()).is_ok()
            {
                self.busy = true;
                self.last_frame = Some(sim.frames);
            } else {
                self.dead = true;
            }
        }
        result
    }

    /// Whether a run is in progress.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.busy
    }
}

impl Drop for UntangleWorker {
    fn drop(&mut self) {
        // Closing the job channel ends the thread after its current run.
        self.jobs = None;
        if let Some(t) = self.thread.take() {
            drop(t);
        }
    }
}
