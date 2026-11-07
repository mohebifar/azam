//! Settle detection for runs that stop by themselves once the garment has come to rest.

use std::collections::VecDeque;

use crate::settings::SimValues;

/// Minimum time between two recorded energy samples, in simulated seconds.
const SAMPLE_INTERVAL_S: f64 = 0.25;
/// How far back energy samples are kept, in simulated seconds.
const HISTORY_S: f64 = 4.25;
/// Seams further apart than this (mean gap, mm) mean the garment is still being sewn.
const SEWING_GAP_MM: f64 = 2.5;
/// No settling is reported in the first second.
const WARMUP_S: f64 = 1.0;
/// Consecutive frames below the threshold needed to report [`SettleReason::Threshold`].
const THRESHOLD_FRAMES: u32 = 40;
/// Earliest time a plateau can be reported, in simulated seconds.
const PLATEAU_START_S: f64 = 4.0;
/// Width of the recent window compared against the older one, in simulated seconds.
const RECENT_WINDOW_S: f64 = 2.0;
/// Samples needed in each window to judge a plateau.
const PLATEAU_SAMPLES: usize = 4;
/// A plateau's recent energy must be below this multiple of the threshold.
const PLATEAU_THRESHOLD_FACTOR: f64 = 8.0;
/// A plateau's recent energy must be above this fraction of the older energy (no longer falling).
const PLATEAU_DECAY: f64 = 0.9;

/// Why a run counts as settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleReason {
    /// Motion stayed below the threshold for a while.
    Threshold,
    /// Motion is low and has stopped decreasing.
    Plateau,
}

/// Tracks motion energy over simulated time and reports when it has settled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettleTracker {
    samples: VecDeque<(f64, f64)>,
    below: u32,
}

impl SettleTracker {
    /// A tracker with no history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the energy `energy` at time `t` (seconds since the run started) with mean seam gap
    /// `gap` (mm), and reports whether the run has settled for threshold `threshold`.
    pub fn push(&mut self, t: f64, energy: f64, gap: f64, threshold: f64) -> Option<SettleReason> {
        if self
            .samples
            .back()
            .is_none_or(|&(t0, _)| t - t0 >= SAMPLE_INTERVAL_S)
        {
            self.samples.push_back((t, energy));
        }
        while self.samples.front().is_some_and(|&(t0, _)| t - t0 > HISTORY_S) {
            self.samples.pop_front();
        }
        if gap >= SEWING_GAP_MM {
            self.below = 0;
            return None;
        }
        if t > WARMUP_S && energy < threshold {
            self.below += 1;
            if self.below > THRESHOLD_FRAMES {
                return Some(SettleReason::Threshold);
            }
        } else {
            self.below = 0;
        }
        if t >= PLATEAU_START_S {
            let (mut a, mut na, mut b, mut nb) = (0.0, 0_usize, 0.0, 0_usize);
            for &(ts, e) in &self.samples {
                if t - ts <= RECENT_WINDOW_S {
                    a += e;
                    na += 1;
                } else {
                    b += e;
                    nb += 1;
                }
            }
            if na >= PLATEAU_SAMPLES && nb >= PLATEAU_SAMPLES {
                let recent = a / na as f64;
                if recent < threshold * PLATEAU_THRESHOLD_FACTOR && recent > PLATEAU_DECAY * (b / nb as f64) {
                    return Some(SettleReason::Plateau);
                }
            }
        }
        None
    }
}

/// Why an auto-stopping run stopped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StopReason {
    /// The garment settled after this many simulated seconds.
    Settled {
        /// What counted as settled.
        reason: SettleReason,
        /// Simulated seconds since the run started.
        after: f64,
    },
    /// The run reached [`SimValues::max_time`].
    MaxTime {
        /// Simulated seconds since the run started.
        after: f64,
    },
}

/// Auto-stop state of one run, fed once per screen frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutoStop {
    start: Option<f64>,
    tracker: SettleTracker,
}

impl AutoStop {
    /// A run that starts at the next [`AutoStop::update`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds the simulation's state after a frame; returns why the run should stop, if it should.
    pub fn update(&mut self, time: f64, energy: f64, sew_gap: f64, values: &SimValues) -> Option<StopReason> {
        let t0 = *self.start.get_or_insert(time);
        let t = time - t0;
        if let Some(reason) = self.tracker.push(t, energy, sew_gap, values.settle) {
            return Some(StopReason::Settled { reason, after: t });
        }
        (t > values.max_time).then_some(StopReason::MaxTime { after: t })
    }
}
