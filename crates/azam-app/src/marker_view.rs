//! The Marker editor: the pattern nested on the fabric as a cutting room lays it, with the
//! fabric width, the gap between pieces, whether pieces may turn and which sizes go in, and the
//! length of fabric it takes and how much of it the pieces use. It follows every edit; the
//! settings are kept with the project.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use azam_geom::Vec2;

use azam_doc::Project;
use azam_io::{Marker, MarkerOptions};
use blenderui::prelude::*;
use serde_json::Value;

use crate::app::AzamApp;
use crate::properties::field;

/// [`Project::extra`] key holding the marker settings.
pub const MARKER_KEY: &str = "marker";
/// Colours of the pieces, one per size.
const FILLS: [u32; 6] = [0xe8a87c, 0x85c1e9, 0xa9dfbf, 0xf7dc6f, 0xd7bde2, 0xf5b7b1];
/// Fill and edge of a piece that overlaps another or leaves the fabric.
const CLASH_FILL: u32 = 0xe57373;
const CLASH_EDGE: u32 = 0xb71c1c;
/// Edge of the piece being dragged.
const SELECTED_EDGE: u32 = 0xffa040;
/// Width of a label character at the marker's text size, in logical pixels.
const LABEL_CHAR_PX: f32 = 6.0;
/// Height a piece needs to carry its label, in logical pixels.
const LABEL_HEIGHT_PX: f32 = 16.0;
/// Space around the drawn marker, in logical pixels.
const MARGIN_PX: f32 = 16.0;

/// The marker settings of a project.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerSettings {
    /// Fabric width, mm.
    pub width_mm: f64,
    /// Gap between pieces, mm.
    pub gap_mm: f64,
    /// Whether pieces may turn 180°.
    pub rotate_180: bool,
    /// One garment of every size (else of the size shown).
    pub all_sizes: bool,
    /// Pieces moved by hand: offset from where the nest put them, mm, by placement key.
    pub moves: BTreeMap<String, Vec2>,
}

impl MarkerSettings {
    /// The settings `project` keeps (the defaults where it keeps none).
    #[must_use]
    pub fn of(project: &Project) -> Self {
        let m = project.extra.get(MARKER_KEY);
        let num = |k: &str, d: f64| {
            m.and_then(|m| m.get(k))
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(d)
        };
        let flag = |k: &str, d: bool| m.and_then(|m| m.get(k)).and_then(Value::as_bool).unwrap_or(d);
        Self {
            width_mm: num("width", azam_io::DEFAULT_WIDTH_MM).max(50.0),
            gap_mm: num("gap", azam_io::DEFAULT_GAP_MM),
            rotate_180: flag("rotate", true),
            all_sizes: flag("allSizes", false),
            moves: m
                .and_then(|m| m.get("moves"))
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| {
                            let a = v.as_array()?;
                            let (x, y) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
                            (x.is_finite() && y.is_finite()).then(|| (k.clone(), Vec2::new(x, y)))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The nesting options for `project` with these settings.
    #[must_use]
    pub fn options(&self, project: &Project) -> MarkerOptions {
        let sizes = if self.all_sizes && !project.sizes.is_empty() {
            project.sizes.iter().map(|s| (s.clone(), 1)).collect()
        } else {
            vec![(project.active_size.clone(), 1)]
        };
        MarkerOptions {
            width_mm: self.width_mm,
            gap_mm: self.gap_mm,
            rotate_180: self.rotate_180,
            sizes,
            resolution_mm: azam_io::DEFAULT_RESOLUTION_MM,
        }
    }
}

/// Sets marker setting `key` of `project` to `value`.
fn set(project: &mut Project, key: &str, value: Value) {
    let entry = project
        .extra
        .entry(MARKER_KEY.to_owned())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(serde_json::Map::new());
    }
    if let Some(map) = entry.as_object_mut() {
        map.insert(key.to_owned(), value);
    }
}

/// The marker of `project` as its settings ask.
#[must_use]
pub fn marker_for(project: &Project) -> Marker {
    let settings = MarkerSettings::of(project);
    azam_io::make_marker(project, &settings.options(project)).with_moves(&settings.moves, settings.gap_mm)
}

/// A piece being dragged in the Marker editor.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerDrag {
    /// The copy's placement key.
    key: String,
    /// Where the pointer went down, window pixels.
    start: (f32, f32),
    /// Its saved hand offset when the drag began, mm.
    base: Vec2,
    /// How far it has been dragged so far, mm.
    delta: Vec2,
}

/// Where the last drawing put the marker: its top-left corner (window pixels) and pixels per mm.
pub type MarkerFrame = Arc<Mutex<Option<(f32, f32, f32)>>>;

impl AzamApp {
    /// The current marker, made again after the project changed.
    pub fn marker(&mut self) -> Arc<Marker> {
        let revision = self.session.revision;
        if let Some((r, m)) = &self.marker_cache
            && *r == revision
        {
            return Arc::clone(m);
        }
        let m = Arc::new(marker_for(&self.session.project));
        self.marker_cache = Some((revision, Arc::clone(&m)));
        m
    }

