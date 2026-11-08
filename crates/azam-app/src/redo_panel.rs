//! Adjust Last Operation, as in Blender: after an operation with settings, a panel in the editor's
//! corner changes them, redoing the operation in place.

use azam_doc::{PieceId, PointId};
use blenderui::prelude::*;

use crate::app::AzamApp;

/// The last dart cut, as the panel redoes it.
#[derive(Clone, Debug, PartialEq)]
pub struct LastDart {
    /// The piece it was cut into.
    pub piece: PieceId,
    /// The segment it was cut into (before the cut).
    pub segment: PointId,
    /// Where along the segment, 0..1.
    pub t: f64,
    /// Width, mm.
    pub width: f64,
    /// Depth, mm.
    pub depth: f64,
    /// The project revision right after the cut: the panel shows while nothing else changed.
    pub revision: u64,
}

impl AzamApp {
    /// The Adjust Last Operation panel, while the last edit was a dart.
    pub(crate) fn redo_panel(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let last = self
            .last_dart
            .clone()
            .filter(|d| d.revision == self.session.revision)?;
        let field = |label: &'static str, value: f64, max: f64, depth: bool, cx: &mut Context<Self>| {
            NumberField()
                .attr("label", label)
                .value(format!("{value:.0}"))
                .min(1.0)
                .max(max)
                .step(1.0)
                .class("pfield")
                .on_input(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                    let v = e.number.clamp(1.0, max);
                    this.redo_dart(
                        if depth { None } else { Some(v) },
                        if depth { Some(v) } else { None },
                    );
                    cx.notify();
                }))
        };
        Some(
            div()
                .class("redo-panel")
                .child(
                    div()
                        .class("panel-head")
                        .child(self.icons.sized("DISCLOSURE_TRI_DOWN", 12.0, None))
                        .child("Add Dart"),
                )
                .child(
                    div()
                        .class("panel-body")
                        .child(
                            div()
                                .class("prow")
                                .child(field("Width (mm)", last.width, 200.0, false, cx)),
                        )
                        .child(
                            div()
                                .class("prow")
                                .child(field("Depth (mm)", last.depth, 400.0, true, cx)),
                        ),
                ),
        )
    }

    /// Undoes the last dart and cuts it again with a new `width` and/or `depth`.
    pub fn redo_dart(&mut self, width: Option<f64>, depth: Option<f64>) {
        let Some(mut last) = self
            .last_dart
            .clone()
            .filter(|d| d.revision == self.session.revision)
        else {
            return;
        };
        last.width = width.unwrap_or(last.width);
        last.depth = depth.unwrap_or(last.depth);
        self.session.undo();
        let mut result = Err(azam_doc::DartError::NoSuchSegment);
        self.session.edit("Add Dart", |p| {
            result = azam_doc::add_dart(p, &last.piece, &last.segment, last.t, last.width, last.depth);
            if result.is_ok() {
                azam_doc::sync_mirrors(p);
            }
        });
        self.status = match result {
            Ok(_) => format!("Dart {:.0} × {:.0} mm", last.width, last.depth),
            Err(e) => format!("No dart: {e}"),
        };
        // the tool cuts the next dart at this size too
        self.tools.dart.width = last.width;
        self.tools.dart.depth = last.depth;
        last.revision = self.session.revision;
        self.last_dart = Some(last);
    }

    /// The last dart, while the panel can still redo it.
    #[must_use]
    pub fn last_dart(&self) -> Option<&LastDart> {
        self.last_dart
            .as_ref()
            .filter(|d| d.revision == self.session.revision)
    }
}
