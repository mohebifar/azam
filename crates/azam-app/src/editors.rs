//! Editor types, as in Blender: every area shows one editor, and the editor-type button in its
//! header switches it. Choosing the 3D view or pattern editor where another area shows it swaps
//! the two.

use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::areas::{AreaId, Dir};
use crate::operators::{Menu, MenuEntry, Operator};

/// An editor an area can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
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
    /// Every colorway rendered side by side.
    Colorways,
}

impl Editor {
    /// Every editor, in the editor-type menu's order.
    pub const ALL: [Self; 6] = [
        Self::View3d,
        Self::Pattern,
        Self::Colorways,
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
            Self::Colorways => "Colorways",
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
            Self::Colorways => "COLOR",
        }
    }
}

impl Editor {
    /// Whether two areas may show this editor at once (Blender lets any editor show twice; the
    /// 3D view and pattern editor here have one camera each, so they stay unique).
    #[must_use]
    pub const fn duplicable(self) -> bool {
        matches!(self, Self::TechPack | Self::Outliner | Self::Properties)
    }
}

/// The editor-type menu of area `id`: the editors, then Blender's View ▸ Area commands.
#[must_use]
pub fn editor_menu(id: AreaId) -> Menu {
    let mut entries: Vec<MenuEntry> = Editor::ALL
        .iter()
        .map(|&e| MenuEntry::Item(Operator::SetEditor(id, e)))
        .collect();
    entries.extend([
        MenuEntry::Separator,
        MenuEntry::Item(Operator::SplitArea(id, Dir::Row)),
        MenuEntry::Item(Operator::SplitArea(id, Dir::Column)),
        MenuEntry::Item(Operator::JoinArea(id)),
        MenuEntry::Separator,
        MenuEntry::Item(Operator::MaximizeArea(id)),
    ]);
    Menu {
        title: "Editor Type",
        entries,
    }
}

impl AzamApp {
    /// The area showing `editor` in the current workspace, if one does.
    #[must_use]
    pub fn area_with(&self, editor: Editor) -> Option<AreaId> {
        self.layout()
            .leaves()
            .into_iter()
            .find(|&(_, e)| e == editor)
            .map(|(id, _)| id)
    }

    /// The editor area `id` shows in the current workspace.
    #[must_use]
    pub fn editor_of(&self, id: AreaId) -> Option<Editor> {
        self.layout().editor_of(id)
    }

    /// The editors the current workspace's areas show, left to right and top to bottom.
    #[must_use]
    pub fn editors_shown(&self) -> Vec<Editor> {
        self.layout().leaves().into_iter().map(|(_, e)| e).collect()
    }

    /// Splits area `id` along `dir`. The new area shows the same editor when it can show twice,
    /// else a large editor no area shows, else the properties. Returns the new area's id.
    pub fn split_area(&mut self, id: AreaId, dir: Dir) -> Option<AreaId> {
        let current = self.editor_of(id)?;
        let shown = self.editors_shown();
        let other = if current.duplicable() {
            current
        } else {
            [Editor::Pattern, Editor::View3d]
                .into_iter()
                .find(|e| !shown.contains(e))
                .unwrap_or(Editor::Properties)
        };
        let mut next = self.next_area_id;
        let done = self.layout_mut().split(id, dir, other, &mut next);
        self.next_area_id = next;
        self.framed = false;
        self.layouts_changed();
        done.then_some(next)
    }

    /// Joins area `id` with its neighbour, `id`'s editor taking the room. Returns `false` if the
    /// area fills the window alone.
    pub fn join_area(&mut self, id: AreaId) -> bool {
        let done = self.layout_mut().join(id);
        self.framed = false;
        self.layouts_changed();
        done
    }

    /// Shows `editor` in area `id`; when another area shows it and it cannot show twice, that
    /// area takes `id`'s editor instead.
    pub fn set_editor(&mut self, id: AreaId, editor: Editor) {
        let Some(current) = self.editor_of(id) else {
            return;
        };
        if !editor.duplicable()
            && let Some(other) = self.area_with(editor).filter(|&o| o != id)
        {
            self.layout_mut().set_editor(other, current);
        }
        self.layout_mut().set_editor(id, editor);
        self.framed = false;
        self.layouts_changed();
    }

    /// The editor-type button of area `id`: its editor's icon, opening the menu of editors.
    pub(crate) fn editor_type_button(&mut self, id: AreaId, cx: &mut Context<Self>) -> Div {
        let icon = self.editor_of(id).map_or("QUESTION", Editor::icon);
        div()
            .class("editor-type")
            .id(format!("editor-type-{id}"))
            .child(self.icons.icon(icon))
            .child(self.icons.sized("TRIA_DOWN", 10.0, None))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.menus[this.editor_menu_start] = editor_menu(id);
                this.open_menu = Some(this.editor_menu_start);
                this.open_submenu = None;
                this.menu_anchor = (e.up.position.x.0 - 8.0, e.up.position.y.0 + 12.0);
                cx.notify();
            }))
    }

    /// Area `id`, showing `editor`.
    pub(crate) fn editor_area(&mut self, id: AreaId, editor: Editor, cx: &mut Context<Self>) -> Div {
        match editor {
            Editor::View3d => self.viewport_area(id, cx),
            Editor::Pattern => self.pattern_area(id, cx),
            Editor::TechPack => self.techpack_area(id, cx),
            Editor::Colorways => self.colorways_area(id, cx),
            Editor::Outliner => self.outliner_area(id, cx),
            Editor::Properties => self.properties_area(id, cx),
        }
    }
}