    /// The Marker editor's area.
    pub(crate) fn marker_area(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        let header = div()
            .class("area-header")
            .child(self.editor_type_button(area, cx))
            .child(div().class("mode-label").child("Marker"))
            .child(div().class("header-spacer"))
            .child(self.header_button("EXPORT", false, crate::operators::Operator::ExportMarkerDxf, cx));
        div()
            .class("area")
            .flex_1()
            .child(header)
            .child(self.marker_body(cx))
    }

    /// The settings, the figures and the drawing.
    fn marker_body(&mut self, cx: &mut Context<Self>) -> Div {
        let settings = MarkerSettings::of(&self.session.project);
        let marker = self.marker();
        let units = self.session.project.units;
        let controls = Self::marker_controls(&settings, units, cx);
        // a piece being dragged shows where it is now
        let marker = match &self.marker_drag {
            Some(d) => {
                let mut moves = BTreeMap::new();
                moves.insert(d.key.clone(), d.delta);
                Arc::new((*marker).clone().with_moves(&moves, settings.gap_mm))
            }
            None => marker,
        };
        let clashes = marker.clashes();
        let mut figures = format!(
            "Length {} · {} pieces · efficiency {:.1}%",
            crate::units::text(units, marker.length_mm),
            marker.placements.len(),
            marker.efficiency() * 100.0
        );
        if !marker.left_out.is_empty() {
            let _ = write!(
                figures,
                " · {} wider than the fabric, left out",
                marker.left_out.len()
            );
        }
        let clashing = clashes.iter().filter(|&&c| c).count();
        if clashing > 0 {
            let _ = write!(figures, " · {clashing} overlapping or off the fabric");
        }
        let renest = div()
            .class("pbtn marker-renest")
            .id("marker-renest")
            .child("Re-nest")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.session.edit("Re-nest Marker", |p| {
                    set(p, "moves", Value::Object(serde_json::Map::new()));
                });
                this.status = "Marker nested again; hand moves cleared".into();
                cx.notify();
            }));
        let frame = Arc::clone(&self.marker_frame);
        let dragged = self.marker_drag.as_ref().map(|d| d.key.clone());
        let drawing = {
            let marker = Arc::clone(&marker);
            canvas(move |b, p| paint_marker(b, p, &marker, &clashes, dragged.as_deref(), &frame))
                .flex_1()
                .w_full()
        };
        let drawing = div()
            .class("marker-canvas")
            .id("marker-canvas")
            .child(drawing)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.marker_press(e.position.x.0, e.position.y.0);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.marker_drag_to(e.position.x.0, e.position.y.0) {
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.marker_release();
                    cx.notify();
                }),
            );
        div()
            .class("marker-body")
            .flex_1()
            .child(controls)
            .child(
                div()
                    .class("marker-figures")
                    .id("marker-figures")
                    .child(figures)
                    .child(div().class("header-spacer"))
                    .child(renest),
            )
            .child(drawing)
    }

    /// The marker settings' fields, lengths shown in `units`.
    fn marker_controls(settings: &MarkerSettings, units: azam_doc::Units, cx: &mut Context<Self>) -> Div {
        // mm per displayed unit
        let factor = 1.0 / crate::units::per_mm(units);
        let (width_label, gap_label) = match units {
            azam_doc::Units::Mm => ("Fabric Width (mm)", "Gap (mm)"),
            azam_doc::Units::Cm => ("Fabric Width (cm)", "Gap (cm)"),
            azam_doc::Units::In => ("Fabric Width (in)", "Gap (in)"),
        };
        div()
            .class("marker-controls")
            .child(Self::number(
                field(width_label, 50.0 / factor, 5000.0 / factor, 10.0 / factor),
                settings.width_mm / factor,
                cx,
                move |p, v| set(p, "width", Value::from(v * factor)),
            ))
            .child(Self::number(
                field(gap_label, 0.0, 50.0 / factor, 1.0 / factor),
                settings.gap_mm / factor,
                cx,
                move |p, v| set(p, "gap", Value::from(v * factor)),
            ))
            .child(Self::choice(
                "Pieces May Turn",
                &["180°", "Never (one-way fabric)"],
                usize::from(!settings.rotate_180),
                cx,
                |p, i| set(p, "rotate", Value::from(i == 0)),
            ))
            .child(Self::choice(
                "Sizes",
                &["Size shown", "One of each size"],
                usize::from(settings.all_sizes),
                cx,
                |p, i| set(p, "allSizes", Value::from(i == 1)),
            ))
    }

    /// The piece under window point (`x`, `y`), by key, and its saved hand offset.
    fn marker_piece_at(&mut self, x: f32, y: f32) -> Option<(String, Vec2)> {
        let (ox, oy, scale) = (*self.marker_frame.lock().ok()?)?;
        let q = Vec2::new(f64::from((x - ox) / scale), f64::from((y - oy) / scale));
        let marker = self.marker();
        let hit = marker.placements.iter().rev().find(|p| contains(&p.outline, q))?;
        let key = hit.key();
        let base = MarkerSettings::of(&self.session.project)
            .moves
            .get(&key)
            .copied()
            .unwrap_or(Vec2::ZERO);
        Some((key, base))
    }

    /// Where marker point `q` (mm) was last drawn, in window pixels (once the editor has drawn).
    #[must_use]
    pub fn marker_to_window(&self, q: Vec2) -> Option<(f32, f32)> {
        let (ox, oy, scale) = (*self.marker_frame.lock().ok()?)?;
        Some((ox + q.x as f32 * scale, oy + q.y as f32 * scale))
    }

    /// The pointer went down at (`x`, `y`): starts dragging the piece there, if any.
    pub fn marker_press(&mut self, x: f32, y: f32) {
        self.marker_drag = self.marker_piece_at(x, y).map(|(key, base)| MarkerDrag {
            key,
            start: (x, y),
            base,
            delta: Vec2::ZERO,
        });
    }

    /// The pointer moved to (`x`, `y`): drags the piece along. `true` while dragging.
    pub fn marker_drag_to(&mut self, x: f32, y: f32) -> bool {
        let scale = self
            .marker_frame
            .lock()
            .ok()
            .and_then(|f| *f)
            .map_or(1.0, |(_, _, s)| s);
        let Some(d) = &mut self.marker_drag else {
            return false;
        };
        d.delta = Vec2::new(
            f64::from((x - d.start.0) / scale),
            f64::from((y - d.start.1) / scale),
        );
        true
    }

    /// The pointer came up: keeps where the dragged piece went, as one undoable edit.
    pub fn marker_release(&mut self) {
        let Some(d) = self.marker_drag.take() else {
            return;
        };
        if d.delta.length() < 0.5 {
            return;
        }
        let to = d.base + d.delta;
        let key = d.key.clone();
        self.session.edit("Move Marker Piece", |p| {
            let mut moves = MarkerSettings::of(p).moves;
            moves.insert(key, to);
            let map: serde_json::Map<String, Value> = moves
                .into_iter()
                .map(|(k, v)| (k, Value::from(vec![v.x, v.y])))
                .collect();
            set(p, "moves", Value::Object(map));
        });
        self.status = format!("Moved {} in the marker", d.key.replace('|', " "));
    }
}

