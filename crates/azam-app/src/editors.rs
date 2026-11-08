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

/// The editor-type menus, one per slot in [`Slot::ALL`] order.
#[must_use]
pub fn editor_menus() -> Vec<Menu> {
    Slot::ALL
        .iter()
        .map(|&slot| Menu {
            title: "Editor Type",
            entries: Editor::ALL
                .iter()
                .map(|&e| MenuEntry::Item(Operator::SetEditor(slot, e)))
                .collect(),
        })
        .collect()
}

/// Editors the user switched to, by workspace and slot.
pub type EditorChoices = BTreeMap<(Workspace, Slot), Editor>;

impl AzamApp {
    /// The editor `slot` shows in the current workspace.
    #[must_use]
    pub fn editor_at(&self, slot: Slot) -> Option<Editor> {
        let fallback = default_editor(self.workspace(), slot)?;
        Some(
            self.editor_choices
                .get(&(self.workspace(), slot))
                .copied()
                .unwrap_or(fallback),
        )
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
