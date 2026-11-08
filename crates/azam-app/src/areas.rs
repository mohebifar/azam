//! Areas, as in Blender: each workspace's window is a tree of splits whose leaves are editors.
//! Any area can be split side by side or one over the other, an area joins its neighbour, and the
//! border of every split can be dragged.

use std::collections::BTreeMap;

use blenderui::prelude::*;

use crate::app::{AzamApp, Workspace};
use crate::editors::Editor;

/// Identifies an area (a leaf) or a split in a layout.
pub type AreaId = u32;

/// How a split lays out its two parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Dir {
    /// Side by side (the border runs up and down).
    Row,
    /// One over the other (the border runs across).
    Column,
}

/// A layout: an area showing an editor, or a split into two parts.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AreaNode {
    /// An area.
    Leaf {
        /// Its id.
        id: AreaId,
        /// The editor it shows.
        editor: Editor,
    },
    /// Two parts sharing the room.
    Split {
        /// Its id.
        id: AreaId,
        /// How the parts are laid out.
        dir: Dir,
        /// Share of the first part, 0..1.
        ratio: f32,
        /// The first part (left or top).
        a: Box<AreaNode>,
        /// The second part (right or bottom).
        b: Box<AreaNode>,
    },
}

/// Smallest share either part of a split keeps when its border is dragged.
const MIN_RATIO: f32 = 0.08;
/// Share of the window the large areas take beside the outliner and properties column.
const MAIN_RATIO: f32 = 0.775;
/// Share of the side column the outliner takes.
const OUTLINER_RATIO: f32 = 0.3;

impl AreaNode {
    /// Its id.
    #[must_use]
    pub fn id(&self) -> AreaId {
        match self {
            Self::Leaf { id, .. } | Self::Split { id, .. } => *id,
        }
    }

    /// Every area with its editor, left to right and top to bottom.
    #[must_use]
    pub fn leaves(&self) -> Vec<(AreaId, Editor)> {
        match self {
            Self::Leaf { id, editor } => vec![(*id, *editor)],
            Self::Split { a, b, .. } => {
                let mut out = a.leaves();
                out.extend(b.leaves());
                out
            }
        }
    }

    /// Every split's id.
    #[must_use]
    pub fn splits(&self) -> Vec<AreaId> {
        match self {
            Self::Leaf { .. } => Vec::new(),
            Self::Split { id, a, b, .. } => {
                let mut out = vec![*id];
                out.extend(a.splits());
                out.extend(b.splits());
                out
            }
        }
    }

    /// The split `id`: its direction and ratio.
    #[must_use]
    pub fn split_info(&self, id: AreaId) -> Option<(Dir, f32)> {
        match self {
            Self::Leaf { .. } => None,
            Self::Split {
                id: sid,
                dir,
                ratio,
                a,
                b,
            } => {
                if *sid == id {
                    Some((*dir, *ratio))
                } else {
                    a.split_info(id).or_else(|| b.split_info(id))
                }
            }
        }
    }

    /// The editor area `id` shows.
    #[must_use]
    pub fn editor_of(&self, id: AreaId) -> Option<Editor> {
        self.leaves().into_iter().find(|&(i, _)| i == id).map(|(_, e)| e)
    }

