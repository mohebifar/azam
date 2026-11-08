//! Preferences, as Blender's Preferences window keeps them: interface options and a keymap of
//! the user's own shortcuts, saved as `prefs.json` in azam's data folder (next to the fabric
//! library). Custom shortcuts take precedence over the built-in keys and show in menus and
//! tooltips in their place.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use blenderui::prelude::*;
use serde::{Deserialize, Serialize};

use crate::app::AzamApp;
use crate::operators::Operator;

/// File name of the preferences in the data folder.
pub const PREFS_FILE: &str = "prefs.json";

/// The user's preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Show tooltips when the pointer rests on a control.
    pub tooltips: bool,
    /// Show tool names beside the icons in the tool shelves.
    pub tool_labels: bool,
    /// Custom shortcuts: key combination (see [`combo_of`]) to the label of the operator it runs.
    pub keymap: BTreeMap<String, String>,
    /// Save a copy of the project to the data folder now and then (File ▸ Recover Auto Save).
    pub autosave: bool,
    /// Minutes between auto saves.
    pub autosave_minutes: f64,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            tooltips: true,
            tool_labels: false,
            keymap: BTreeMap::new(),
            autosave: true,
            autosave_minutes: 2.0,
        }
    }
}

impl Prefs {
    /// The preferences saved at `path` (the defaults if there are none or they do not parse).
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Saves the preferences at `path`.
    ///
    /// # Errors
    /// Fails if the folder cannot be made or the file written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// The custom combination bound to `op`, if any.
    #[must_use]
    pub fn combo_for(&self, op: Operator) -> Option<&str> {
        let label = op.label();
        self.keymap
            .iter()
            .find(|(_, l)| **l == label)
            .map(|(c, _)| c.as_str())
    }

    /// Binds `combo` to `op`, replacing what either was bound to before.
    pub fn bind(&mut self, combo: &str, op: Operator) {
        let label = op.label();
        self.keymap.retain(|_, l| *l != label);
        self.keymap.insert(combo.to_owned(), label);
    }

    /// Removes `op`'s custom shortcut; returns `false` if it had none.
    pub fn unbind(&mut self, op: Operator) -> bool {
        let label = op.label();
        let before = self.keymap.len();
        self.keymap.retain(|_, l| *l != label);
        self.keymap.len() != before
    }

    /// The operator a custom shortcut binds `combo` to.
    #[must_use]
    pub fn operator_for(&self, combo: &str) -> Option<Operator> {
        let label = self.keymap.get(combo)?;
        Operator::searchable().into_iter().find(|op| op.label() == *label)
    }
}

/// The preferences file in azam's data folder, if one is known.
#[must_use]
pub fn prefs_path() -> Option<PathBuf> {
    crate::fabric_library::data_dir().map(|d| d.join(PREFS_FILE))
}

/// A key combination as stored: modifiers `ctrl+`, `alt+`, `shift+` (in that order), then the
/// key in lower case (`ctrl+shift+k`, `f5`). `None` for a bare modifier.
#[must_use]
pub fn combo_of(key: &Keystroke) -> Option<String> {
    let k = key.key.to_ascii_lowercase();
    if k.is_empty()
        || matches!(
            k.as_str(),
            "shift_l"
                | "shift_r"
                | "control_l"
                | "control_r"
                | "alt_l"
                | "alt_r"
                | "meta_l"
                | "meta_r"
                | "super_l"
                | "super_r"
        )
    {
        return None;
    }
    let mut out = String::new();
    for (on, name) in [
        (key.modifiers.control, "ctrl+"),
        (key.modifiers.alt, "alt+"),
        (key.modifiers.shift, "shift+"),
    ] {
        if on {
            out.push_str(name);
        }
    }
    out.push_str(&k);
    Some(out)
}

