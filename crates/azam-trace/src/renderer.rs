//! The progressive renderer: accumulates samples over tiles in parallel and develops the result
//! into an 8-bit image.

use std::io;
use std::path::Path;

use glam::{Vec2, Vec3};
use rayon::prelude::*;

use crate::denoise::{Guides, denoise};
use crate::integrator::{PathParams, Prepared};
use crate::tonemap::{aces_filmic, linear_to_srgb};
use crate::{Camera, Rng, Scene};

/// Render settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Image width, in pixels.
    pub width: u32,
    /// Image height, in pixels.
    pub height: u32,
    /// Samples per pixel to accumulate.
    pub samples: u32,
    /// Most bounces a path makes after the first hit (Russian roulette may end it sooner).
    pub max_bounces: u32,
    /// Seed of every random number.
    pub seed: u64,
    /// Whether [`Renderer::image`] runs the denoiser.
    pub denoise: bool,
    /// Exposure multiplier applied before the tone curve.
    pub exposure: f32,
    /// Largest luminance one indirect contribution may add (suppresses fireflies).
    pub clamp: f32,
    /// Side of the square tiles rendered in parallel, in pixels.
    pub tile: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 800,
            samples: 64,
            max_bounces: 4,
            seed: 0,
            denoise: true,
            exposure: 1.0,
            clamp: 10.0,
            tile: 32,
        }
    }
}

/// An 8-bit sRGB image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Width, in pixels.
    pub width: u32,
    /// Height, in pixels.
    pub height: u32,
    /// RGBA bytes, row by row from the top-left.
    pub rgba: Vec<u8>,
}

impl Image {
    /// Writes the image as a PNG file.
    ///
    /// # Errors
    /// Fails if the file cannot be written.
    pub fn write_png(&self, path: &Path) -> io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut encoder = png::Encoder::new(io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(&self.rgba))
            .map_err(io::Error::other)
    }

    /// The RGBA of pixel `x`, `y`.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba
            .get(i..i + 4)
            .and_then(|s| s.try_into().ok())
            .unwrap_or([0; 4])
    }
}

/// A rectangle of pixels.
#[derive(Clone, Copy, Debug)]
struct Tile {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Sums of one tile's samples, per pixel.
struct TileSums {
    tile: Tile,
    color: Vec<Vec3>,
    lum2: Vec<f32>,
    albedo: Vec<Vec3>,
    normal: Vec<Vec3>,
    depth: Vec<f32>,
}

/// A progressive path-traced render of one scene from one camera.
pub struct Renderer {
    scene: Prepared,
    camera: Camera,
    settings: Settings,
    tiles: Vec<Tile>,
    color: Vec<Vec3>,
    lum2: Vec<f32>,
    albedo: Vec<Vec3>,
    normal: Vec<Vec3>,
    depth: Vec<f32>,
    samples_done: u32,
}

impl Renderer {
    /// Prepares `scene` (building its hierarchy) for rendering from `camera` with `settings`.
    #[must_use]
    pub fn new(scene: &Scene, camera: Camera, settings: Settings) -> Self {
        let settings = Settings {
            width: settings.width.max(1),
            height: settings.height.max(1),
            tile: settings.tile.max(4),
            ..settings
        };
        let (w, h, ts) = (settings.width, settings.height, settings.tile);
        let mut tiles = Vec::new();
        for y in (0..h).step_by(ts as usize) {
            for x in (0..w).step_by(ts as usize) {
                tiles.push(Tile {
                    x,
                    y,
                    w: ts.min(w - x),
                    h: ts.min(h - y),
                });
            }
        }
        let n = w as usize * h as usize;
        Self {
            scene: Prepared::new(scene),
            camera,
            settings,
            tiles,
            color: vec![Vec3::ZERO; n],
            lum2: vec![0.0; n],
            albedo: vec![Vec3::ZERO; n],
            normal: vec![Vec3::ZERO; n],
            depth: vec![0.0; n],
            samples_done: 0,
        }
    }

    /// The settings in use.
    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Samples per pixel accumulated so far.
    #[must_use]
    pub const fn samples_done(&self) -> u32 {
        self.samples_done
    }

    /// Whether every requested sample has been taken.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.samples_done >= self.settings.samples
    }

