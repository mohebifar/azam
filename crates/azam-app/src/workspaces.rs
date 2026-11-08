//! The workspace tabs, as Blender's top bar keeps them: a list the user adds to (from any of the
//! built-in kinds), duplicates, renames, reorders and deletes. Each tab has its own area layout,
//! keyed by its name, and the list is saved in the preferences.

use blenderui::prelude::*;
use serde::{Deserialize, Serialize};

use crate::app::{AzamApp, Workspace};

/// One workspace tab: its name and the kind it was made from (which sets its default areas and
/// what switching to it does).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTab {
    /// Name on the tab, unique among the tabs.
    pub name: String,
    /// The built-in kind it was made from, by name.
    pub kind: String,
}

impl WorkspaceTab {
    /// A tab of `kind` named `name`.
    #[must_use]
    pub fn new(name: &str, kind: Workspace) -> Self {
        Self {
            name: name.to_owned(),
            kind: kind.name().to_owned(),
        }
    }

    /// The kind it was made from (Design when the saved name is unknown).
    #[must_use]
    pub fn workspace(&self) -> Workspace {
        Workspace::from_name(&self.kind).unwrap_or(Workspace::Design)
    }
}

/// The built-in tabs a new installation starts with.
#[must_use]
pub fn default_tabs() -> Vec<WorkspaceTab> {
    Workspace::ALL
        .into_iter()
        .map(|w| WorkspaceTab::new(w.name(), w))
        .collect()
}