/// Whether `q` lies inside the closed polygon `pts`.
fn contains(pts: &[Vec2], q: Vec2) -> bool {
    let n = pts.len();
    let mut odd = false;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if (a.y > q.y) != (b.y > q.y) && q.x < a.x + (q.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            odd = !odd;
        }
    }
    odd
}

/// Draws `marker` scaled to fit `b`: the fabric, then each piece in its size's colour.
fn paint_marker(
    b: Bounds<f32>,
    p: &mut Painter,
    marker: &Marker,
    clashes: &[bool],
    dragged: Option<&str>,
    frame: &MarkerFrame,
) {
    let (l, w) = (marker.length_mm.max(1.0) as f32, marker.width_mm.max(1.0) as f32);
    let room = (b.size.width - 2.0 * MARGIN_PX, b.size.height - 2.0 * MARGIN_PX);
    if room.0 <= 0.0 || room.1 <= 0.0 {
        return;
    }
    let scale = (room.0 / l).min(room.1 / w);
    let (ox, oy) = (b.origin.x + MARGIN_PX, b.origin.y + MARGIN_PX);
    if let Ok(mut f) = frame.lock() {
        *f = Some((ox, oy, scale));
    }
    let at = |q: azam_geom::Vec2| (ox + q.x as f32 * scale, oy + q.y as f32 * scale);
    p.quad(
        Bounds {
            origin: point(ox, oy),
            size: size(l * scale, w * scale),
        },
        0.0,
        rgb(0x3a3a3a),
        rgb(0x8a8a8a),
        1.0,
    );
    let mut sizes: Vec<&azam_geom::SizeName> = Vec::new();
    for (i, pl) in marker.placements.iter().enumerate() {
        let k = sizes.iter().position(|s| **s == pl.size).unwrap_or_else(|| {
            sizes.push(&pl.size);
            sizes.len() - 1
        });
        let pts: Vec<(f32, f32)> = pl.outline.iter().map(|&q| at(q)).collect();
        let clash = clashes.get(i).copied().unwrap_or(false);
        p.polygon(&pts, rgb(if clash { CLASH_FILL } else { FILLS[k % FILLS.len()] }));
        let mut ring = pts.clone();
        if let Some(&first) = pts.first() {
            ring.push(first);
        }
        let held = dragged == Some(pl.key().as_str());
        let (edge, width) = if held {
            (rgb(SELECTED_EDGE), 2.0)
        } else if clash {
            (rgb(CLASH_EDGE), 1.5)
        } else {
            (rgb(0x1a1a1a), 1.0)
        };
        p.line(&ring, edge, width);
        let n = pts.len().max(1) as f32;
        let (cx, cy) = pts
            .iter()
            .fold((0.0, 0.0), |(x, y), q| (x + q.0 / n, y + q.1 / n));
        let label = format!("{} {}", pl.name, pl.size.0);
        // only where it fits inside the piece, so small pieces' names do not spill over others
        let (x0, x1) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.0), b.max(q.0)));
        let (y0, y1) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.1), b.max(q.1)));
        let text_w = label.chars().count() as f32 * LABEL_CHAR_PX;
        if text_w < (x1 - x0) * 0.9 && y1 - y0 > LABEL_HEIGHT_PX {
            p.text(cx - text_w / 2.0, cy - 6.0, &label, 11.0, rgb(0x1a1a1a));
        }
    }
}