    /// Shows `editor` in area `id`; `false` if there is no such area.
    pub fn set_editor(&mut self, id: AreaId, editor: Editor) -> bool {
        match self {
            Self::Leaf { id: lid, editor: e } if *lid == id => {
                *e = editor;
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { a, b, .. } => a.set_editor(id, editor) || b.set_editor(id, editor),
        }
    }

    /// Splits area `id` in two along `dir`: it keeps its editor in the first part and `other`
    /// shows in the second. New ids come from `next`. `false` if there is no such area.
    pub fn split(&mut self, id: AreaId, dir: Dir, other: Editor, next: &mut AreaId) -> bool {
        match self {
            Self::Leaf { id: lid, editor } if *lid == id => {
                let editor = *editor;
                let mut fresh = || {
                    *next += 1;
                    *next
                };
                let (split_id, other_id) = (fresh(), fresh());
                *self = Self::Split {
                    id: split_id,
                    dir,
                    ratio: 0.5,
                    a: Box::new(Self::Leaf { id, editor }),
                    b: Box::new(Self::Leaf {
                        id: other_id,
                        editor: other,
                    }),
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { a, b, .. } => a.split(id, dir, other, next) || b.split(id, dir, other, next),
        }
    }

    /// Joins area `id` with its neighbour: the split holding it becomes it, taking all the room.
    /// `false` if there is no such area or it fills the window alone.
    pub fn join(&mut self, id: AreaId) -> bool {
        let Self::Split { a, b, .. } = self else {
            return false;
        };
        let keep = if matches!(**a, Self::Leaf { id: lid, .. } if lid == id) {
            Some((**a).clone())
        } else if matches!(**b, Self::Leaf { id: lid, .. } if lid == id) {
            Some((**b).clone())
        } else {
            None
        };
        match keep {
            Some(leaf) => {
                *self = leaf;
                true
            }
            None => a.join(id) || b.join(id),
        }
    }

    /// Sets split `id`'s ratio (kept between [`MIN_RATIO`] and its complement).
    pub fn set_ratio(&mut self, id: AreaId, value: f32) -> bool {
        match self {
            Self::Leaf { .. } => false,
            Self::Split {
                id: sid, ratio, a, b, ..
            } => {
                if *sid == id {
                    *ratio = value.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
                    true
                } else {
                    a.set_ratio(id, value) || b.set_ratio(id, value)
                }
            }
        }
    }
}

/// The layout `workspace` starts with: its large areas beside the outliner over the properties.
/// Ids come from `next`.
#[must_use]
pub fn default_layout(workspace: Workspace, next: &mut AreaId) -> AreaNode {
    let mut fresh = || {
        *next += 1;
        *next
    };
    let leaf = |editor, id| AreaNode::Leaf { id, editor };
    let main = match workspace {
        Workspace::Design => AreaNode::Split {
            id: fresh(),
            dir: Dir::Row,
            ratio: 0.5,
            a: Box::new(leaf(Editor::View3d, fresh())),
            b: Box::new(leaf(Editor::Pattern, fresh())),
        },
        // the garment beside every colorway of it
        Workspace::Colorway => AreaNode::Split {
            id: fresh(),
            dir: Dir::Row,
            ratio: 0.45,
            a: Box::new(leaf(Editor::View3d, fresh())),
            b: Box::new(leaf(Editor::Colorways, fresh())),
        },
        // the drape above the timeline that plays its recording back
        Workspace::Review => AreaNode::Split {
            id: fresh(),
            dir: Dir::Column,
            ratio: 0.84,
            a: Box::new(leaf(Editor::View3d, fresh())),
            b: Box::new(leaf(Editor::Timeline, fresh())),
        },
        Workspace::Grading => leaf(Editor::Pattern, fresh()),
        // the tables above the marker they cut from
        Workspace::TechPack => AreaNode::Split {
            id: fresh(),
            dir: Dir::Column,
            ratio: 0.5,
            a: Box::new(leaf(Editor::TechPack, fresh())),
            b: Box::new(leaf(Editor::Marker, fresh())),
        },
    };
    let side = AreaNode::Split {
        id: fresh(),
        dir: Dir::Column,
        ratio: OUTLINER_RATIO,
        a: Box::new(leaf(Editor::Outliner, fresh())),
        b: Box::new(leaf(Editor::Properties, fresh())),
    };
    AreaNode::Split {
        id: fresh(),
        dir: Dir::Row,
        ratio: MAIN_RATIO,
        a: Box::new(main),
        b: Box::new(side),
    }
}

/// Layouts by workspace.
pub type Layouts = BTreeMap<String, AreaNode>;

/// Id of split `id`'s draggable border element.
fn border_id(id: AreaId) -> String {
    format!("border-{id}")
}

/// Id of area `id`'s element.
fn area_box_id(id: AreaId) -> String {
    format!("area-{id}")
}

/// Id of split `id`'s element.
fn split_box_id(id: AreaId) -> String {
    format!("splitbox-{id}")
}

/// The largest id used in `node`.
fn max_id(node: &AreaNode) -> AreaId {
    match node {
        AreaNode::Leaf { id, .. } => *id,
        AreaNode::Split { id, a, b, .. } => (*id).max(max_id(a)).max(max_id(b)),
    }
}

impl AzamApp {
    /// Saves every workspace's layout in the preferences (when they have a file), so the window
    /// opens as it was left.
    pub(crate) fn layouts_changed(&mut self) {
        self.prefs.layouts = self
            .layouts
            .iter()
            .filter_map(|(name, node)| Some((name.clone(), serde_json::to_value(node).ok()?)))
            .collect();
        self.save_prefs();
    }

    /// Takes the layouts saved in the preferences (those that read back).
    pub(crate) fn load_layouts(&mut self) {
        for (name, value) in &self.prefs.layouts {
            if let Ok(node) = serde_json::from_value::<AreaNode>(value.clone())
                && !node.leaves().is_empty()
            {
                self.next_area_id = self.next_area_id.max(max_id(&node));
                self.layouts.insert(name.clone(), node);
            }
        }
        self.framed = false;
    }

    /// Puts the current workspace's areas back as they start.
    pub fn reset_layout(&mut self) {
        let ws = self.workspace();
        let mut next = self.next_area_id;
        self.layouts
            .insert(self.active_tab_name(), default_layout(ws, &mut next));
        self.next_area_id = next;
        self.maximized = None;
        self.framed = false;
        self.layouts_changed();
        self.status = "Layout reset".into();
    }

    /// The current workspace's layout (made from its defaults the first time).
    pub(crate) fn layout_mut(&mut self) -> &mut AreaNode {
        let ws = self.workspace();
        let name = self.active_tab_name();
        let next = &mut self.next_area_id;
        self.layouts
            .entry(name)
            .or_insert_with(|| default_layout(ws, next))
    }

    /// The current workspace's layout (its defaults until it is first shown; the ids they get
    /// then are the ones shown here, as nothing else hands out ids).
    #[must_use]
    pub fn layout(&self) -> AreaNode {
        self.layouts
            .get(&self.active_tab_name())
            .cloned()
            .unwrap_or_else(|| {
                let mut next = self.next_area_id;
                default_layout(self.workspace(), &mut next)
            })
    }

    /// The areas of `node`, its splits laid out with draggable borders.
    pub(crate) fn area_tree(&mut self, node: &AreaNode, cx: &mut Context<Self>) -> Div {
        match node {
            AreaNode::Leaf { id, editor } => self
                .editor_area(*id, *editor, cx)
                .id(area_box_id(*id))
                // kept bounds let a narrow area's header leave out what does not fit
                .on_hover(cx.listener(|_, _: &bool, _, _| {})),
            AreaNode::Split { id, dir, ratio, a, b } => {
                let first = self.area_tree(a, cx).css("flex", format!("{ratio} 1 0px"));
                let second = self
                    .area_tree(b, cx)
                    .css("flex", format!("{} 1 0px", 1.0 - ratio));
                let (id, dir) = (*id, *dir);
                div()
                    .class(if dir == Dir::Row {
                        "area-split-row"
                    } else {
                        "area-split-col"
                    })
                    .id(split_box_id(id))
                    // a listener makes the kit keep this element's bounds for border drags
                    .on_hover(cx.listener(|_, _: &bool, _, _| {}))
                    .child(first)
                    .child(
                        div()
                            .class(if dir == Dir::Row {
                                "area-split v"
                            } else {
                                "area-split h"
                            })
                            .id(border_id(id))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                    this.resizing = Some(id);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(second)
            }
        }
    }

    /// Remembers where the current layout's areas and splits were laid out (window bounds can
    /// only be read while a frame is prepared, not in event handlers).
    pub(crate) fn note_areas(&mut self, window: &Window) {
        let layout = self.layout();
        let splits = layout.splits().into_iter().map(|id| (id, split_box_id(id)));
        let leaves = layout.leaves().into_iter().map(|(id, _)| (id, area_box_id(id)));
        self.area_bounds = splits
            .chain(leaves)
            .filter_map(|(id, el)| window.bounds_of(&el).map(|b| (id, b)))
            .collect();
    }

    /// Width area `id` was laid out at last frame (unbounded before it was).
    #[must_use]
    pub fn area_width(&self, id: AreaId) -> f32 {
        self.area_bounds.get(&id).map_or(f32::INFINITY, |b| b.size.width)
    }

    /// Follows the pointer with the border being dragged; returns `true` if one is.
    pub(crate) fn drag_border(&mut self, at: Point<Pixels>) -> bool {
        let Some(id) = self.resizing else {
            return false;
        };
        let (Some(b), Some((dir, _))) = (self.area_bounds.get(&id).copied(), self.layout().split_info(id))
        else {
            return true;
        };
        let ratio = match dir {
            Dir::Row => (at.x.0 - b.origin.x) / b.size.width.max(1.0),
            Dir::Column => (at.y.0 - b.origin.y) / b.size.height.max(1.0),
        };
        self.layout_mut().set_ratio(id, ratio);
        self.framed = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_split_join_and_resize() {
        let mut next = 0;
        let mut layout = default_layout(Workspace::Review, &mut next);
        let editors = |l: &AreaNode| l.leaves().into_iter().map(|(_, e)| e).collect::<Vec<_>>();
        assert_eq!(
            editors(&layout),
            vec![
                Editor::View3d,
                Editor::Timeline,
                Editor::Outliner,
                Editor::Properties
            ]
        );
        // the 3D view takes the timeline's room
        let (view, _) = layout.leaves()[0];
        assert!(layout.join(view));
        assert_eq!(
            editors(&layout),
            vec![Editor::View3d, Editor::Outliner, Editor::Properties]
        );
        let (view, _) = layout.leaves()[0];
        assert!(layout.split(view, Dir::Row, Editor::Pattern, &mut next));
        assert_eq!(
            editors(&layout),
            vec![
                Editor::View3d,
                Editor::Pattern,
                Editor::Outliner,
                Editor::Properties
            ]
        );
        let (pattern, _) = layout.leaves()[1];
        assert!(
            layout.split(pattern, Dir::Column, Editor::Outliner, &mut next),
            "an area splits again"
        );
        assert_eq!(layout.leaves().len(), 5);
        assert!(layout.join(pattern), "the pattern editor takes its split back");
        assert_eq!(layout.leaves().len(), 4);
        assert!(layout.join(view));
        assert_eq!(
            editors(&layout),
            vec![Editor::View3d, Editor::Outliner, Editor::Properties]
        );
        let root = layout.id();
        assert!(layout.set_ratio(root, 2.0));
        assert_eq!(layout.split_info(root), Some((Dir::Row, 1.0 - MIN_RATIO)));
        let lone = AreaNode::Leaf {
            id: 99,
            editor: Editor::View3d,
        };
        assert!(!lone.clone().join(99), "a lone area has nothing to join");
    }
}
