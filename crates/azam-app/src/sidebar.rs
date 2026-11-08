//! The 3D view's sidebar (Blender's N panel): the selected piece's placement as numbers, and the
//! view's overlays.

use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::Operator;
use crate::properties::field;

/// Width the sidebar takes from the 3D view, logical pixels (`.sidebar` in the theme plus its gap).
const SIDEBAR_PX: f32 = 238.0;

impl AzamApp {
    /// Shows or hides the 3D view's sidebar.
    pub fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
        // the navigation gizmo moves left of the sidebar, as in Blender
        self.viewport.right_inset = if self.show_sidebar { SIDEBAR_PX } else { 0.0 };
    }

    /// Shows or hides tool names in the tool shelves.
    pub fn toggle_tool_labels(&mut self) {
        self.tool_labels = !self.tool_labels;
        self.framed = false;
    }

    /// Whether the 3D view's sidebar is shown.
    #[must_use]
    pub fn sidebar_shown(&self) -> bool {
        self.show_sidebar
    }

    /// The sidebar, while it is shown.
    pub(crate) fn sidebar(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.show_sidebar {
            return None;
        }
        let item = match self.session.selection.active.clone() {
            Some(id) if self.session.project.pieces.contains_key(&id) => {
                let pc = &self.session.project.pieces[&id];
                let (name, moved, turned) = (
                    pc.name.clone(),
                    pc.arr.moved.unwrap_or([0.0; 3]),
                    pc.arr.turned.unwrap_or(0.0),
                );
                let slot = azam_avatar::slot_by_id(&pc.arr.slot).label;
                let axis = |label, k: usize, cx: &mut Context<Self>| {
                    let id = id.clone();
                    Self::number(field(label, -2000.0, 2000.0, 5.0), moved[k], cx, move |p, v| {
                        if let Some(pc) = p.pieces.get_mut(&id) {
                            let mut m = pc.arr.moved.unwrap_or([0.0; 3]);
                            m[k] = v;
                            pc.arr.moved = (m != [0.0; 3]).then_some(m);
                        }
                    })
                };
                let turn = {
                    let id = id.clone();
                    Self::number(
                        field("Turn (°)", -360.0, 360.0, 5.0),
                        turned.to_degrees(),
                        cx,
                        move |p, v| {
                            if let Some(pc) = p.pieces.get_mut(&id) {
                                pc.arr.turned = (v != 0.0).then_some(v.to_radians());
                            }
                        },
                    )
                };
                vec![
                    Self::info_row("Piece", name),
                    Self::info_row("Slot", slot.to_owned()),
                    axis("Move X (mm)", 0, cx),
                    axis("Move Y (mm)", 1, cx),
                    axis("Move Z (mm)", 2, cx),
                    turn,
                ]
            }
            _ => vec![
                div()
                    .class("hint")
                    .child("Select a piece to place it by numbers."),
            ],
        };
        let toggle = |label: &'static str, on: bool, op: Operator, cx: &mut Context<Self>| {
            div()
                .class(if on { "pbtn primary" } else { "pbtn" })
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.run(op, window, cx)))
        };
        let (avatar, grid, points) = (
            self.session.project.avatar.show,
            self.viewport.overlays.grid,
            self.show_arrangement,
        );
        let view = vec![
            div()
                .class("pbuttons")
                .child(toggle("Avatar", avatar, Operator::ToggleAvatar, cx))
                .child(toggle("Grid", grid, Operator::ToggleGrid, cx))
                .child(toggle("Points", points, Operator::ToggleArrangementPoints, cx)),
        ];
        Some(
            div()
                .class("sidebar")
                .child(self.panel("Item", item))
                .child(self.panel("View", view)),
        )
    }
}
