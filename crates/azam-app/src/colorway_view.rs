//! The Colorways editor: the garment as the 3D view sees it, rendered once per colorway side by
//! side, so colorways compare at a glance (CLO's colorway view). Clicking one makes it active.

use std::sync::Arc;

use blenderui::Image;
use blenderui::prelude::*;

use crate::app::AzamApp;

/// Size of one colorway's picture, logical pixels.
const THUMB: (f32, f32) = (220.0, 300.0);

/// The rendered colorways and what they were rendered for.
#[derive(Default)]
pub struct ColorwayThumbs {
    key: String,
    /// Colorway id, name and picture, in colorway order.
    pub images: Vec<(azam_doc::ColorwayId, String, Arc<Image>)>,
}

impl AzamApp {
    /// Renders every colorway when the Colorways editor shows and something changed (the
    /// project, the camera, the drape).
    pub(crate) fn update_colorway_thumbs(&mut self, scale: f32) {
        let shown = self
            .layout()
            .leaves()
            .iter()
            .any(|&(_, e)| e == crate::editors::Editor::Colorways);
        if !shown {
            return;
        }
        let key = format!(
            "{}|{:?}|{:?}|{}",
            self.session.revision,
            self.viewport.camera,
            self.sim_time(),
            self.session.project.colorways.len()
        );
        if self.colorway_thumbs.key == key {
            return;
        }
        let mut colorways: Vec<_> = self.session.project.colorways.values().collect();
        colorways.sort_by_key(|c| c.order);
        let ids: Vec<(azam_doc::ColorwayId, String)> =
            colorways.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
        let (w, h) = ((THUMB.0 * scale) as u32, (THUMB.1 * scale) as u32);
        let camera = self.viewport.camera;
        let mut images = Vec::new();
        for (id, name) in ids {
            let mut project = self.session.project.clone();
            project.active_colorway = id.clone();
            let draws = self.scene3d.draws(
                &project,
                &crate::selection::Selection::default(),
                crate::operators::Shading::Solid,
            );
            let overlays = azam_render::Overlays {
                grid: false,
                ..azam_render::Overlays::default()
            };
            let Some(renderer) = self.viewport.renderer() else {
                return;
            };
            let pixels = renderer.render(&camera, &draws, overlays, w, h).to_vec();
            if let Some(img) = Image::from_rgba(w, h, &pixels) {
                images.push((id, name, Arc::new(img)));
            }
        }
        self.colorway_thumbs = ColorwayThumbs { key, images };
    }

    /// Adds a copy of the active colorway (its fabric colours to change from there) and makes it
    /// active.
    pub fn new_colorway(&mut self) {
        let source = self.session.project.active_colorway.clone();
        let mut made = None;
        self.session.edit("New Colorway", |p| {
            made = azam_doc::duplicate_colorway(p, &source);
            if let Some(id) = &made {
                p.active_colorway = id.clone();
            }
        });
        self.status = match made {
            Some(_) => "New colorway: change its fabric colours in the Fabric tab".into(),
            None => "Could not copy the colorway".into(),
        };
    }

    /// The Colorways editor's body: a card per colorway, the active one marked.
    pub(crate) fn colorways_body(&mut self, cx: &mut Context<Self>) -> Div {
        let active = self.session.project.active_colorway.clone();
        let mut grid = div().class("cw-grid").id("cw-grid");
        if self.colorway_thumbs.images.is_empty() {
            return grid.child(div().class("pnote").child("Rendering the colorways…"));
        }
        for (id, name, img) in self.colorway_thumbs.images.clone() {
            let picture = canvas(move |b, p| p.image(b, &img)).w(px(THUMB.0)).h(px(THUMB.1));
            let on = id == active;
            let card_id = format!("cw-{}", id.as_str());
            grid = grid.child(
                div()
                    .class(if on { "cw-card on" } else { "cw-card" })
                    .id(card_id)
                    .child(picture)
                    .child(div().class("cw-name").child(name))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let id = id.clone();
                        this.session.edit("Active Colorway", |p| p.active_colorway = id);
                        cx.notify();
                    })),
            );
        }
        grid
    }
}
