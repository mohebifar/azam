//! Resizable areas, as in Blender: dragging the border between two areas shares the space
//! differently (3D view | pattern editor | outliner and properties, outliner over properties).

use blenderui::prelude::*;

use crate::app::AzamApp;

/// Padding around the areas, in logical pixels (`.areas` in the theme).
const AREAS_PAD: f32 = 2.0;
/// Thickness of a border, in logical pixels (`.area-split` in the theme).
const BORDER_PX: f32 = 3.0;
/// Narrowest and widest the outliner and properties column gets, in logical pixels.
const SIDE_RANGE: (f32, f32) = (220.0, 720.0);
/// Smallest share of the 3D view or the pattern editor in their pair.
const MIN_SHARE: f32 = 0.15;
/// Shortest the outliner gets, and the least height it leaves the properties, in logical pixels.
const MIN_OUTLINER_PX: f32 = 80.0;
const MIN_PROPERTIES_PX: f32 = 160.0;

/// A border between areas that can be dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Border {
    /// Between the 3D view and the pattern editor.
    Main,
    /// Before the outliner and properties column.
    Side,
    /// Between the outliner and the properties editor.
    Outliner,
}

impl Border {
    /// Whether the border runs up and down (dragged sideways).
    const fn vertical(self) -> bool {
        !matches!(self, Self::Outliner)
    }
}

/// How the areas share the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaLayout {
    /// Share of the 3D view in the 3D view | pattern editor pair, 0..1.
    pub main_share: f32,
    /// Width of the outliner and properties column.
    pub side_px: f32,
    /// Height of the outliner.
    pub outliner_px: f32,
}

impl Default for AreaLayout {
    fn default() -> Self {
        Self {
            main_share: 0.5,
            side_px: 330.0,
            outliner_px: 250.0,
        }
    }
}

/// Where the borders lie now, for a drag to measure from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderSpots {
    /// The dragged border.
    pub here: Bounds<f32>,
    /// Left edge of the border before the side column.
    pub side_left: f32,
    /// Height of the outliner and properties column.
    pub column_height: f32,
}

impl AreaLayout {
    /// Moves `border` to the pointer at `(x, y)`.
    pub fn drag(&mut self, border: Border, x: f32, y: f32, spots: BorderSpots) {
        let centre = |b: Bounds<f32>| (b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0);
        match border {
            Border::Side => {
                self.side_px = (self.side_px + centre(spots.here).0 - x).clamp(SIDE_RANGE.0, SIDE_RANGE.1);
            }
            Border::Main => {
                let room = (spots.side_left - AREAS_PAD - BORDER_PX).max(1.0);
                self.main_share = ((x - AREAS_PAD) / room).clamp(MIN_SHARE, 1.0 - MIN_SHARE);
            }
            Border::Outliner => {
                let most = (spots.column_height - MIN_PROPERTIES_PX).max(MIN_OUTLINER_PX);
                self.outliner_px = (self.outliner_px + y - centre(spots.here).1).clamp(MIN_OUTLINER_PX, most);
            }
        }
    }
}

/// Id of border `border`'s element.
fn border_id(border: Border) -> String {
    format!("border-{border:?}")
}

impl AzamApp {
    /// The draggable border `border`.
    pub(crate) fn area_border(border: Border, cx: &mut Context<Self>) -> Div {
        div()
            .class(if border.vertical() {
                "area-split v"
            } else {
                "area-split h"
            })
            .id(border_id(border))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.resizing = Some(border);
                    cx.notify();
                }),
            )
    }

    /// Remembers where the borders were laid out (window bounds can only be read while a frame
    /// is prepared, not in event handlers).
    pub(crate) fn note_borders(&mut self, window: &Window) {
        self.border_bounds =
            [Border::Main, Border::Side, Border::Outliner].map(|b| window.bounds_of(&border_id(b)));
        self.window_width = window.viewport_size().width.0;
    }

    /// Follows the pointer with the border being dragged; returns `true` if one is.
    pub(crate) fn drag_border(&mut self, at: Point<Pixels>) -> bool {
        let Some(border) = self.resizing else {
            return false;
        };
        let index = match border {
            Border::Main => 0,
            Border::Side => 1,
            Border::Outliner => 2,
        };
        let Some(here) = self.border_bounds[index] else {
            return true;
        };
        let side = self.border_bounds[1];
        let spots = BorderSpots {
            here,
            side_left: side.map_or(self.window_width, |b| b.origin.x),
            column_height: side.map_or(here.size.height, |b| b.size.height),
        };
        self.layout.drag(border, at.x.0, at.y.0, spots);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<f32> {
        Bounds {
            origin: Point { x, y },
            size: Size { width: w, height: h },
        }
    }

    #[test]
    fn borders_follow_the_pointer_within_limits() {
        let mut l = AreaLayout::default();
        let side = bounds(1065.0, 30.0, 3.0, 800.0);
        let spots = |here| BorderSpots {
            here,
            side_left: 1065.0,
            column_height: 800.0,
        };
        l.drag(Border::Side, 966.5, 0.0, spots(side));
        assert!(
            (l.side_px - 430.0).abs() < 1e-3,
            "dragged 100 px left: {}",
            l.side_px
        );
        l.drag(Border::Side, 2000.0, 0.0, spots(side));
        assert!(
            (l.side_px - SIDE_RANGE.0).abs() < 1e-3,
            "the column keeps a usable width"
        );
        l.drag(Border::Main, 532.0, 0.0, spots(bounds(530.0, 30.0, 3.0, 800.0)));
        assert!((l.main_share - 530.0 / 1060.0).abs() < 1e-4);
        l.drag(Border::Main, 0.0, 0.0, spots(side));
        assert!((l.main_share - MIN_SHARE).abs() < 1e-6);
        let outliner = bounds(1070.0, 280.0, 330.0, 3.0);
        l.drag(Border::Outliner, 0.0, 331.5, spots(outliner));
        assert!((l.outliner_px - 300.0).abs() < 1e-3);
        l.drag(Border::Outliner, 0.0, 5000.0, spots(outliner));
        assert!((l.outliner_px - (800.0 - MIN_PROPERTIES_PX)).abs() < 1e-3);
    }
}
