//! What is printed or sewn onto pieces, in 3D: the fabric's texture image (tiled along the grain),
//! then a piece's graphics, topstitches, buttons and buttonholes, composited into one image that
//! the viewport and the path tracer lay over the cloth by its pattern coordinates.

use std::collections::HashMap;
use std::sync::Arc;

use azam_doc::{Extras, Graphic, PieceId, Project};
use azam_geom::Vec2;

use crate::pattern::{ButtonDraw, HoleDraw, StitchDraw};

/// Texels per mm of a composited image.
const TEXELS_PER_MM: f64 = 4.0;
/// Longest side of a composited image, in texels.
const MAX_TEXELS: f64 = 2048.0;
/// Thread narrower than this many texels is drawn this wide, so stitches stay visible.
const MIN_THREAD_TEXELS: f64 = 1.2;
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

/// Everything printed or sewn onto a piece, as drawn.
struct Prints<'a> {
    graphics: &'a [Graphic],
    stitches: Vec<StitchDraw>,
    buttons: Vec<ButtonDraw>,
    holes: Vec<HoleDraw>,
}

impl<'a> Prints<'a> {
    fn of(project: &'a Project, piece: &PieceId) -> Option<Self> {
        let graphics = project.pieces.get(piece)?.graphics.as_slice();
        let stitches = crate::scene::piece_stitches(project, piece);
        let (buttons, holes) = crate::scene::piece_trims(project, piece);
        let any = !graphics.is_empty() || !stitches.is_empty() || !buttons.is_empty() || !holes.is_empty();
        any.then_some(Self {
            graphics,
            stitches,
            buttons,
            holes,
        })
    }

    /// Bounds of every mark on the pattern.
    fn bounds(&self) -> (Vec2, Vec2) {
        let mut min = Vec2::splat(f64::INFINITY);
        let mut max = Vec2::splat(f64::NEG_INFINITY);
        let mut add = |q: Vec2, r: f64| {
            min = min.min(q - Vec2::splat(r));
            max = max.max(q + Vec2::splat(r));
        };
        for g in self.graphics {
            for q in frame(g).0 {
                add(q, 0.0);
            }
        }
        for st in &self.stitches {
            for q in st.rows.iter().flatten() {
                add(*q, st.width);
            }
        }
        for b in &self.buttons {
            add(b.at, b.diameter / 2.0);
        }
        for h in &self.holes {
            add(h.at, h.length / 2.0 + 2.0);
        }
        (min, max)
    }
}

/// What decides a piece's composited image: its graphics, topstitches, buttons and buttonholes
/// as stored, and the size shown (topstitches follow the graded outline).
#[must_use]
pub fn decal_key(project: &Project, piece: &PieceId) -> String {
    let on = |p: &Option<PieceId>| p.as_ref() == Some(piece);
    let texture = texture_of(project, piece).map(|(path, mm, grain, _)| format!("{path}@{mm}/{grain}"));
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{}",
        texture,
        project.pieces.get(piece).map(|pc| &pc.graphics),
        project.stitches.values().filter(|s| on(&s.p)).collect::<Vec<_>>(),
        project.buttons.values().filter(|b| on(&b.p)).collect::<Vec<_>>(),
        project.holes.values().filter(|h| on(&h.p)).collect::<Vec<_>>(),
        project.active_size.0
    )
}

/// An image being composited, laid over the pattern from `min` at `scale` texels per mm.
struct Canvas {
    width: u32,
    height: u32,
    data: Vec<u8>,
    min: Vec2,
    scale: f64,
}

