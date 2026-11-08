//! Auto save and recovery, as Blender's: now and then the interactive app writes the open project
//! to `autosave.azam.json` in its data folder (never over the user's own file), and File ▸ Recover
//! Auto Save opens that copy as an unsaved project.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::app::AzamApp;
use crate::session::Session;

/// File name of the auto save in the data folder.
pub const AUTOSAVE_FILE: &str = "autosave.azam.json";

/// When the project was last auto saved, and at which revision.
#[derive(Clone, Copy, Debug)]
pub struct AutosaveState {
    at: Instant,
    revision: u64,
}

impl AutosaveState {
    /// Nothing to save until the project moves past `revision`.
    #[must_use]
    pub fn at_revision(revision: u64) -> Self {
        Self {
            at: Instant::now(),
            revision,
        }
    }
}

impl AzamApp {
    /// Where auto saves go: next to the preferences (the interactive app only).
    #[must_use]
    pub fn autosave_path(&self) -> Option<PathBuf> {
        self.prefs_path.as_ref()?.parent().map(|d| d.join(AUTOSAVE_FILE))
    }

    /// Writes the auto save when it is due: auto save on, the project changed since the last one
    /// and the interval passed. Returns whether it wrote.
    pub(crate) fn autosave_if_due(&mut self) -> bool {
        if !self.prefs.autosave {
            return false;
        }
        let Some(path) = self.autosave_path() else {
            return false;
        };
        let due = Duration::from_secs_f64(self.prefs.autosave_minutes.max(1.0) * 60.0);
        if self.session.revision == self.autosave.revision || self.autosave.at.elapsed() < due {
            return false;
        }
        self.write_autosave(&path)
    }

    /// Writes the project to `path` as an auto save (the session keeps its own file).
    pub(crate) fn write_autosave(&mut self, path: &std::path::Path) -> bool {
        let written = serde_json::to_string(&self.session.project.to_value())
            .map_err(|e| e.to_string())
            .and_then(|text| {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(path, text).map_err(|e| e.to_string())
            });
        self.autosave = AutosaveState {
            at: Instant::now(),
            revision: self.session.revision,
        };
        match written {
            Ok(()) => true,
            Err(e) => {
                self.status = format!("Auto save failed: {e}");
                false
            }
        }
    }

    /// Writes the auto save now, if the app has a place for it; returns whether it wrote.
    pub fn write_autosave_now(&mut self) -> bool {
        match self.autosave_path() {
            Some(path) => self.write_autosave(&path),
            None => false,
        }
    }

    /// Opens the last auto save as an unsaved project (File ▸ Recover Auto Save).
    pub fn recover_autosave(&mut self) {
        let Some(path) = self.autosave_path().filter(|p| p.exists()) else {
            self.status = "There is no auto save to recover".into();
            return;
        };
        match Session::load(&path) {
            Ok(project) => {
                let when = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map_or_else(String::new, |d| format!(" from {} min ago", d.as_secs() / 60));
                self.session.replace(project, None);
                self.framed = false;
                self.status = format!("Recovered the auto save{when} (not saved yet: File ▸ Save As)");
            }
            Err(e) => self.status = format!("Could not recover the auto save: {e}"),
        }
    }
}
