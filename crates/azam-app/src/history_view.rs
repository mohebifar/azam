//! Two of Blender's popups: Edit ▸ Undo History (every step of the history, the current one
//! marked; a click goes to any of them) and Quick Favorites (Q: the commands the user starred in
//! Preferences ▸ Keymap, in a menu at the pointer).

use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::operators::Operator;

/// Which of the two popups is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPopup {
    /// Edit ▸ Undo History.
    UndoHistory,
    /// The Quick Favorites menu, at the pointer.
    Favorites,
}

impl AzamApp {
    /// Stars `label` as a Quick Favorite, or unstars it.
    pub fn toggle_favorite(&mut self, label: &str) {
        if let Some(i) = self.prefs.favorites.iter().position(|l| l == label) {
            self.prefs.favorites.remove(i);
            self.status = format!("{label} removed from Quick Favorites");
        } else {
            self.prefs.favorites.push(label.to_owned());
            self.status = format!("{label} added to Quick Favorites (Q)");
        }
        self.save_prefs();
    }

    /// The Quick Favorites that name a command (labels from older versions that match none are
    /// left out).
    #[must_use]
    pub fn favorites(&self) -> Vec<Operator> {
        let all = Operator::searchable();
        self.prefs
            .favorites
            .iter()
            .filter_map(|l| all.iter().copied().find(|op| op.label() == *l))
            .collect()
    }

    /// Goes to step `step` of the undo history (see [`crate::session::Session::jump_to`]).
    pub fn jump_to_history(&mut self, step: usize) {
        let moved = self.session.jump_to(step);
        let (done, _) = self.session.history();
        self.status = if moved == 0 {
            "Already there".into()
        } else {
            match done.last() {
                Some(label) => format!("Back to: {label}"),
                None => "Back to the original".into(),
            }
        };
    }

    /// The open popup.
    pub(crate) fn history_popup(&mut self, which: HistoryPopup, cx: &mut Context<Self>) -> Div {
        match which {
            HistoryPopup::UndoHistory => self.undo_history_popup(cx),
            HistoryPopup::Favorites => self.favorites_popup(cx),
        }
    }

    fn undo_history_popup(&mut self, cx: &mut Context<Self>) -> Div {
        let (done, undone) = self.session.history();
        let now = done.len();
        let steps = std::iter::once("Original".to_owned())
            .chain(done)
            .chain(undone)
            .enumerate();
        let mut list = div().class("search-results").id("undo-history");
        for (i, label) in steps {
            let class = match i.cmp(&now) {
                std::cmp::Ordering::Equal => "menu-item hist-now",
                std::cmp::Ordering::Greater => "menu-item hist-undone",
                std::cmp::Ordering::Less => "menu-item",
            };
            list = list.child(
                div()
                    .class(class)
                    .id(format!("hist-{i}"))
                    .child(div().class("menu-icon").child(if i == now { "✓" } else { "" }))
                    .child(div().class("menu-label").child(label))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.jump_to_history(i);
                        this.history_open = None;
                        window.focus_id("root");
                        cx.notify();
                    })),
            );
        }
        div()
            .class("search")
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(
                div()
                    .class("prop-title")
                    .child(self.icons.icon("RECOVER_LAST"))
                    .child("Undo History"),
            )
            .child(list)
    }

    fn favorites_popup(&mut self, cx: &mut Context<Self>) -> Div {
        let favorites = self.favorites();
        let (left, top) = self.pointer;
        let mut menu = div()
            .class("menu quick-favorites")
            .id("quick-favorites")
            .css("left", format!("{left}px"))
            .css("top", format!("{top}px"))
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(div().class("menu-heading").child("Quick Favorites"));
        if favorites.is_empty() {
            menu = menu.child(
                div()
                    .class("pnote")
                    .child("Star commands with ☆ in Preferences ▸ Keymap to list them here."),
            );
        }
        for op in favorites {
            menu = menu.child(self.favorite_item(op, cx));
        }
        menu
    }

    fn favorite_item(&mut self, op: Operator, cx: &mut Context<Self>) -> Div {
        let icon = match op.icon() {
            Some(name) => self.icons.icon(name),
            None => div().class("menu-icon"),
        };
        div()
            .class("menu-item")
            .id(format!("fav-run-{}", op.label()))
            .child(icon)
            .child(div().class("menu-label").child(op.label()))
            .when_some(self.shortcut_label(op), |el, s| {
                el.child(div().class("menu-shortcut").child(s))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.history_open = None;
                this.run(op, window, cx);
                cx.notify();
            }))
    }
}
