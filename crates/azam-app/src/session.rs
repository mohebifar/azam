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

    /// Applies an undoable edit named `label`.
    pub fn edit(&mut self, label: &str, f: impl FnOnce(&mut Project)) {
        let merge = self
            .last_edit
            .as_ref()
            .is_some_and(|(l, at)| l == label && at.elapsed() < MERGE_WINDOW);
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
        f(&mut self.project);
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
    fn quick_edits_with_one_label_merge_into_one_step() {
        let mut session = Session::new(new_project(Template::Tee));
        for i in 0..5 {
            session.edit("Drag", |p| p.meta.name = format!("step {i}"));
        }
        session.undo();
        assert_ne!(
            session.project.meta.name, "step 3",
            "one undo goes back past the whole drag"
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
