//! Right-click menus, as in Blender: the commands for what lies under the pointer, opened where
//! it is.

use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::ContextMenu;

impl AzamApp {
    /// Opens right-click menu `which` at `at` (window pixels), as a right-click there does.
    pub fn open_context_menu(&mut self, which: ContextMenu, at: Point<Pixels>) {
        let index = ContextMenu::ALL.iter().position(|&m| m == which).unwrap_or(0);
        self.open_menu = Some(self.context_menu_start + index);
        self.open_submenu = None;
        self.menu_anchor = (at.x.0, at.y.0);
    }

    /// The title of the open menu when it is a right-click menu.
    #[must_use]
    pub fn open_context_menu_title(&self) -> Option<&'static str> {
        self.open_menu
            .filter(|&i| (self.context_menu_start..self.editor_menu_start).contains(&i))
            .map(|i| self.menus[i].title)
    }

    /// Turns the selected outline points of the active piece from corners into curve points, or
    /// back (Blender's toggle; the outline's first point stays a corner).
    pub(crate) fn toggle_curve_points(&mut self) {
        let Some(piece) = self.session.selection.active.clone() else {
            self.status = "Select points first (Edit Pattern tool)".into();
            return;
        };
        let points = self.session.selection.points.clone();
        if points.is_empty() {
            self.status = "Select points first (Edit Pattern tool)".into();
            return;
        }
        let mut changed = 0;
        self.session.edit("Toggle Corner / Curve", |p| {
            for &i in &points {
                let curve = p
                    .pieces
                    .get(&piece)
                    .and_then(|pc| pc.pts.get(i))
                    .is_some_and(|q| q.curve);
                if azam_doc::set_point_curve(p, &piece, i, !curve) {
                    changed += 1;
                }
            }
            azam_doc::sync_mirrors(p);
        });
        self.status = if changed > 0 {
            format!("Toggled {changed} point(s)")
        } else {
            "Those points cannot change (a piece keeps three corners)".into()
        };
    }
}
