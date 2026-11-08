//! The open project: the document, its selection, undo history and file.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use azam_doc::Project;
use azam_sync::Collab;

use crate::selection::Selection;

/// Most undo steps kept.
const UNDO_LIMIT: usize = 128;
/// Edits with the same label closer together than this merge into one undo step (a slider drag).
const MERGE_WINDOW: Duration = Duration::from_millis(900);

/// A project state to return to.
struct Snapshot {
    project: Project,
    selection: Selection,
    label: String,
    /// While collaborating, the state the step led to: undo reverts only the change from it to
    /// `project`, keeping collaborators' edits made since.
    after: Option<Project>,
}

/// The project being edited.
pub struct Session {
    /// The document.
    pub project: Project,
    /// What is selected.
    pub selection: Selection,
    /// Where the project was loaded from or last saved to.
    pub path: Option<PathBuf>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<(String, Instant)>,
    /// A gesture in progress (a drag, a modal transform): `Some(true)` once it has recorded its
    /// undo step.
    gesture: Option<bool>,
    /// Bumped on every change, so derived data (meshes, the simulation) knows to rebuild.
    pub revision: u64,
    saved_revision: u64,
    /// Live collaboration, while the project is shared or joined.
    pub collab: Option<Collab>,
    /// Whether the next remote project replaces this one outright (just joined a session).
    adopt_remote: bool,
}

impl Session {
    /// A session over `project`.
    #[must_use]
    pub fn new(project: Project) -> Self {
        Self {
            project,
            selection: Selection::default(),
            path: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            gesture: None,
            revision: 1,
            saved_revision: 1,
            collab: None,
            adopt_remote: false,
        }
    }

    /// Whether there are unsaved changes.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Applies an undoable edit named `label`, as its own undo step (inside a gesture, as part of
    /// the gesture's one step).
    pub fn edit(&mut self, label: &str, f: impl FnOnce(&mut Project)) {
        self.apply(label, false, f);
    }

    /// Applies an edit from a continuous control (a number field being scrubbed): edits with the
    /// same label in quick succession make one undo step.
    pub fn edit_continuous(&mut self, label: &str, f: impl FnOnce(&mut Project)) {
        self.apply(label, true, f);
    }

    /// Starts a gesture: every edit until [`Self::end_gesture`] makes one undo step.
    pub fn begin_gesture(&mut self) {
        self.gesture = Some(false);
    }

    /// Ends the gesture; without `keep`, everything it changed is put back and it leaves no
    /// trace in the history (nothing to redo either).
    pub fn end_gesture(&mut self, keep: bool) {
        let recorded = self.gesture.take() == Some(true);
        self.last_edit = None;
        if !keep
            && recorded
            && let Some(snap) = self.undo.pop()
        {
            self.restore(snap);
        }
    }

    fn apply(&mut self, label: &str, continuous: bool, f: impl FnOnce(&mut Project)) {
        let merge = match self.gesture {
            Some(recorded) => recorded,
            None => {
                continuous
                    && self
                        .last_edit
                        .as_ref()
                        .is_some_and(|(l, at)| l == label && at.elapsed() < MERGE_WINDOW)
            }
        };
        if self.gesture.is_some() {
            self.gesture = Some(true);
        }
        if !merge {
            self.undo.push(Snapshot {
                project: self.project.clone(),
                selection: self.selection.clone(),
                label: label.to_owned(),
                after: None,
            });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        let lengths = azam_doc::link_lengths(&self.project);
        let symmetric = azam_doc::symmetry_snapshot(&self.project);
        f(&mut self.project);
        azam_doc::sync_symmetry(&mut self.project, &symmetric);
        azam_doc::sync_length_links(&mut self.project, &lengths);
        if self.collab.is_some()
            && let Some(top) = self.undo.last_mut()
        {
            top.after = Some(self.project.clone());
        }
        self.selection.retain_existing(&self.project);
        self.last_edit = Some((label.to_owned(), Instant::now()));
        self.push_sync();
        self.revision += 1;
    }

    /// Undoes the last edit; returns its label.
    pub fn undo(&mut self) -> Option<String> {
        let snap = self.undo.pop()?;
        let label = snap.label.clone();
        let back = self.restore(snap);
        self.redo.push(back);
        Some(label)
    }

    /// Redoes the last undone edit; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        let snap = self.redo.pop()?;
        let label = snap.label.clone();
        let back = self.restore(snap);
        self.undo.push(back);
        Some(label)
    }

