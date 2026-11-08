//! Adjust Last Operation, as in Blender: after an operation with settings, a panel in the editor's
//! corner changes them, redoing the operation in place (undo, then do it again with the new
//! settings).

use azam_doc::{PieceId, PointId, Shape};
use azam_geom::Vec2;
use blenderui::prelude::*;

use crate::app::AzamApp;

/// The last dart cut.
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
}

/// The last rectangle or ellipse piece drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct LastShape {
    /// Rectangle or ellipse.
    pub shape: Shape,
    /// The corner the drag started at.
    pub from: Vec2,
    /// Width and height, mm (signed: the drag's direction).
    pub size: Vec2,
}

/// An operation the panel can adjust.
#[derive(Clone, Debug, PartialEq)]
pub enum LastKind {
    /// A dart.
    Dart(LastDart),
    /// A drawn piece.
    Shape(LastShape),
    /// A rounded corner: the piece, the corner's index before rounding, the radius (mm).
    Fillet(PieceId, usize, f64),
    /// A spread or pleat: the piece, the line dragged and how it opened.
    Spread(PieceId, Vec2, Vec2, azam_doc::Spread),
}

/// The last adjustable operation and the project revision right after it: the panel shows while
/// nothing else changed.
#[derive(Clone, Debug, PartialEq)]
pub struct LastOp {
    /// What it did.
    pub kind: LastKind,
    /// The revision right after it.
    pub revision: u64,
}

/// A setting the panel shows: label, value, range.
struct Setting {
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
}

impl AzamApp {
    /// Remembers `kind` as the last adjustable operation (just done).
    pub(crate) fn remember_last(&mut self, kind: LastKind) {
        self.last_op = Some(LastOp {
            kind,
            revision: self.session.revision,
        });
    }

    /// The last adjustable operation, while the panel can still redo it.
    #[must_use]
    pub fn last_op(&self) -> Option<&LastKind> {
        self.last_op
            .as_ref()
            .filter(|op| op.revision == self.session.revision)
            .map(|op| &op.kind)
    }

    /// The last dart, while the panel can still redo it.
    #[must_use]
    pub fn last_dart(&self) -> Option<&LastDart> {
        match self.last_op() {
            Some(LastKind::Dart(d)) => Some(d),
            _ => None,
        }
    }