impl Canvas {
    /// Paints, over what is there, every texel whose centre lies in `lo`..`hi` on the pattern and
    /// for which `paint` gives a colour (sRGB RGBA, straight alpha).
    fn paint(&mut self, lo: Vec2, hi: Vec2, mut paint: impl FnMut(Vec2) -> Option<[u8; 4]>) {
        let x0 = ((lo.x - self.min.x) * self.scale).floor().max(0.0) as u32;
        let y0 = ((lo.y - self.min.y) * self.scale).floor().max(0.0) as u32;
        let x1 = (((hi.x - self.min.x) * self.scale).ceil().max(0.0) as u32).min(self.width);
        let y1 = (((hi.y - self.min.y) * self.scale).ceil().max(0.0) as u32).min(self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let q = self.min + Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5) / self.scale;
                let Some(src) = paint(q) else { continue };
                let at = ((y * self.width + x) * 4) as usize;
                blend(&mut self.data[at..at + 4], src);
            }
        }
    }

    /// A line from `a` to `b`, `width` mm wide, in `color`, with square ends (so dashes keep their
    /// gaps however wide the thread).
    fn line(&mut self, a: Vec2, b: Vec2, width: f64, color: [u8; 4]) {
        let half = (width.max(MIN_THREAD_TEXELS / self.scale)) / 2.0;
        let pad = Vec2::splat(half);
        self.paint(a.min(b) - pad, a.max(b) + pad, |q| {
            {
                // unclamped projection: points past either end get no paint
                let (d, w) = (b - a, q - a);
                let len_sq = d.x * d.x + d.y * d.y;
                if len_sq <= 0.0 {
                    return None;
                }
                let t = (w.x * d.x + w.y * d.y) / len_sq;
                let across = (w.x * d.y - w.y * d.x).abs() / len_sq.sqrt();
                ((0.0..=1.0).contains(&t) && across <= half).then_some(color)
            }
        });
    }
}