/// A combination as menus show shortcuts: `Shift Ctrl K`.
#[must_use]
pub fn combo_label(combo: &str) -> String {
    let mut parts: Vec<&str> = combo.split('+').collect();
    let key = parts.pop().unwrap_or("");
    let mut out = Vec::new();
    for (stored, shown) in [("shift", "Shift"), ("ctrl", "Ctrl"), ("alt", "Alt")] {
        if parts.contains(&stored) {
            out.push(shown.to_owned());
        }
    }
    out.push(if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        capitalize(key)
    });
    out.join(" ")
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// The Preferences window's state while it is open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrefsView {
    /// The section shown.
    pub keymap: bool,
    /// The keymap filter text.
    pub filter: String,
    /// The operator waiting for its new shortcut (the next key press).
    pub capturing: Option<Operator>,
}

impl AzamApp {
    /// The shortcut shown for `op`: the user's own, else the built-in one.
    #[must_use]
    pub fn shortcut_label(&self, op: Operator) -> Option<String> {
        self.prefs
            .combo_for(op)
            .map(combo_label)
            .or_else(|| op.shortcut())
    }

    /// Saves the preferences, when they have a file (the interactive app; tests have none).
    pub(crate) fn save_prefs(&mut self) {
        if let Some(path) = &self.prefs_path
            && let Err(e) = self.prefs.save(path)
        {
            self.status = format!("Could not save the preferences: {e}");
        }
    }

    /// Uses the preferences at `path` and saves changes there.
    pub fn use_prefs_file(&mut self, path: PathBuf) {
        self.prefs = Prefs::load(&path);
        self.tool_labels = self.prefs.tool_labels;
        self.prefs_path = Some(path);
    }

    /// Whether the Preferences window is open.
    #[must_use]
    pub fn prefs_open(&self) -> bool {
        self.prefs_view.is_some()
    }

    /// The preferences in effect.
    #[must_use]
    pub fn prefs(&self) -> &Prefs {
        &self.prefs
    }

    /// Takes a key press for the Preferences window; returns `true` if it used it (a shortcut
    /// being captured, or Escape closing the window).
    pub(crate) fn prefs_key(&mut self, key: &Keystroke) -> bool {
        let Some(view) = &mut self.prefs_view else {
            return false;
        };
        if let Some(op) = view.capturing {
            if key.key == "Escape" {
                view.capturing = None;
                return true;
            }
            let Some(combo) = combo_of(key) else {
                return true;
            };
            view.capturing = None;
            self.prefs.bind(&combo, op);
            self.status = format!("{} now runs {}", combo_label(&combo), op.label());
            self.save_prefs();
            return true;
        }
        if key.key == "Escape" {
            self.prefs_view = None;
            return true;
        }
        false
    }

    /// Runs the operator the user bound to `key`, if any; returns `true` if one ran.
    pub(crate) fn custom_shortcut(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(op) = combo_of(key).and_then(|c| self.prefs.operator_for(&c)) else {
            return false;
        };
        self.run(op, window, cx);
        true
    }