/// `base`, or `base.001`, `base.002`… (as Blender numbers copies), whichever no tab in `tabs` has.
#[must_use]
pub fn unique_name(base: &str, tabs: &[WorkspaceTab]) -> String {
    let taken = |n: &str| tabs.iter().any(|t| t.name == n);
    // a copy of a copy numbers from the original name
    let stem = match base.rsplit_once('.') {
        Some((s, n)) if n.len() == 3 && n.chars().all(|c| c.is_ascii_digit()) => s,
        _ => base,
    };
    if !taken(base) {
        return base.to_owned();
    }
    // there are fewer tabs than numbers to try, so one is always free
    (1..=tabs.len() + 1)
        .map(|i| format!("{stem}.{i:03}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| base.to_owned())
}

impl AzamApp {
    /// The workspace tabs, in order.
    #[must_use]
    pub fn workspace_tabs(&self) -> &[WorkspaceTab] {
        &self.tabs
    }

    /// Index of the active tab.
    #[must_use]
    pub fn active_tab(&self) -> usize {
        self.active_tab
    }

    /// The active tab's name (its layout's key).
    pub(crate) fn active_tab_name(&self) -> String {
        self.tabs
            .get(self.active_tab)
            .map_or_else(|| Workspace::Design.name().to_owned(), |t| t.name.clone())
    }

    /// Switches to tab `index`.
    pub fn activate_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
            self.maximized = None;
            self.workspace_switched();
        }
    }

    /// Adds a tab of `kind` after the active one (named after the kind) and switches to it.
    pub fn add_workspace(&mut self, kind: Workspace) {
        let name = unique_name(kind.name(), &self.tabs);
        self.tabs
            .insert(self.active_tab + 1, WorkspaceTab::new(&name, kind));
        self.status = format!("Workspace \"{name}\" added");
        self.activate_tab(self.active_tab + 1);
        self.workspaces_changed();
    }

    /// Copies the active tab, layout and all, next to it and switches to the copy.
    pub fn duplicate_workspace(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab).cloned() else {
            return;
        };
        let name = unique_name(&tab.name, &self.tabs);
        let layout = self.layout();
        self.layouts.insert(name.clone(), layout);
        self.tabs.insert(
            self.active_tab + 1,
            WorkspaceTab {
                name: name.clone(),
                kind: tab.kind,
            },
        );
        self.status = format!("Workspace \"{name}\" added");
        self.activate_tab(self.active_tab + 1);
        self.workspaces_changed();
    }

    /// Renames the active tab; refused (`false`) for an empty name or one another tab has.
    pub fn rename_workspace(&mut self, name: &str) -> bool {
        let name = name.trim();
        let current = self.active_tab_name();
        if name.is_empty() || (name != current && self.tabs.iter().any(|t| t.name == name)) {
            self.status = if name.is_empty() {
                "A workspace needs a name".into()
            } else {
                format!("There is a workspace named \"{name}\" already")
            };
            return false;
        }
        if let Some(layout) = self.layouts.remove(&current) {
            self.layouts.insert(name.to_owned(), layout);
        }
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            name.clone_into(&mut tab.name);
        }
        self.status = format!("Workspace renamed to \"{name}\"");
        self.workspaces_changed();
        true
    }

    /// Deletes the active tab and its layout; the last tab stays (`false`).
    pub fn delete_workspace(&mut self) -> bool {
        if self.tabs.len() < 2 {
            self.status = "The last workspace stays".into();
            return false;
        }
        let tab = self.tabs.remove(self.active_tab);
        self.layouts.remove(&tab.name);
        self.status = format!("Workspace \"{}\" deleted", tab.name);
        self.activate_tab(self.active_tab.min(self.tabs.len() - 1));
        self.workspaces_changed();
        true
    }

    /// Moves the active tab `delta` places along the bar (stopping at the ends).
    pub fn move_workspace(&mut self, delta: isize) {
        let to = self
            .active_tab
            .saturating_add_signed(delta)
            .min(self.tabs.len().saturating_sub(1));
        if to != self.active_tab {
            let tab = self.tabs.remove(self.active_tab);
            self.tabs.insert(to, tab);
            self.active_tab = to;
            self.workspaces_changed();
        }
    }

    /// Opens the rename prompt for the active tab.
    pub fn start_rename_workspace(&mut self) {
        self.rename_prompt = Some(self.active_tab_name());
    }

    /// Renames the active tab to the rename prompt's text and closes it (it stays open when the
    /// name is refused).
    pub(crate) fn confirm_rename_workspace(&mut self, window: &mut Window) {
        let Some(text) = self.rename_prompt.clone() else {
            return;
        };
        if self.rename_workspace(&text) {
            self.rename_prompt = None;
            window.focus_id("root");
        }
    }

    /// The rename prompt.
    pub(crate) fn rename_popup(&mut self, text: &str, cx: &mut Context<Self>) -> Div {
        div()
            .class("search")
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(
                div()
                    .class("prop-title")
                    .child(self.icons.icon("GREASEPENCIL"))
                    .child("Rename Workspace"),
            )
            .child(
                input()
                    .id("rename-workspace-input")
                    .attr("autofocus", "")
                    .attr("live", "")
                    .value(text.to_owned())
                    .w_full()
                    .on_input(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        if let Some(t) = &mut this.rename_prompt {
                            t.clone_from(&e.value);
                        }
                        cx.notify();
                    })),
            )
            .child(div().class("pbuttons").mt(px(6.0)).child(
                div().class("pbtn primary").child("Rename").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| {
                        this.confirm_rename_workspace(window);
                        cx.notify();
                    },
                )),
            ))
    }

    /// Saves the tabs and their layouts in the preferences.
    pub(crate) fn workspaces_changed(&mut self) {
        self.prefs.workspaces.clone_from(&self.tabs);
        self.layouts_changed();
    }

    /// Takes the tabs saved in the preferences, if they make sense (unique, non-empty names).
    pub(crate) fn load_workspaces(&mut self) {
        let saved = &self.prefs.workspaces;
        let unique = saved
            .iter()
            .enumerate()
            .all(|(i, t)| !t.name.trim().is_empty() && !saved[..i].iter().any(|u| u.name == t.name));
        if !saved.is_empty() && unique {
            self.tabs.clone_from(saved);
            self.active_tab = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_numbered_from_the_original_name() {
        let mut tabs = default_tabs();
        assert_eq!(unique_name("Fitting", &tabs), "Fitting");
        assert_eq!(unique_name("Design", &tabs), "Design.001");
        tabs.push(WorkspaceTab::new("Design.001", Workspace::Design));
        assert_eq!(unique_name("Design.001", &tabs), "Design.002");
        assert_eq!(unique_name("Design", &tabs), "Design.002");
        assert_eq!(
            WorkspaceTab::new("x", Workspace::Review).workspace(),
            Workspace::Review
        );
        let odd = WorkspaceTab {
            name: "x".into(),
            kind: "Nonsense".into(),
        };
        assert_eq!(odd.workspace(), Workspace::Design);
    }
}
