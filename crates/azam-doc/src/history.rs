//! Undo and redo by snapshots: each step stores the whole project as it was.

use crate::command::{Command, Created, EditError, apply};
use crate::project::Project;

/// Undo steps kept by default.
pub const DEFAULT_HISTORY_LIMIT: usize = 150;

#[derive(Clone, Debug)]
struct Step {
    label: &'static str,
    state: Project,
}

/// Undo/redo history of one project.
#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(DEFAULT_HISTORY_LIMIT)
    }
}

impl History {
    /// An empty history keeping [`DEFAULT_HISTORY_LIMIT`] steps.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty history keeping at most `limit` undo steps.
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    /// Applies `command` and records an undo step if the project changed (which also clears the
    /// redo steps). Returns what the command created.
    ///
    /// # Errors
    /// Returns the command's error; the project and history are left unchanged.
    pub fn execute(&mut self, project: &mut Project, command: Command) -> Result<Vec<Created>, EditError> {
        let label = command.label();
        let applied = apply(project, command)?;
        if applied.changed {
            self.undo.push(Step {
                label,
                state: applied.before,
            });
            if self.undo.len() > self.limit {
                let excess = self.undo.len() - self.limit;
                self.undo.drain(..excess);
            }
            self.redo.clear();
        }
        Ok(applied.created)
    }

    /// Restores the project to before the last step. Returns the step's label, or `None` when
    /// there is nothing to undo.
    pub fn undo(&mut self, project: &mut Project) -> Option<&'static str> {
        let step = self.undo.pop()?;
        let current = std::mem::replace(project, step.state);
        self.redo.push(Step {
            label: step.label,
            state: current,
        });
        Some(step.label)
    }

    /// Re-applies the last undone step. Returns its label, or `None` when there is nothing to redo.
    pub fn redo(&mut self, project: &mut Project) -> Option<&'static str> {
        let step = self.redo.pop()?;
        let current = std::mem::replace(project, step.state);
        self.undo.push(Step {
            label: step.label,
            state: current,
        });
        Some(step.label)
    }

    /// Label of the step [`History::undo`] would undo.
    #[must_use]
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|s| s.label)
    }

    /// Label of the step [`History::redo`] would redo.
    #[must_use]
    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|s| s.label)
    }

    /// Whether there is a step to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is a step to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Forgets every step.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;
    use crate::templates::{Template, from_template};

    #[test]
    fn undo_and_redo_restore_snapshots() {
        let mut p = from_template(Template::Tee, Sex::Female, None, None, 0.0);
        let original = p.clone();
        let mut h = History::new();
        h.execute(
            &mut p,
            Command::Rename {
                name: "Box tee".into(),
            },
        )
        .expect("valid rename");
        let front = p.pieces.keys().next().cloned().expect("tee has pieces");
        let created = h
            .execute(
                &mut p,
                Command::SplitSegment {
                    piece: front,
                    segment: "hemL".into(),
                    t: 0.5,
                },
            )
            .expect("segment exists");
        assert_eq!(created.len(), 1);
        let edited = p.clone();
        assert_eq!(h.undo(&mut p), Some("Split"));
        assert_eq!(h.undo(&mut p), Some("Rename"));
        assert_eq!(p, original);
        assert_eq!(h.undo(&mut p), None);
        h.redo(&mut p);
        h.redo(&mut p);
        assert_eq!(p, edited);
    }

    #[test]
    fn refused_and_no_op_commands_leave_no_step() {
        let mut p = from_template(Template::Skirt, Sex::Female, None, None, 0.0);
        let mut h = History::new();
        let err = h.execute(&mut p, Command::Rename { name: "  ".into() });
        assert_eq!(err, Err(EditError::EmptyName));
        let units = p.units;
        h.execute(&mut p, Command::SetUnits { units }).expect("valid");
        assert!(!h.can_undo());
    }
}
