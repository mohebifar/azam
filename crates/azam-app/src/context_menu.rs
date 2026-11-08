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
}
