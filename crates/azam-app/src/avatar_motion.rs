//! The avatar in motion while the simulation plays, and the bundled morph-target body.
//!
//! A [`MotionState`] keeps the chosen [`Motion`] and its clock. Poses are computed ahead on a
//! worker thread ([`PoseWorker`]): every played frame takes the newest finished pose, if any, and
//! asks for the pose at the end of the next frame, so the window never waits for a collision grid
//! (a full update takes about 2–6 ms in release, so the body normally changes every frame; when
//! posing falls behind the frame rate the body simply updates every few frames).

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

use azam_avatar::makehuman::find_pack;
use azam_avatar::{Avatar, MhModel, Motion, pose_at};

/// Whether the bundled morph-target pack exists (checked once).
pub(crate) fn makehuman_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| find_pack().is_some())
}

/// The bundled morph-target model, parsed once on first use; `None` if missing or unreadable.
pub(crate) fn bundled_model() -> Option<Arc<MhModel>> {
    static MODEL: OnceLock<Option<Arc<MhModel>>> = OnceLock::new();
    MODEL
        .get_or_init(|| match MhModel::load_bundled()? {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                eprintln!("avatar pack: {e}; using the mannequin");
                None
            }
        })
        .clone()
}

/// Poses one avatar for one motion on a thread of its own.
struct PoseWorker {
    requests: Sender<f64>,
    results: Receiver<Arc<Avatar>>,
    waiting: bool,
}

impl PoseWorker {
    fn start(rest: Arc<Avatar>, motion: Motion) -> Self {
        let (requests, inbox) = channel::<f64>();
        let (outbox, results) = channel();
        std::thread::spawn(move || {
            let frame = rest.motion_frame(motion);
            let mut previous: Option<(Arc<Avatar>, f64)> = None;
            while let Ok(mut t) = inbox.recv() {
                while let Ok(later) = inbox.try_recv() {
                    t = later;
                }
                let pose = pose_at(motion, t);
                let posed = match &previous {
                    Some((p, t0)) if t > *t0 => rest.posed_after(&pose, &frame, p, t - t0),
                    _ => rest.posed_in(&pose, &frame),
                };
                let posed = Arc::new(posed);
                if outbox.send(posed.clone()).is_err() {
                    break;
                }
                previous = Some((posed, t));
            }
        });
        Self {
            requests,
            results,
            waiting: false,
        }
    }
}

/// The chosen motion, its clock and the posed body in use.
#[derive(Default)]
pub struct MotionState {
    motion: Motion,
    time: f64,
    body: Option<Arc<Avatar>>,
    worker: Option<PoseWorker>,
    back_to_rest: bool,
}

impl MotionState {
    /// The chosen motion.
    #[must_use]
    pub fn motion(&self) -> Motion {
        self.motion
    }

    /// Chooses `motion`; it starts from the rest pose.
    pub fn set(&mut self, motion: Motion) {
        if motion != self.motion {
            self.motion = motion;
            self.stop();
        }
    }

    /// Back to the rest pose at time zero (after a rebuild or reset, or a new motion).
    pub fn stop(&mut self) {
        self.time = 0.0;
        self.worker = None;
        if self.body.take().is_some() {
            self.back_to_rest = true;
        }
    }

    /// Whether the body went back to rest since the last call (the solver and the renderer
    /// should get the rest avatar again).
    pub fn take_back_to_rest(&mut self) -> bool {
        std::mem::take(&mut self.back_to_rest)
    }

    /// The posed body in use, if the avatar is moving.
    #[must_use]
    pub fn body(&self) -> Option<&Arc<Avatar>> {
        self.body.as_ref()
    }

    /// Advances the clock by one played frame of `frame_dt` seconds; returns a newly posed body
    /// when one is ready.
    pub fn advance(&mut self, rest: &Arc<Avatar>, frame_dt: f64) -> Option<Arc<Avatar>> {
        if self.motion == Motion::None || !rest.can_pose() {
            return None;
        }
        let worker = self
            .worker
            .get_or_insert_with(|| PoseWorker::start(rest.clone(), self.motion));
        let mut fresh = None;
        while let Ok(body) = worker.results.try_recv() {
            worker.waiting = false;
            fresh = Some(body);
        }
        self.time += frame_dt;
        if !worker.waiting && worker.requests.send(self.time + frame_dt).is_ok() {
            worker.waiting = true;
        }
        if let Some(body) = &fresh {
            self.body = Some(body.clone());
        }
        fresh
    }
}
