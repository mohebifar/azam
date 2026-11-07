//! Blender's icons as elements, rasterized once per name, size and tint.

use std::collections::HashMap;
use std::sync::Arc;

use blenderui::prelude::*;
use blenderui::{Image, Rgba};

/// Logical size of a regular icon, in pixels.
pub const ICON_PX: f32 = 16.0;

/// Rasterized icons, keyed by name, device size and tint.
#[derive(Default)]
pub struct IconCache {
    images: HashMap<(&'static str, u32, u32), Option<Arc<Image>>>,
    scale: f32,
}

impl IconCache {
    /// Sets the window's device pixels per logical pixel; icons are rasterized at that density.
    pub fn set_scale(&mut self, scale: f32) {
        if (scale - self.scale).abs() > f32::EPSILON {
            self.scale = scale;
            self.images.clear();
        }
    }

    fn image(&mut self, name: &'static str, px: f32, tint: Option<u32>) -> Option<Arc<Image>> {
        let size = (px * self.scale.max(1.0)).round() as u32;
        let key = (name, size, tint.map_or(0, |t| t | 0xff00_0000));
        self.images
            .entry(key)
            .or_insert_with(|| {
                let tint = tint.map_or(Rgba::TRANSPARENT, rgb);
                Image::icon(name, size, tint).map(Arc::new)
            })
            .clone()
    }

    /// Icon `name` at the regular size in its theme colour.
    pub fn icon(&mut self, name: &'static str) -> Div {
        self.sized(name, ICON_PX, None)
    }

    /// Icon `name` at `px` logical pixels, optionally tinted.
    pub fn sized(&mut self, name: &'static str, px_size: f32, tint: Option<u32>) -> Div {
        let image = self.image(name, px_size, tint);
        canvas(move |b, p| {
            if let Some(image) = &image {
                let x = b.origin.x + (b.size.width - px_size) / 2.0;
                let y = b.origin.y + (b.size.height - px_size) / 2.0;
                p.image(
                    Bounds {
                        origin: point(x, y),
                        size: size(px_size, px_size),
                    },
                    image,
                );
            }
        })
        .w(px(px_size))
        .h(px(px_size))
        .flex_shrink_0()
    }
}