    /// Number of triangles in the scene's hierarchy.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.scene.triangle_count()
    }

    /// Takes up to `samples` more samples per pixel (never beyond [`Settings::samples`]); returns
    /// how many were taken.
    pub fn step(&mut self, samples: u32) -> u32 {
        let n = samples.min(self.settings.samples.saturating_sub(self.samples_done));
        if n == 0 {
            return 0;
        }
        let first = self.samples_done;
        let results: Vec<TileSums> = self
            .tiles
            .par_iter()
            .map(|&tile| self.render_tile(tile, first, n))
            .collect();
        let w = self.settings.width as usize;
        for r in results {
            for ty in 0..r.tile.h as usize {
                for tx in 0..r.tile.w as usize {
                    let i = (r.tile.y as usize + ty) * w + r.tile.x as usize + tx;
                    let j = ty * r.tile.w as usize + tx;
                    self.color[i] += r.color[j];
                    self.lum2[i] += r.lum2[j];
                    self.albedo[i] += r.albedo[j];
                    self.normal[i] += r.normal[j];
                    self.depth[i] += r.depth[j];
                }
            }
        }
        self.samples_done += n;
        n
    }

    /// Takes every remaining sample and returns the developed image.
    #[must_use]
    pub fn render(mut self) -> Image {
        while !self.is_done() {
            self.step(self.settings.samples);
        }
        self.image()
    }

    fn render_tile(&self, tile: Tile, first: u32, count: u32) -> TileSums {
        let s = &self.settings;
        let n = tile.w as usize * tile.h as usize;
        let mut out = TileSums {
            tile,
            color: vec![Vec3::ZERO; n],
            lum2: vec![0.0; n],
            albedo: vec![Vec3::ZERO; n],
            normal: vec![Vec3::ZERO; n],
            depth: vec![0.0; n],
        };
        let pixel_mm = self.camera.footprint(0.0, s.height);
        let params = PathParams {
            max_bounces: s.max_bounces,
            clamp: s.clamp,
            pixel_angle: self.camera.footprint(1.0, s.height) - pixel_mm,
            pixel_mm,
        };
        let (w, h) = (s.width as f32, s.height as f32);
        for ty in 0..tile.h {
            for tx in 0..tile.w {
                let (px, py) = (tile.x + tx, tile.y + ty);
                let pixel = u64::from(py) * u64::from(s.width) + u64::from(px);
                let j = (ty * tile.w + tx) as usize;
                for sample in first..first + count {
                    let mut rng = Rng::for_sample(s.seed, pixel, u64::from(sample));
                    let (jx, jy) = rng.next_2d();
                    let screen = Vec2::new((px as f32 + jx) / w, (py as f32 + jy) / h);
                    let ray = self.camera.ray(screen.x, screen.y);
                    let r = self.scene.trace(ray, screen, &params, &mut rng);
                    out.color[j] += r.radiance;
                    let l = crate::bsdf::luminance(r.radiance);
                    out.lum2[j] += l * l;
                    out.albedo[j] += r.albedo;
                    out.normal[j] += r.normal;
                    out.depth[j] += r.depth;
                }
            }
        }
        out
    }

    /// The mean linear radiance at pixel `x`, `y` so far (black before the first sample).
    #[must_use]
    pub fn radiance(&self, x: u32, y: u32) -> [f32; 3] {
        let i = y as usize * self.settings.width as usize + x as usize;
        match self.color.get(i) {
            Some(c) if self.samples_done > 0 => (*c / self.samples_done as f32).into(),
            _ => [0.0; 3],
        }
    }

    /// The mean linear radiance of every pixel, denoised or not.
    #[must_use]
    pub fn radiance_buffer(&self, denoised: bool) -> Vec<[f32; 3]> {
        let n = self.samples_done.max(1) as f32;
        let mean: Vec<Vec3> = self.color.iter().map(|&c| c / n).collect();
        if !denoised || self.samples_done == 0 {
            return mean.into_iter().map(Into::into).collect();
        }
        let albedo: Vec<Vec3> = self.albedo.iter().map(|&a| a / n).collect();
        let normal: Vec<Vec3> = self.normal.iter().map(|&v| v.normalize_or_zero()).collect();
        let depth: Vec<f32> = self.depth.iter().map(|&d| d / n).collect();
        let variance: Vec<f32> = mean
            .iter()
            .zip(&self.lum2)
            .map(|(&m, &l2)| {
                let l = crate::bsdf::luminance(m);
                // Variance of one sample, then of the mean of n.
                let v = (l2 / n - l * l).max(0.0);
                if self.samples_done > 1 { v / (n - 1.0) } else { v }
            })
            .collect();
        denoise(&Guides {
            width: self.settings.width as usize,
            height: self.settings.height as usize,
            color: &mean,
            albedo: &albedo,
            normal: &normal,
            depth: &depth,
            variance: &variance,
        })
        .into_iter()
        .map(Into::into)
        .collect()
    }

    /// The render so far, tone mapped to 8-bit sRGB (denoised if [`Settings::denoise`]).
    #[must_use]
    pub fn image(&self) -> Image {
        self.image_with(self.settings.denoise)
    }

    /// The render so far, tone mapped to 8-bit sRGB, denoised or not.
    #[must_use]
    pub fn image_with(&self, denoised: bool) -> Image {
        let exposure = self.settings.exposure;
        let seed = self.settings.seed;
        let rgba = self
            .radiance_buffer(denoised)
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, c)| {
                let m = aces_filmic([c[0] * exposure, c[1] * exposure, c[2] * exposure]);
                // Triangular dither of ±1 code value hides banding in the smooth backdrop.
                let mut rng = Rng::for_sample(seed ^ 0xd1_7e_4e, i as u64, 0);
                let dither = rng.next_f32() - rng.next_f32();
                let byte = |v: f32| (linear_to_srgb(v) * 255.0 + 0.5 + dither).clamp(0.0, 255.0) as u8;
                [byte(m[0]), byte(m[1]), byte(m[2]), 255]
            })
            .collect();
        Image {
            width: self.settings.width,
            height: self.settings.height,
            rgba,
        }
    }
}