    /// Returns to `snap` and gives the snapshot that comes back. While collaborating, only the
    /// step's own change is reverted, on top of the current (merged) project.
    fn restore(&mut self, snap: Snapshot) -> Snapshot {
        let Snapshot {
            project: target,
            selection,
            label,
            after,
        } = snap;
        let (back_project, back_after) = match after {
            Some(after) if self.collab.is_some() => {
                self.project = azam_sync::rebase(&after, &target, &self.project);
                (after, Some(target))
            }
            _ => {
                let back_after = self.collab.is_some().then(|| target.clone());
                (std::mem::replace(&mut self.project, target), back_after)
            }
        };
        let back = Snapshot {
            project: back_project,
            selection: std::mem::replace(&mut self.selection, selection),
            label,
            after: back_after,
        };
        self.selection.retain_existing(&self.project);
        self.last_edit = None;
        self.push_sync();
        self.revision += 1;
        back
    }

    /// The undo history: the steps done (oldest first; the last is where the project is now) and
    /// the steps undone (in the order Redo would bring them back).
    #[must_use]
    pub fn history(&self) -> (Vec<String>, Vec<String>) {
        (
            self.undo.iter().map(|s| s.label.clone()).collect(),
            self.redo.iter().rev().map(|s| s.label.clone()).collect(),
        )
    }

    /// Goes to step `step` of the history: 0 is before the first step done, the number of steps
    /// done is now, beyond that steps undone come back. Returns how many steps it moved.
    pub fn jump_to(&mut self, step: usize) -> usize {
        let done = self.undo.len();
        let mut moved = 0;
        if step < done {
            while self.undo.len() > step && self.undo().is_some() {
                moved += 1;
            }
        } else {
            let target = step.min(done + self.redo.len());
            while self.undo.len() < target && self.redo().is_some() {
                moved += 1;
            }
        }
        moved
    }

    /// Replaces the project (New, Open), clearing the history. Stops sharing the old one.
    pub fn replace(&mut self, project: Project, path: Option<PathBuf>) {
        self.collab = None;
        self.project = project;
        self.selection.clear();
        self.path = path;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.revision += 1;
        self.saved_revision = self.revision;
    }

    /// Shares the project through `collab` (hosting or the shared update log), or, when
    /// `joining`, waits for the session's project and then shows it in place of this one.
    pub fn start_collab(&mut self, collab: Collab, joining: bool) {
        self.collab = Some(collab);
        self.adopt_remote = joining;
        self.push_sync();
    }

    /// Stops sharing.
    pub fn stop_collab(&mut self) {
        self.collab = None;
        self.adopt_remote = false;
    }

    /// Sends the current project's changes to collaborators.
    fn push_sync(&mut self) {
        if let Some(collab) = &mut self.collab {
            collab.local_edit(&mut self.project);
        }
    }

    /// Takes in collaborators' changes; returns whether the project changed. Call between frames.
    pub fn pull_sync(&mut self) -> bool {
        let Some(project) = self.collab.as_mut().and_then(Collab::poll) else {
            return false;
        };
        self.project = project;
        if std::mem::take(&mut self.adopt_remote) {
            // A joined session's project replaces ours: our history belongs to another project.
            self.selection.clear();
            self.path = None;
            self.undo.clear();
            self.redo.clear();
        }
        self.selection.retain_existing(&self.project);
        self.last_edit = None;
        self.revision += 1;
        true
    }

