//! The Marker editor: the pattern nested on the fabric as a cutting room lays it, with the
//! fabric width, the gap between pieces, whether pieces may turn and which sizes go in, and the
//! length of fabric it takes and how much of it the pieces use. It follows every edit; the
//! settings are kept with the project.

use std::fmt::Write as _;
use std::sync::Arc;

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
    azam_io::make_marker(project, &MarkerSettings::of(project).options(project))
}

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
        // mm per displayed unit
        let factor = 1.0 / crate::units::per_mm(units);
        let (width_label, gap_label) = match units {
            azam_doc::Units::Mm => ("Fabric Width (mm)", "Gap (mm)"),
            azam_doc::Units::Cm => ("Fabric Width (cm)", "Gap (cm)"),
            azam_doc::Units::In => ("Fabric Width (in)", "Gap (in)"),
        };
        let controls = div()
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
            ));
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
        let drawing = {
            let marker = Arc::clone(&marker);
            canvas(move |b, p| paint_marker(b, p, &marker)).flex_1().w_full()
        };
        div()
            .class("marker-body")
            .flex_1()
            .child(controls)
            .child(div().class("marker-figures").id("marker-figures").child(figures))
            .child(drawing)
    }
}

/// Draws `marker` scaled to fit `b`: the fabric, then each piece in its size's colour.
fn paint_marker(b: Bounds<f32>, p: &mut Painter, marker: &Marker) {
    let (l, w) = (marker.length_mm.max(1.0) as f32, marker.width_mm.max(1.0) as f32);
    let room = (b.size.width - 2.0 * MARGIN_PX, b.size.height - 2.0 * MARGIN_PX);
    if room.0 <= 0.0 || room.1 <= 0.0 {
        return;
    }
    let scale = (room.0 / l).min(room.1 / w);
    let (ox, oy) = (b.origin.x + MARGIN_PX, b.origin.y + MARGIN_PX);
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
    for pl in &marker.placements {
        let k = sizes.iter().position(|s| **s == pl.size).unwrap_or_else(|| {
            sizes.push(&pl.size);
            sizes.len() - 1
        });
        let pts: Vec<(f32, f32)> = pl.outline.iter().map(|&q| at(q)).collect();
        p.polygon(&pts, rgb(FILLS[k % FILLS.len()]));
        let mut ring = pts.clone();
        if let Some(&first) = pts.first() {
            ring.push(first);
        }
        p.line(&ring, rgb(0x1a1a1a), 1.0);
        let n = pts.len().max(1) as f32;
        let (cx, cy) = pts
            .iter()
            .fold((0.0, 0.0), |(x, y), q| (x + q.0 / n, y + q.1 / n));
        let label = format!("{} {}", pl.name, pl.size.0);
        p.text(
            cx - label.len() as f32 * 3.0,
            cy - 6.0,
            &label,
            11.0,
            rgb(0x1a1a1a),
        );
    }
}
