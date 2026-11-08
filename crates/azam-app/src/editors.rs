//! Editor types, as in Blender: every area shows one editor, and the editor-type button in its
//! header switches it. Choosing an editor another area already shows swaps the two.

use std::collections::BTreeMap;

use blenderui::prelude::*;

use crate::app::{AzamApp, Workspace};
use crate::operators::{Menu, MenuEntry, Operator};

/// An editor an area can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Editor {
    /// The 3D view with the avatar and the draped garment.
    View3d,
    /// The 2D pattern editor.
    Pattern,
    /// The tech pack sheet.
    TechPack,
    /// The outliner.
    Outliner,
    /// The properties editor.
    Properties,
}

impl Editor {
    /// Every editor, in the editor-type menu's order.
    pub const ALL: [Self; 5] = [
        Self::View3d,
        Self::Pattern,
        Self::TechPack,
        Self::Outliner,
        Self::Properties,
    ];

    /// Name in the editor-type menu.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::View3d => "3D Viewport",
            Self::Pattern => "Pattern Editor",
            Self::TechPack => "Tech Pack",
            Self::Outliner => "Outliner",
            Self::Properties => "Properties",
        }
    }

    /// Blender icon.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::View3d => "VIEW3D",
            Self::Pattern => "UV",
            Self::TechPack => "SPREADSHEET",
            Self::Outliner => "OUTLINER",
            Self::Properties => "PROPERTIES",
        }
    }
}

/// A place in the window that shows an editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// The large area on the left.
    Main,
    /// The second large area (Design and Colorway workspaces).
    Second,
    /// The top of the side column.
    SideTop,
    /// The bottom of the side column.
    SideBottom,
}

impl Slot {
    /// Every slot, in the order the editor-type menus are listed.
    pub const ALL: [Self; 4] = [Self::Main, Self::Second, Self::SideTop, Self::SideBottom];
}

/// The editor `slot` shows in `workspace` unless the user switched it; `None` where the workspace
/// has no such area.
#[must_use]
pub const fn default_editor(workspace: Workspace, slot: Slot) -> Option<Editor> {
    Some(match (workspace, slot) {
        (_, Slot::SideTop) => Editor::Outliner,
        (_, Slot::SideBottom) => Editor::Properties,
        (Workspace::Grading, Slot::Main) => Editor::Pattern,
        (Workspace::TechPack, Slot::Main) => Editor::TechPack,
        (_, Slot::Main) => Editor::View3d,
        (Workspace::Design | Workspace::Colorway, Slot::Second) => Editor::Pattern,
        (_, Slot::Second) => return None,
    })
}

impl Slot {
    /// The slot that splits off from this one, or joins into it: the second large area for the
    /// large ones, the other half of the side column for the side ones.
    #[must_use]
    pub const fn partner(self) -> Self {
        match self {
            Self::Main => Self::Second,
            Self::Second => Self::Main,
            Self::SideTop => Self::SideBottom,
            Self::SideBottom => Self::SideTop,
        }
    }
}

/// The editor-type menus, one per slot in [`Slot::ALL`] order: the editors, then Split Area and
/// Join Areas (Blender's View ▸ Area).
#[must_use]
pub fn editor_menus() -> Vec<Menu> {
    Slot::ALL
        .iter()
        .map(|&slot| {
            let mut entries: Vec<MenuEntry> = Editor::ALL
                .iter()
                .map(|&e| MenuEntry::Item(Operator::SetEditor(slot, e)))
                .collect();
            entries.extend([
                MenuEntry::Separator,
                MenuEntry::Item(Operator::SplitArea(slot)),
                MenuEntry::Item(Operator::JoinArea(slot)),
            ]);
            Menu {
                title: "Editor Type",
                entries,
            }
        })
        .collect()
}

/// Editors the user switched to, by workspace and slot.
pub type EditorChoices = BTreeMap<(Workspace, Slot), Editor>;
/// Areas the user split off or joined away, by workspace and slot: `true` open, `false` closed.
pub type AreaChoices = BTreeMap<(Workspace, Slot), bool>;

impl AzamApp {
    /// The editor `slot` shows in the current workspace; `None` when there is no such area.
    #[must_use]
    pub fn editor_at(&self, slot: Slot) -> Option<Editor> {
        let ws = self.workspace();
        let open = self.area_choices.get(&(ws, slot)).copied();
        if open == Some(false) {
            return None;
        }
        let chosen = self.editor_choices.get(&(ws, slot)).copied();
        match default_editor(ws, slot) {
            Some(fallback) => Some(chosen.unwrap_or(fallback)),
            // an area split off where the workspace has none: the chosen editor, else the first
            // large editor no other area shows
            None if open == Some(true) => chosen.or_else(|| {
                [Editor::Pattern, Editor::View3d, Editor::TechPack]
                    .into_iter()
                    .find(|&e| {
                        Slot::ALL
                            .iter()
                            .all(|&s| s == slot || self.editor_at_without(s, slot) != Some(e))
                    })
            }),
            None => None,
        }
    }