/// `src` (straight alpha) over `dst`, in place.
fn blend(dst: &mut [u8], src: [u8; 4]) {
    let a = f64::from(src[3]) / 255.0;
    let da = f64::from(dst[3]) / 255.0;
    let out_a = a + da * (1.0 - a);
    if out_a <= 0.0 {
        return;
    }
    for k in 0..3 {
        let c = (f64::from(src[k]) * a + f64::from(dst[k]) * da * (1.0 - a)) / out_a;
        dst[k] = c.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round() as u8;
}

/// `0xRRGGBB` as opaque RGBA, scaled by `k` (to shade).
fn rgba(color: u32, k: f64) -> [u8; 4] {
    let ch = |shift: u32| ((f64::from((color >> shift) & 0xff) * k).round().clamp(0.0, 255.0)) as u8;
    [ch(16), ch(8), ch(0), 255]
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

/// What is printed or sewn onto `piece` composited into one image: graphics (later over
/// earlier), then topstitches as dashed thread, then buttons and buttonholes. `None` if there is
/// nothing.
pub fn piece_decal(project: &Project, piece: &PieceId, cache: &mut ImageCache) -> Option<PieceDecal> {
    let texture = texture_of(project, piece);
    let prints = Prints::of(project, piece);
    if prints.is_none() && texture.is_none() {
        return None;
    }
    let prints = prints.unwrap_or(Prints {
        graphics: &[],
        stitches: Vec::new(),
        buttons: Vec::new(),
        holes: Vec::new(),
    });
    let (mut min, mut max) = prints.bounds();
    if let Some((.., outline)) = &texture {
        // the texture covers the whole piece
        for q in outline {
            min = min.min(*q);
            max = max.max(*q);
        }
    }
    let extent = max - min;
    if !(extent.x > 0.0 && extent.y > 0.0 && extent.x.is_finite() && extent.y.is_finite()) {
        return None;
    }
    let scale = TEXELS_PER_MM.min(MAX_TEXELS / extent.x.max(extent.y));
    let (width, height) = (
        (extent.x * scale).ceil().max(1.0) as u32,
        (extent.y * scale).ceil().max(1.0) as u32,
    );
    let mut canvas = Canvas {
        width,
        height,
        data: vec![0u8; (width * height * 4) as usize],
        min,
        scale,
    };
    if let Some((path, mm, grain, _)) = &texture {
        paint_texture(&mut canvas, path, *mm, *grain, cache);
    }
    for g in prints.graphics {
        paint_graphic(&mut canvas, g, cache);
    }
    for st in &prints.stitches {
        paint_stitch(&mut canvas, st);
    }
    for b in &prints.buttons {
        paint_button(&mut canvas, b);
    }
    for h in &prints.holes {
        paint_hole(&mut canvas, h);
    }
    Some(PieceDecal {
        image: Rgba {
            width,
            height,
            data: canvas.data,
        },
        min,
        size: Vec2::new(f64::from(width), f64::from(height)) / scale,
    })
}

/// The texture of `piece`'s fabric (in the active colorway): its image, tile width (mm), the
/// piece's grain angle (degrees) and its outline polygon.
fn texture_of(project: &Project, piece: &PieceId) -> Option<(String, f64, f64, Vec<Vec2>)> {
    let pc = project.pieces.get(piece)?;
    let fabric = project.effective_fabric(&pc.fabric)?;
    let (path, mm) = fabric.texture()?;
    Some((path.to_owned(), mm, pc.grain, pc.outline().poly))
}

/// The texture image at `path` tiled over the whole canvas, `mm` wide per tile, its width running
/// along the grain (`grain` degrees). An image that cannot be read paints nothing.
fn paint_texture(canvas: &mut Canvas, path: &str, mm: f64, grain: f64, cache: &mut ImageCache) {
    let Some(img) = cache.get(path) else { return };
    if img.width == 0 || img.height == 0 {
        return;
    }
    let tile = Vec2::new(mm, mm * f64::from(img.height) / f64::from(img.width));
    let (s, c) = (-grain.to_radians()).sin_cos();
    let lo = canvas.min;
    let hi = canvas.min + Vec2::new(f64::from(canvas.width), f64::from(canvas.height)) / canvas.scale;
    canvas.paint(lo, hi, |q| {
        let u = Vec2::new(q.x * c - q.y * s, q.x * s + q.y * c);
        let x = ((u.x / tile.x).rem_euclid(1.0) * f64::from(img.width)) as u32;
        let y = ((u.y / tile.y).rem_euclid(1.0) * f64::from(img.height)) as u32;
        let at = ((y.min(img.height - 1) * img.width + x.min(img.width - 1)) * 4) as usize;
        Some([img.data[at], img.data[at + 1], img.data[at + 2], 255])
    });
}

/// Graphic `g`: its image (or a grey placeholder), turned and scaled into its frame.
fn paint_graphic(canvas: &mut Canvas, g: &Graphic, cache: &mut ImageCache) {
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
    let centre = Vec2::new(g.x, g.y);
    canvas.paint(lo, hi, |q| {
        let d = q - centre;
        let u = (d.x * c - d.y * s) / size.x + 0.5;
        let v = (d.x * s + d.y * c) / size.y + 0.5;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let mut src = match &image {
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
        src[3] = (f64::from(src[3]) * opacity).round() as u8;
        Some(src)
    });
}

/// Topstitch `st`: dashes of its stitch length with its gaps, along each row.
fn paint_stitch(canvas: &mut Canvas, st: &StitchDraw) {
    let color = rgba(st.color, 1.0);
    if !st.tape.is_empty() {
        paint_zipper_tape(canvas, st);
    }
    let (on, off) = (st.length.max(0.5), st.gap.max(0.0));
    for row in &st.rows {
        // distance along the row where the current dash or gap started, and whether it is a dash
        let mut phase = 0.0;
        let mut dash = true;
        for w in row.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = a.distance(b);
            let mut t = 0.0;
            while t < len {
                let left = if dash { on } else { off } - phase;
                let step = left.min(len - t);
                if dash && step > 0.0 {
                    let dir = (b - a) / len;
                    canvas.line(a + dir * t, a + dir * (t + step), st.width, color);
                }
                t += step;
                phase += step;
                if phase >= if dash { on } else { off } - 1e-9 {
                    phase = 0.0;
                    dash = !dash;
                }
            }
        }
    }
}

/// A zipper half's tape along each row, and its pull at the start of the first row.
fn paint_zipper_tape(canvas: &mut Canvas, st: &StitchDraw) {
    let tape = rgba(
        crate::scene::parse_hex(azam_doc::ZIPPER_TAPE_COLOR).unwrap_or(0x2a2a2c),
        1.0,
    );
    for line in &st.tape {
        for w in line.windows(2) {
            canvas.line(w[0], w[1], azam_doc::ZIPPER_TAPE_MM, tape);
        }
    }
    let Some(row) = st.rows.first().filter(|r| r.len() >= 2) else {
        return;
    };
    // the pull: a slider over the teeth and a tab hanging from it
    let along = (row[1] - row[0]).normalize();
    let (at, metal, shade) = (row[0] + along * 8.0, rgba(st.color, 1.15), rgba(st.color, 0.7));
    canvas.line(at - along * 5.0, at + along * 5.0, 9.0, shade);
    canvas.line(at - along * 4.0, at + along * 4.0, 7.0, metal);
    let across = Vec2::new(-along.y, along.x);
    canvas.line(at, at + across * 16.0, 5.0, metal);
}

/// Button `b`: a disc in its colour with a darker rim and four thread holes.
fn paint_button(canvas: &mut Canvas, b: &ButtonDraw) {
    let r = b.diameter / 2.0;
    let (face, rim, hole) = (rgba(b.color, 1.0), rgba(b.color, 0.6), rgba(b.color, 0.35));
    let hole_r = r * 0.12;
    let holes =
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| b.at + Vec2::new(x, y) * (r * 0.3));
    canvas.paint(b.at - Vec2::splat(r), b.at + Vec2::splat(r), |q| {
        let d = q.distance(b.at);
        if d > r {
            None
        } else if holes.iter().any(|h| q.distance(*h) <= hole_r) {
            Some(hole)
        } else if d > r * 0.82 {
            Some(rim)
        } else {
            Some(face)
        }
    });
}

/// Buttonhole `h`: a slit with dense bar-tacked thread around it.
fn paint_hole(canvas: &mut Canvas, h: &HoleDraw) {
    let (s, c) = h.angle.to_radians().sin_cos();
    let along = Vec2::new(c, s) * (h.length / 2.0);
    let (a, b) = (h.at - along, h.at + along);
    let thread = [60, 60, 60, 255];
    let slit = [25, 25, 25, 255];
    canvas.line(a, b, 3.0, thread);
    canvas.line(a, b, 0.8, slit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use azam_doc::{Project, Sex};

    #[test]
    fn a_fabric_texture_tiles_over_the_whole_piece() {
        // a 2 × 1 texture: red on the left, blue on the right
        let path = std::env::temp_dir().join(format!("azam-texture-{}.png", std::process::id()));
        {
            let file = std::fs::File::create(&path).expect("file");
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 2, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .and_then(|mut w| w.write_image_data(&[255, 0, 0, 255, 0, 0, 255, 255]))
                .expect("png");
        }
        let mut p = azam_doc::from_template(azam_doc::Template::Tee, Sex::Female, None, None, 0.0);
        let id = p.pieces.keys().next().cloned().expect("a piece");
        let fid = p.pieces[&id].fabric.clone();
        let f = p.fabrics.get_mut(&fid).expect("fabric");
        f.extra.insert(
            azam_doc::FABRIC_TEXTURE_KEY.to_owned(),
            path.to_string_lossy().into_owned().into(),
        );
        f.extra
            .insert(azam_doc::FABRIC_TEXTURE_MM_KEY.to_owned(), 40.0.into());
        p.pieces.get_mut(&id).expect("piece").graphics.clear();
        let decal = piece_decal(&p, &id, &mut ImageCache::default()).expect("a texture makes an image");
        let _ = std::fs::remove_file(&path);
        let px = |k: usize| &decal.image.data[k * 4..k * 4 + 4];
        let n = (decal.image.width * decal.image.height) as usize;
        let reds = (0..n).filter(|&k| px(k)[0] > 200 && px(k)[2] < 50).count();
        let blues = (0..n).filter(|&k| px(k)[2] > 200 && px(k)[0] < 50).count();
        assert!(
            reds > n / 3 && blues > n / 3,
            "both halves of the tile repeat: {reds} red, {blues} blue of {n}"
        );
        let ol = p.pieces[&id].outline().poly;
        let bb = azam_geom::bbox(ol).expect("outline");
        assert!(
            decal.min.x <= bb.min.x + 1e-6 && decal.min.x + decal.size.x >= bb.max.x - 1.0,
            "covers the piece"
        );
        assert_ne!(decal_key(&p, &id), {
            let mut q = p.clone();
            q.fabrics
                .get_mut(&fid)
                .expect("fabric")
                .extra
                .remove(azam_doc::FABRIC_TEXTURE_KEY);
            decal_key(&q, &id)
        });
    }

    #[test]
    fn a_graphic_without_an_image_prints_a_placeholder_where_it_sits() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (300.0, 0.0), (300.0, 300.0), (0.0, 300.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = azam_doc::create_piece(&mut p, &square).expect("piece");
        assert!(
            piece_decal(&p, &id, &mut ImageCache::default()).is_none(),
            "nothing printed, no image"
        );
        azam_doc::add_graphic(&mut p, &id, Vec2::new(150.0, 100.0), Vec2::new(80.0, 60.0), None)
            .expect("graphic");
        let decal = piece_decal(&p, &id, &mut ImageCache::default()).expect("decal");
        let size = azam_doc::graphic_size(&p.pieces[&id].graphics[0]);
        assert!((decal.size.x - size.x).abs() <= 0.25 && (decal.size.y - size.y).abs() <= 0.25);
        assert!((decal.min.x - (150.0 - size.x / 2.0)).abs() < 1e-9);
        let centre = ((decal.image.height / 2 * decal.image.width + decal.image.width / 2) * 4) as usize;
        assert_eq!(&decal.image.data[centre..centre + 4], &PLACEHOLDER);
    }

    #[test]
    fn a_topstitch_prints_dashed_thread_along_its_edge() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (300.0, 0.0), (300.0, 300.0), (0.0, 300.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = azam_doc::create_piece(&mut p, &square).expect("piece");
        let top = p.pieces[&id].pts[0].id.clone();
        azam_doc::add_stitch(&mut p, &id, Some(&top)).expect("stitch");
        let decal = piece_decal(&p, &id, &mut ImageCache::default()).expect("decal");
        assert!(decal.size.x > 290.0, "the image spans the stitched edge");
        // walk the image row through the thread: dashes alternate with gaps
        let img = &decal.image;
        let row = (0..img.height)
            .max_by_key(|&y| {
                (0..img.width)
                    .filter(|&x| img.data[((y * img.width + x) * 4 + 3) as usize] > 128)
                    .count()
            })
            .expect("rows");
        let inked: Vec<bool> = (0..img.width)
            .map(|x| img.data[((row * img.width + x) * 4 + 3) as usize] > 128)
            .collect();
        let switches = inked.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(switches > 20, "dashes along 300 mm: {switches} switches");
        assert!(
            inked.iter().filter(|&&i| i).count() * 10 > inked.len() * 3,
            "a good share is thread"
        );
    }

    #[test]
    fn a_zipper_prints_tape_under_its_teeth() {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let square =
            [(0.0, 0.0), (300.0, 0.0), (300.0, 300.0), (0.0, 300.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = azam_doc::create_piece(&mut p, &square).expect("piece");
        let left = p.pieces[&id].pts[3].id.clone();
        azam_doc::add_zipper_half(&mut p, &id, &left).expect("zipper");
        let decal = piece_decal(&p, &id, &mut ImageCache::default()).expect("decal");
        let img = &decal.image;
        let tape = crate::scene::parse_hex(azam_doc::ZIPPER_TAPE_COLOR).expect("tape colour");
        let texel = |x: u32, y: u32| {
            let at = ((y * img.width + x) * 4) as usize;
            u32::from(img.data[at]) << 16 | u32::from(img.data[at + 1]) << 8 | u32::from(img.data[at + 2])
        };
        // across the middle of the zipped edge: tape at the outer side of the band, teeth or tape inside
        let y = img.height / 2;
        let colours: Vec<u32> = (0..img.width).map(|x| texel(x, y)).collect();
        assert!(colours.contains(&tape), "the tape shows");
        let teeth = crate::scene::parse_hex(azam_doc::ZIPPER_TEETH_COLOR).expect("teeth colour");
        let any_teeth = (0..img.height).any(|y| (0..img.width).any(|x| texel(x, y) == teeth));
        assert!(any_teeth, "the teeth show");
    }
}