    /// Loads a project file.
    ///
    /// # Errors
    /// Fails if the file cannot be read or is not a project.
    pub fn load(path: &Path) -> Result<Project, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        Project::from_json(&text).map_err(|e| format!("{} is not a project file: {e}", path.display()))
    }

    /// Saves the project to `path` and remembers it.
    ///
    /// # Errors
    /// Fails if the file cannot be written.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.project.to_value()).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))?;
        self.path = Some(path.to_owned());
        self.saved_revision = self.revision;
        // The saved file holds everything: a shared update log can shrink to one snapshot.
        if let Some(collab) = &self.collab {
            collab.compact_log();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::new_project;
    use crate::operators::Template;

    fn rename(session: &mut Session, label: &str, name: &str) {
        let name = name.to_owned();
        session.edit(label, move |p| p.meta.name = name);
    }

    #[test]
    fn a_length_link_follows_point_edits_and_undoes_with_them() {
        let mut session = Session::new(new_project(Template::Skirt));
        let ids: Vec<azam_doc::PieceId> = session.project.pieces.keys().take(2).cloned().collect();
        // each skirt panel's second segment (a side seam)
        let side = |s: &Session, i: usize| {
            azam_doc::SegmentRef::new(&ids[i], &s.project.pieces[&ids[i]].outline().segments[1].id)
        };
        let (a, b) = (side(&session, 0), side(&session, 1));
        session.edit("Link Lengths", |p| {
            assert!(azam_doc::add_length_link(p, &a, &b, 3.0));
        });
        let len =
            |s: &Session, r: &azam_doc::SegmentRef| azam_doc::segment_length(&s.project, r).expect("segment");
        assert!((len(&session, &b) - len(&session, &a) - 3.0).abs() < 0.01);
        // drag the end of a's side down 25 mm: b follows
        let end = session.project.pieces[&ids[0]].outline().segments[1].end_point;
        session.edit("Move Points", |p| {
            p.pieces.get_mut(&ids[0]).expect("piece").pts[end].pos.y += 25.0;
        });
        assert!(
            (len(&session, &b) - len(&session, &a) - 3.0).abs() < 0.01,
            "b followed"
        );
        let moved = len(&session, &b);
        session.undo();
        assert!(len(&session, &b) < moved - 1.0, "undo puts the follower back too");
    }

    #[test]
    fn symmetric_editing_mirrors_point_moves_and_undoes_with_them() {
        let mut session = Session::new(new_project(Template::Tee));
        let front = session
            .project
            .pieces
            .values()
            .find(|pc| pc.name == "Front")
            .expect("front")
            .id
            .clone();
        let mut axis = None;
        session.edit("Symmetric Editing", |p| {
            axis = azam_doc::toggle_symmetry(p, &front);
        });
        let axis = axis.expect("on");
        let at = |s: &Session, id: &str| {
            let pc = &s.project.pieces[&front];
            pc.pts[pc.point_index(&azam_geom::PointId::new(id)).expect("point")].pos
        };
        let right_before = at(&session, "shR");
        session.edit("Move Points", |p| {
            let pc = p.pieces.get_mut(&front).expect("front");
            let k = pc.point_index(&azam_geom::PointId::new("shL")).expect("point");
            pc.pts[k].pos.x -= 15.0;
            pc.pts[k].pos.y += 6.0;
        });
        let (l, r) = (at(&session, "shL"), at(&session, "shR"));
        assert!(
            (l.x + r.x - 2.0 * axis).abs() < 1e-9 && (l.y - r.y).abs() < 1e-9,
            "mirrored"
        );
        session.undo();
        assert_eq!(at(&session, "shR"), right_before, "undo puts the mirror back too");
    }

    #[test]
    fn the_history_lists_steps_and_jumps_to_any() {
        let mut session = Session::new(new_project(Template::Tee));
        rename(&mut session, "Rename A", "a");
        rename(&mut session, "Rename B", "b");
        rename(&mut session, "Rename C", "c");
        assert_eq!(
            session.history(),
            (
                vec!["Rename A".into(), "Rename B".into(), "Rename C".into()],
                vec![]
            )
        );
        assert_eq!(session.jump_to(1), 2, "two steps back");
        assert_eq!(session.project.meta.name, "a");
        let (done, undone) = session.history();
        assert_eq!(done, ["Rename A"]);
        assert_eq!(undone, ["Rename B", "Rename C"], "in the order they come back");
        assert_eq!(session.jump_to(3), 2);
        assert_eq!(session.project.meta.name, "c");
        session.jump_to(0);
        assert_ne!(session.project.meta.name, "a", "the original");
        assert_eq!(session.jump_to(99), 3, "clamped to the newest step");
    }

    #[test]
    fn quick_discrete_edits_stay_separate_undo_steps() {
        let mut session = Session::new(new_project(Template::Tee));
        rename(&mut session, "Add Dart", "one");
        rename(&mut session, "Add Dart", "two");
        assert_eq!(session.undo().as_deref(), Some("Add Dart"));
        assert_eq!(
            session.project.meta.name, "one",
            "the second dart alone is undone"
        );
    }

    #[test]
    fn a_cancelled_gesture_leaves_nothing_to_undo_or_redo() {
        let mut session = Session::new(new_project(Template::Tee));
        rename(&mut session, "Rename", "before");
        session.begin_gesture();
        rename(&mut session, "Move Pieces", "a");
        rename(&mut session, "Move Pieces", "b");
        session.end_gesture(false);
        assert_eq!(session.project.meta.name, "before");
        assert!(session.redo().is_none(), "nothing to redo");
        assert_eq!(
            session.undo().as_deref(),
            Some("Rename"),
            "the edit before the gesture is next"
        );
    }

    #[test]
    fn a_kept_gesture_is_one_step_and_scrubbing_merges() {
        let mut session = Session::new(new_project(Template::Tee));
        let start = session.project.meta.name.clone();
        session.begin_gesture();
        rename(&mut session, "Move Pieces", "a");
        rename(&mut session, "Move Pieces", "b");
        session.end_gesture(true);
        let name = |s: &str| s.to_owned();
        session.edit_continuous("Grain", |p| p.meta.name = name("c"));
        session.edit_continuous("Grain", |p| p.meta.name = name("d"));
        assert_eq!(session.undo().as_deref(), Some("Grain"));
        assert_eq!(
            session.project.meta.name, "b",
            "both scrubbed values go in one step"
        );
        assert_eq!(session.undo().as_deref(), Some("Move Pieces"));
        assert_eq!(session.project.meta.name, start, "the whole drag is one step");
    }

    #[test]
    fn save_then_load_round_trips() {
        let mut session = Session::new(new_project(Template::Labadeh));
        let dir = std::env::temp_dir().join(format!("azam-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("labadeh.azam.json");
        session.save(&path).unwrap();
        assert!(!session.is_dirty());
        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.to_value(), session.project.to_value());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn undo_and_redo_restore_edits() {
        let mut session = Session::new(new_project(Template::Tee));
        let before = session.project.meta.name.clone();
        session.edit("Rename", |p| p.meta.name = "Renamed".into());
        assert!(session.is_dirty());
        assert_eq!(session.undo().as_deref(), Some("Rename"));
        assert_eq!(session.project.meta.name, before);
        assert_eq!(session.redo().as_deref(), Some("Rename"));
        assert_eq!(session.project.meta.name, "Renamed");
        assert!(session.redo().is_none());
    }

    #[test]
    fn quick_continuous_edits_with_one_label_merge_into_one_step() {
        let mut session = Session::new(new_project(Template::Tee));
        for i in 0..5 {
            session.edit_continuous("Scrub", |p| p.meta.name = format!("step {i}"));
        }
        session.undo();
        assert_ne!(
            session.project.meta.name, "step 3",
            "one undo goes back past the whole scrub"
        );
        assert!(session.undo().is_none());
    }

    fn local_config(name: &str) -> azam_sync::Config {
        azam_sync::Config {
            presence: azam_sync::Presence::named(name, 0),
            listen: Some(std::net::SocketAddr::from(([127, 0, 0, 1], 0))),
            discovery: false,
            wake: None,
        }
    }

    /// Pulls on both sessions until `done` holds, for up to ten seconds.
    fn pump(a: &mut Session, b: &mut Session, done: impl Fn(&Session, &Session) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            a.pull_sync();
            b.pull_sync();
            if done(a, b) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn shared_sessions_follow_each_others_edits_undo_and_redo() {
        let mut a = Session::new(new_project(Template::Tee));
        let host = Collab::host(&a.project, None, local_config("a")).unwrap();
        let addr = host.status().listen.unwrap();
        a.start_collab(host, false);
        let mut b = Session::new(new_project(Template::Skirt));
        b.edit("Rename", |p| p.meta.name = "mine".into());
        let join = Collab::join(azam_sync::JoinTarget::Addr(addr), local_config("b")).unwrap();
        b.start_collab(join, true);
        let same = |a: &Session, b: &Session| a.project.to_value() == b.project.to_value();
        assert!(pump(&mut a, &mut b, same), "b adopts a's project");
        assert!(b.undo().is_none(), "b's history of its own project is gone");

        a.edit("Rename", |p| p.meta.name = "Shared tee".into());
        b.edit("Avatar", |p| p.avatar.show = !p.avatar.show);
        assert!(pump(&mut a, &mut b, same), "concurrent edits merge");
        assert_eq!(b.project.meta.name, "Shared tee");
        let shown = a.project.avatar.show;
        a.undo();
        assert!(pump(&mut a, &mut b, |a, b| same(a, b)
            && b.project.meta.name != "Shared tee"));
        assert_eq!(b.project.avatar.show, shown, "undo keeps b's edit");
        a.redo();
        assert!(pump(&mut a, &mut b, |a, b| same(a, b)
            && b.project.meta.name == "Shared tee"));
    }
}
