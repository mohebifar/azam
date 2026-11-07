//! The untangler on the GPU path: runs on a background thread every few frames on the read-back
//! positions; its tangle codes become flag bits of the particle data, which make the contact
//! detection push poked-through particles back to their side.

use azam_sim::Simulation;
use azam_sim::untangle::{UntangleTotals, UntangleWorker};

/// The background untangler of one GPU simulation, started on first use.
#[derive(Debug)]
pub struct Untangler {
    worker: Option<UntangleWorker>,
    /// Whether to run it at all (the simulation's parameters may still switch it off).
    pub enabled: bool,
}

impl Untangler {
    /// An untangler that starts with the first frame.
    pub fn new() -> Self {
        Self {
            worker: None,
            enabled: true,
        }
    }

    /// Call after every read-back frame: returns new tangle codes when a run finished, and
    /// starts the next run when due.
    pub fn tick(&mut self, sim: &Simulation) -> Option<Vec<u32>> {
        if !self.enabled || !sim.params.untangle {
            return None;
        }
        let w = self.worker.get_or_insert_with(|| UntangleWorker::spawn(sim));
        w.tick(sim).map(|r| r.tangles)
    }

    /// Totals over the completed runs.
    pub fn totals(&self) -> UntangleTotals {
        self.worker.as_ref().map(|w| w.totals).unwrap_or_default()
    }

    /// Stops the worker (a new one starts on the next tick).
    pub fn restart(&mut self) {
        self.worker = None;
    }
}
