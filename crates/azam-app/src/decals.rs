//! Graphics printed on pieces in 3D: all of a piece's graphics composited into one image that the
//! viewport and the path tracer lay over the cloth by its pattern coordinates.

use std::collections::HashMap;
use std::sync::Arc;

use azam_doc::{Extras, Graphic, Piece};
use azam_geom::Vec2;

/// Texels per mm of a composited image.
const TEXELS_PER_MM: f64 = 4.0;
/// Longest side of a composited image, in texels.
const MAX_TEXELS: f64 = 1024.0;
/// A graphic whose image cannot be read shows as this translucent grey (sRGB RGBA).
const PLACEHOLDER: [u8; 4] = [200, 200, 200, 150];

/// An RGBA image: sRGB, straight alpha, row by row, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub data: Vec<u8>,
}

/// A piece's graphics composited into one image, and where it lies on the pattern (mm).
#[derive(Clone, Debug, PartialEq)]
pub struct PieceDecal {
    /// The image.
    pub image: Rgba,
    /// Pattern position of the image's top-left corner.
    pub min: Vec2,
    /// Size of the image on the pattern.
    pub size: Vec2,
}

impl PieceDecal {
    /// Placement for the renderer: `[min x, min y, 1 / width, 1 / height]` in pattern mm.
    #[must_use]
    pub fn rect(&self) -> [f32; 4] {
        [
            self.min.x as f32,
            self.min.y as f32,
            (1.0 / self.size.x) as f32,
            (1.0 / self.size.y) as f32,
        ]
    }
}

/// Decoded graphic images by file path. Files that cannot be read are tried again next time, so
/// a graphic whose file appears later shows up.
#[derive(Default)]
pub struct ImageCache {
    images: HashMap<String, Arc<Rgba>>,
}

impl ImageCache {
    /// The image at `path`, decoded once.
    pub fn get(&mut self, path: &str) -> Option<Arc<Rgba>> {
        if let Some(img) = self.images.get(path) {
            return Some(Arc::clone(img));
        }
        let img = blenderui::Image::open(path)?;
        let rgba = Arc::new(Rgba {
            width: img.width,
            height: img.height,
            data: img.to_rgba(),
        });
        self.images.insert(path.to_owned(), Arc::clone(&rgba));
        Some(rgba)
    }
}

/// What decides a piece's composited image: its graphics as stored.
#[must_use]
pub fn decal_key(piece: &Piece) -> String {
    format!("{:?}", piece.graphics)
}

/// The corners of graphic `g` on the pattern, and the rotation that undoes its turn.
fn frame(g: &Graphic) -> ([Vec2; 4], (f64, f64)) {
    let size = azam_doc::graphic_size(g);
    let rot = g.num(azam_doc::GRAPHIC_ROTATION_KEY, 0.0).to_radians();
    let (s, c) = rot.sin_cos();
    let centre = Vec2::new(g.x, g.y);
    let corner = |sx: f64, sy: f64| {
        let l = Vec2::new(sx * size.x / 2.0, sy * size.y / 2.0);
        centre + Vec2::new(l.x * c - l.y * s, l.x * s + l.y * c)
    };
    (
        [
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, 1.0),
        ],
        (-rot).sin_cos(),
    )
}