    /// The Adjust Last Operation panel, while the last edit can be adjusted.
    pub(crate) fn redo_panel(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let kind = self.last_op()?.clone();
        let setting = |label, value, min, max| Setting {
            label,
            value,
            min,
            max,
        };
        let (title, settings): (&str, Vec<Setting>) = match &kind {
            LastKind::Dart(d) => (
                "Add Dart",
                vec![
                    setting("Width (mm)", d.width, 1.0, 200.0),
                    setting("Depth (mm)", d.depth, 1.0, 400.0),
                ],
            ),
            LastKind::Shape(s) => (
                s.shape.name(),
                vec![
                    setting("Width (mm)", s.size.x.abs(), 5.0, 3000.0),
                    setting("Height (mm)", s.size.y.abs(), 5.0, 3000.0),
                ],
            ),
            LastKind::Fillet(_, _, r) => ("Fillet", vec![setting("Radius (mm)", *r, 1.0, 300.0)]),
            LastKind::Spread(_, _, _, azam_doc::Spread::Pleat { depth }) => {
                ("Add Pleat", vec![setting("Depth (mm)", *depth, 1.0, 500.0)])
            }
            LastKind::Spread(_, _, _, azam_doc::Spread::Fan { amount }) => (
                "Slash and Spread",
                vec![setting("Opening (mm)", *amount, 1.0, 500.0)],
            ),
        };
        let mut body = div().class("panel-body");
        for (i, setting) in settings.into_iter().enumerate() {
            let Setting {
                label,
                value,
                min,
                max,
            } = setting;
            body = body.child(
                div().class("prow").child(
                    NumberField()
                        .attr("label", label)
                        .value(format!("{value:.0}"))
                        .min(min)
                        .max(max)
                        .step(1.0)
                        .class("pfield")
                        .on_input(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                            this.adjust_last(i, e.number.clamp(min, max));
                            cx.notify();
                        })),
                ),
            );
        }
        Some(
            div()
                .class("redo-panel")
                .child(
                    div()
                        .class("panel-head")
                        .child(self.icons.sized("DISCLOSURE_TRI_DOWN", 12.0, None))
                        .child(title),
                )
                .child(body),
        )
    }

    /// Redoes the last adjustable operation with setting `index` (its first, second…) set to
    /// `value`.
    pub fn adjust_last(&mut self, index: usize, value: f64) {
        let Some(mut kind) = self.last_op().cloned() else {
            return;
        };
        self.session.undo();
        match &mut kind {
            LastKind::Dart(d) => {
                if index == 0 {
                    d.width = value;
                } else {
                    d.depth = value;
                }
                let mut result = Err(azam_doc::DartError::NoSuchSegment);
                self.session.edit("Add Dart", |p| {
                    result = azam_doc::add_dart(p, &d.piece, &d.segment, d.t, d.width, d.depth);
                    if result.is_ok() {
                        azam_doc::sync_mirrors(p);
                    }
                });
                self.status = match result {
                    Ok(_) => format!("Dart {:.0} × {:.0} mm", d.width, d.depth),
                    Err(e) => format!("No dart: {e}"),
                };
                // the tool cuts the next dart at this size too
                self.tools.dart.width = d.width;
                self.tools.dart.depth = d.depth;
            }
            LastKind::Shape(s) => {
                let signed = |v: f64, was: f64| if was < 0.0 { -v } else { v };
                if index == 0 {
                    s.size.x = signed(value, s.size.x);
                } else {
                    s.size.y = signed(value, s.size.y);
                }
                let (shape, from, to) = (s.shape, s.from, s.from + s.size);
                let mut created = None;
                self.session.edit("Create Pattern", |p| {
                    created = azam_doc::create_shape_piece(p, shape, from, to);
                });
                if let Some(id) = created {
                    self.session.selection.select_only(id);
                }
                self.status = format!(
                    "{} {:.0} × {:.0} mm",
                    shape.name(),
                    s.size.x.abs(),
                    s.size.y.abs()
                );
            }
            LastKind::Fillet(piece, index, radius) => {
                *radius = value;
                let (piece, index, r) = (piece.clone(), *index, *radius);
                self.session.edit("Fillet", |p| {
                    azam_doc::fillet_corner(p, &piece, index, r);
                    azam_doc::sync_mirrors(p);
                });
                self.tools.fillet.radius = r;
                self.status = format!("Rounded the corner ({r:.0} mm)");
            }
            LastKind::Spread(piece, from, to, how) => {
                match how {
                    azam_doc::Spread::Pleat { depth } => {
                        *depth = value;
                        self.tools.spread.depth = value;
                    }
                    azam_doc::Spread::Fan { amount } => {
                        *amount = value;
                        self.tools.spread.amount = value;
                    }
                }
                let (piece, from, to, how) = (piece.clone(), *from, *to, *how);
                let mut result = Err(azam_doc::CutError::NoPiece);
                self.session.edit(
                    if matches!(how, azam_doc::Spread::Pleat { .. }) {
                        "Add Pleat"
                    } else {
                        "Slash and Spread"
                    },
                    |p| {
                        result = azam_doc::spread_piece(p, &piece, from, to, how);
                        azam_doc::sync_mirrors(p);
                    },
                );
                self.status = match result {
                    Ok(()) => format!("Spread {value:.0} mm"),
                    Err(e) => format!("Not spread: {e}"),
                };
            }
        }
        self.remember_last(kind);
    }

    /// Redoes the last dart with a new `width` and/or `depth`.
    pub fn redo_dart(&mut self, width: Option<f64>, depth: Option<f64>) {
        if let Some(w) = width {
            self.adjust_last(0, w);
        }
        if let Some(d) = depth {
            self.adjust_last(1, d);
        }
    }
}