    /// The editor `slot` shows by its own settings, never asking about `skip` (so a split-off
    /// area can pick an editor without asking itself).
    fn editor_at_without(&self, slot: Slot, skip: Slot) -> Option<Editor> {
        let ws = self.workspace();
        if slot == skip || self.area_choices.get(&(ws, slot)) == Some(&false) {
            return None;
        }
        self.editor_choices
            .get(&(ws, slot))
            .copied()
            .or_else(|| default_editor(ws, slot))
    }

    /// Splits the area in `slot`: its partner area opens beside it (the large areas side by side,
    /// the side column in two). Returns `false` if the partner is open already or has nothing
    /// left to show.
    pub fn split_area(&mut self, slot: Slot) -> bool {
        let (ws, partner) = (self.workspace(), slot.partner());
        if self.editor_at(partner).is_some() {
            return false;
        }
        self.area_choices.insert((ws, partner), true);
        // the new area shows an editor no other area shows (its default may be on screen
        // already, after a join)
        let shown: Vec<Editor> = Slot::ALL
            .into_iter()
            .filter(|&s| s != partner)
            .filter_map(|s| self.editor_at(s))
            .collect();
        let fresh = if matches!(partner, Slot::Main | Slot::Second) {
            [Editor::Pattern, Editor::View3d, Editor::TechPack]
                .into_iter()
                .find(|e| !shown.contains(e))
        } else {
            [Editor::Outliner, Editor::Properties]
                .into_iter()
                .find(|e| !shown.contains(e))
        };
        match fresh {
            Some(editor) => {
                self.editor_choices.insert((ws, partner), editor);
            }
            None => {
                self.area_choices.remove(&(ws, partner));
                return false;
            }
        }
        self.framed = false;
        true
    }

    /// Joins the area in `slot` with its partner: `slot`'s editor takes the whole room. Returns
    /// `false` if there is no partner area to join.
    pub fn join_area(&mut self, slot: Slot) -> bool {
        let (ws, partner) = (self.workspace(), slot.partner());
        let (Some(kept), Some(_)) = (self.editor_at(slot), self.editor_at(partner)) else {
            return false;
        };
        // the main and bottom slots stay; joining into them keeps the editor that was clicked
        if matches!(partner, Slot::Main | Slot::SideBottom) {
            self.editor_choices.insert((ws, partner), kept);
            self.area_choices.insert((ws, slot), false);
        } else {
            self.area_choices.insert((ws, partner), false);
        }
        self.framed = false;
        true
    }

    /// Shows `editor` in `slot`; an area already showing it takes `slot`'s editor instead.
    pub fn set_editor(&mut self, slot: Slot, editor: Editor) {
        let ws = self.workspace();
        let Some(current) = self.editor_at(slot) else {
            return;
        };
        if let Some(other) = Slot::ALL
            .into_iter()
            .find(|&s| s != slot && self.editor_at(s) == Some(editor))
        {
            self.editor_choices.insert((ws, other), current);
        }
        self.editor_choices.insert((ws, slot), editor);
        self.framed = false;
    }

    /// The editor-type button of the area in `slot`: its editor's icon, opening the menu of
    /// editors.
    pub(crate) fn editor_type_button(&mut self, slot: Slot, cx: &mut Context<Self>) -> Div {
        let icon = self.editor_at(slot).map_or("QUESTION", Editor::icon);
        let index = Slot::ALL.iter().position(|&s| s == slot).unwrap_or(0);
        div()
            .class("editor-type")
            .id(format!("editor-type-{index}"))
            .child(self.icons.icon(icon))
            .child(self.icons.sized("TRIA_DOWN", 10.0, None))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.open_menu = Some(this.editor_menu_start + index);
                this.open_submenu = None;
                this.menu_anchor = (e.up.position.x.0 - 8.0, e.up.position.y.0 + 12.0);
                cx.notify();
            }))
    }

    /// The area showing `editor` in `slot`.
    pub(crate) fn editor_area(&mut self, slot: Slot, editor: Editor, cx: &mut Context<Self>) -> Div {
        match editor {
            Editor::View3d => self.viewport_area(slot, cx),
            Editor::Pattern => self.pattern_area(slot, cx),
            Editor::TechPack => self.techpack_area(slot, cx),
            Editor::Outliner => self.outliner_area(slot, cx),
            Editor::Properties => self.properties_area(slot, cx),
        }
    }
}