    /// The Preferences window: Interface and Keymap sections.
    pub(crate) fn prefs_popup(&mut self, view: &PrefsView, cx: &mut Context<Self>) -> Div {
        let tab = |label: &'static str, keymap: bool, on: bool, cx: &mut Context<Self>| {
            div()
                .class(if on { "prefs-tab on" } else { "prefs-tab" })
                .id(format!("prefs-tab-{label}"))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(v) = &mut this.prefs_view {
                        v.keymap = keymap;
                        v.capturing = None;
                    }
                    cx.notify();
                }))
        };
        let header = div()
            .class("prefs-head")
            .child(self.icons.icon("PREFERENCES"))
            .child(div().class("prefs-title").child("Preferences"))
            .child(tab("Interface", false, !view.keymap, cx))
            .child(tab("Keymap", true, view.keymap, cx))
            .child(div().class("header-spacer"))
            .child(div().class("pbtn").child("Close").on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| {
                    this.prefs_view = None;
                    window.focus_id("root");
                    cx.notify();
                },
            )));
        let body = if view.keymap {
            self.keymap_section(view, cx)
        } else {
            self.interface_section(cx)
        };
        div()
            .class("prefs")
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
    }

    fn interface_section(&mut self, cx: &mut Context<Self>) -> Div {
        let check =
            |label: &'static str, on: bool, set: fn(&mut AzamApp, bool), cx: &mut Context<Self>| {
                div()
                    .class("prow")
                    .child(div().class("plabel").child(label))
                    .child(Checkbox().checked(on).on_change(cx.listener(
                        move |this, e: &ChangeEvent, _, cx| {
                            set(this, e.checked);
                            this.save_prefs();
                            cx.notify();
                        },
                    )))
            };
        div()
            .class("prefs-body")
            .child(check(
                "Tooltips",
                self.prefs.tooltips,
                |a, v| a.prefs.tooltips = v,
                cx,
            ))
            .child(check(
                "Tool Names in Shelf",
                self.prefs.tool_labels,
                |a, v| {
                    a.prefs.tool_labels = v;
                    a.tool_labels = v;
                },
                cx,
            ))
            .child(check(
                "Auto Save",
                self.prefs.autosave,
                |a, v| a.prefs.autosave = v,
                cx,
            ))
            .child(
                div().class("prow").child(
                    NumberField()
                        .attr("label", "Minutes Between Saves")
                        .value(format!("{:.0}", self.prefs.autosave_minutes))
                        .min(1.0)
                        .max(60.0)
                        .step(1.0)
                        .class("pfield")
                        .on_input(cx.listener(|this, e: &ChangeEvent, _, cx| {
                            this.prefs.autosave_minutes = e.number.clamp(1.0, 60.0);
                            this.save_prefs();
                            cx.notify();
                        })),
                ),
            )
            .child(div().class("pnote").child(match &self.prefs_path {
                Some(p) => format!("Saved in {}", p.display()),
                None => "Not saved (no data folder).".to_owned(),
            }))
    }

    fn keymap_section(&mut self, view: &PrefsView, cx: &mut Context<Self>) -> Div {
        let filter = view.filter.to_lowercase();
        let mut rows = Vec::new();
        for op in Operator::searchable() {
            let label = op.label();
            if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                continue;
            }
            let custom = self.prefs.combo_for(op).map(combo_label);
            let shown = custom.clone().or_else(|| op.shortcut()).unwrap_or_default();
            let capturing = view.capturing == Some(op);
            let set = div()
                .class(if capturing { "pbtn primary" } else { "pbtn" })
                .id(format!("bind-{label}"))
                .child(if capturing { "Press a key…" } else { "Set" })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(v) = &mut this.prefs_view {
                        v.capturing = Some(op);
                    }
                    cx.notify();
                }));
            let mut row = div()
                .class("keymap-row")
                .child(div().class("keymap-name").child(label.clone()))
                .child(
                    div()
                        .class(if custom.is_some() {
                            "keymap-key custom"
                        } else {
                            "keymap-key"
                        })
                        .child(shown),
                )
                .child(set);
            if custom.is_some() {
                row = row.child(
                    div()
                        .class("pbtn")
                        .id(format!("unbind-{label}"))
                        .child("Reset")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.prefs.unbind(op);
                            this.save_prefs();
                            cx.notify();
                        })),
                );
            }
            rows.push(row);
        }
        div()
            .class("prefs-body")
            .child(
                input()
                    .id("keymap-filter")
                    .attr("live", "")
                    .placeholder("Filter commands")
                    .value(view.filter.clone())
                    .w_full()
                    .on_input(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        if let Some(v) = &mut this.prefs_view {
                            v.filter.clone_from(&e.value);
                        }
                        cx.notify();
                    })),
            )
            .child(div().class("keymap-list").id("keymap-list").children(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_bind_save_and_load() {
        let mut p = Prefs::default();
        let op = Operator::SetTool(crate::operators::Tool::Measure);
        p.bind("ctrl+shift+m", op);
        assert_eq!(p.operator_for("ctrl+shift+m"), Some(op));
        p.bind("q", op);
        assert_eq!(
            p.operator_for("ctrl+shift+m"),
            None,
            "an operator has one custom shortcut"
        );
        assert_eq!(combo_label("ctrl+shift+m"), "Shift Ctrl M");
        assert_eq!(combo_label("f5"), "F5");
        let path = std::env::temp_dir().join(format!("azam-prefs-{}.json", std::process::id()));
        p.save(&path).expect("saved");
        assert_eq!(Prefs::load(&path), p);
        let _ = std::fs::remove_file(&path);
        assert!(p.unbind(op) && p.keymap.is_empty());
    }
}