/// `piece`'s graphics composited into one image (later graphics over earlier ones), or `None`
/// if it has none.
pub fn piece_decal(piece: &Piece, cache: &mut ImageCache) -> Option<PieceDecal> {
    let mut min = Vec2::splat(f64::INFINITY);
    let mut max = Vec2::splat(f64::NEG_INFINITY);
    for g in &piece.graphics {
        for q in frame(g).0 {
            min = min.min(q);
            max = max.max(q);
        }
    }
    let extent = max - min;
    if !(extent.x > 0.0 && extent.y > 0.0 && extent.x.is_finite() && extent.y.is_finite()) {
        return None;
    }
    let scale = TEXELS_PER_MM.min(MAX_TEXELS / extent.x.max(extent.y));
    let (w, h) = (
        (extent.x * scale).ceil().max(1.0) as u32,
        (extent.y * scale).ceil().max(1.0) as u32,
    );
    let mut data = vec![0u8; (w * h * 4) as usize];
    for g in &piece.graphics {
        let size = azam_doc::graphic_size(g);
        let opacity = g.num(azam_doc::GRAPHIC_OPACITY_KEY, 1.0).clamp(0.0, 1.0);
        let image = g
            .text(azam_doc::GRAPHIC_PATH_KEY)
            .filter(|p| !p.is_empty())
            .and_then(|p| cache.get(p));
        let (corners, (s, c)) = frame(g);
        let lo = corners.iter().fold(Vec2::splat(f64::INFINITY), |a, q| a.min(*q));
        let hi = corners
            .iter()
            .fold(Vec2::splat(f64::NEG_INFINITY), |a, q| a.max(*q));
        let x0 = ((lo.x - min.x) * scale).floor().max(0.0) as u32;
        let y0 = ((lo.y - min.y) * scale).floor().max(0.0) as u32;
        let x1 = (((hi.x - min.x) * scale).ceil() as u32).min(w);
        let y1 = (((hi.y - min.y) * scale).ceil() as u32).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let q = min + Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5) / scale;
                let d = q - Vec2::new(g.x, g.y);
                let u = (d.x * c - d.y * s) / size.x + 0.5;
                let v = (d.x * s + d.y * c) / size.y + 0.5;
                if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                    continue;
                }
                let src = match &image {
                    Some(img) if img.width > 0 && img.height > 0 => {
                        let ix = ((u * f64::from(img.width)) as u32).min(img.width - 1);
                        let iy = ((v * f64::from(img.height)) as u32).min(img.height - 1);
                        let at = ((iy * img.width + ix) * 4) as usize;
                        img.data
                            .get(at..at + 4)
                            .map_or(PLACEHOLDER, |p| [p[0], p[1], p[2], p[3]])
                    }
                    _ => PLACEHOLDER,
                };
                let a = f64::from(src[3]) / 255.0 * opacity;
                let at = ((y * w + x) * 4) as usize;
                let dst = &mut data[at..at + 4];
                let da = f64::from(dst[3]) / 255.0;
                let out_a = a + da * (1.0 - a);
                if out_a <= 0.0 {
                    continue;
                }
                for k in 0..3 {
                    let blended = (f64::from(src[k]) * a + f64::from(dst[k]) * da * (1.0 - a)) / out_a;
                    dst[k] = blended.round().clamp(0.0, 255.0) as u8;
                }
                dst[3] = (out_a * 255.0).round() as u8;
            }
        }
    }
    Some(PieceDecal {
        image: Rgba {
            width: w,
            height: h,
            data,
        },
        min,
        size: Vec2::new(f64::from(w), f64::from(h)) / scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use azam_doc::{Project, Sex};

    #[test]
    fn a_graphic_without_an_image_prints_a_placeholder_where_it_sits() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (300.0, 0.0), (300.0, 300.0), (0.0, 300.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = azam_doc::create_piece(&mut p, &square).expect("piece");
        assert!(
            piece_decal(&p.pieces[&id], &mut ImageCache::default()).is_none(),
            "no graphics, no image"
        );
        azam_doc::add_graphic(&mut p, &id, Vec2::new(150.0, 100.0), Vec2::new(80.0, 60.0), None)
            .expect("graphic");
        let decal = piece_decal(&p.pieces[&id], &mut ImageCache::default()).expect("decal");
        let size = azam_doc::graphic_size(&p.pieces[&id].graphics[0]);
        assert!((decal.size.x - size.x).abs() <= 0.25 && (decal.size.y - size.y).abs() <= 0.25);
        assert!((decal.min.x - (150.0 - size.x / 2.0)).abs() < 1e-9);
        let centre = ((decal.image.height / 2 * decal.image.width + decal.image.width / 2) * 4) as usize;
        assert_eq!(&decal.image.data[centre..centre + 4], &PLACEHOLDER);
    }
}
