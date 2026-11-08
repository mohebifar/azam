//! Blender-style tooltips: hovering a tool, tab or header button for a moment shows its name and
//! shortcut next to the pointer.

use std::time::{Duration, Instant};

use blenderui::prelude::*;

use crate::app::AzamApp;

/// How long the pointer rests on an element before its tooltip shows (Blender: about half a second).
const TOOLTIP_DELAY: Duration = Duration::from_millis(450);
/// A tooltip hides once the pointer is this far from where it showed, in logical pixels (a jump
/// of the pointer may skip the element's hover-out).
const TOOLTIP_REACH: f32 = 32.0;
/// Offset of the tooltip from the pointer, in logical pixels.
const TOOLTIP_OFFSET: (f32, f32) = (14.0, 20.0);

/// The tooltip of the element under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct Tooltip {
    /// The text (name, then shortcut).
    pub text: String,
    /// Where the pointer was when it entered the element, in logical pixels.
    pub at: (f32, f32),
    /// When the pointer entered the element.
    pub since: Instant,
}

impl AzamApp {
    /// `el` with a tooltip that shows `text` while the pointer rests on it.
    pub(crate) fn with_tooltip(el: Div, text: String, cx: &mut Context<Self>) -> Div {
        el.on_hover(cx.listener(move |this, entered: &bool, window, cx| {
            if *entered {
                let p = window.mouse_position();
                this.tooltip = Some(Tooltip {
                    text: text.clone(),
                    at: (p.x.0, p.y.0),
                    since: Instant::now(),
                });
                window.request_animation_frame();
            } else if this.tooltip.as_ref().is_some_and(|t| t.text == text) {
                this.tooltip = None;
            }
            cx.notify();
        }))
    }

    /// Drops the tooltip if the pointer at `at` has moved away from where it showed; returns
    /// `true` if it did.
    pub(crate) fn tooltip_left_behind(&mut self, at: Point<Pixels>) -> bool {
        let far = self
            .tooltip
            .as_ref()
            .is_some_and(|t| (at.x.0 - t.at.0).hypot(at.y.0 - t.at.1) > TOOLTIP_REACH);
        if far {
            self.tooltip = None;
        }
        far
    }

    /// The tooltip to draw over the window, once its delay has passed (until then, asks for
    /// another frame).
    pub(crate) fn tooltip_overlay(&self, window: &mut Window) -> Option<Div> {
        if !self.prefs.tooltips {
            return None;
        }
        let tip = self.tooltip.as_ref()?;
        if tip.since.elapsed() < TOOLTIP_DELAY {
            window.request_animation_frame();
            return None;
        }
        let (name, shortcut) = tip
            .text
            .split_once('\t')
            .map_or((tip.text.as_str(), None), |(n, s)| (n, Some(s)));
        Some(
            div()
                .class("tooltip")
                .css("left", format!("{}px", tip.at.0 + TOOLTIP_OFFSET.0))
                .css("top", format!("{}px", tip.at.1 + TOOLTIP_OFFSET.1))
                .child(div().class("tooltip-name").child(name.to_owned()))
                .when_some(shortcut, |el, s| {
                    el.child(div().class("tooltip-key").child(format!("Shortcut: {s}")))
                }),
        )
    }
}
