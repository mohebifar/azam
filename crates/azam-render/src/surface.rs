//! Surface relief of a fabric: a tangent-space normal map and a roughness map, packed into one
//! tiling texture (normal in RGB, roughness in A) that the viewport and the path tracer both read.

/// A fabric's normal and roughness maps, packed. Texel values are linear data, not colours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceMap {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// RGBA bytes, row by row, top row first: RGB the normal (OpenGL convention, 128 = 0), A the
    /// roughness (255 = matte).
    pub texels: Vec<u8>,
    /// Whether the RGB channels hold a normal map (else they are flat).
    pub has_normal: bool,
    /// Whether the A channel holds a roughness map (else the material's roughness applies).
    pub has_roughness: bool,
}

/// A flat normal, as a texel.
const FLAT: [u8; 3] = [128, 128, 255];

impl SurfaceMap {
    /// Packs `normal` and `roughness` (each `(width, height, RGBA bytes)`; roughness is read from
    /// its red channel). The roughness map is resampled to the normal map's size when both are
    /// given. `None` when neither is given or an image's byte count does not match its size.
    #[must_use]
    pub fn pack(normal: Option<(u32, u32, &[u8])>, roughness: Option<(u32, u32, &[u8])>) -> Option<Self> {
        let valid = |&(w, h, px): &(u32, u32, &[u8])| w > 0 && h > 0 && px.len() == (w * h * 4) as usize;
        let (normal, roughness) = (normal.filter(valid), roughness.filter(valid));
        let (width, height) = normal.or(roughness).map(|(w, h, _)| (w, h))?;
        let mut texels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let n = normal.map_or(FLAT, |(_, _, px)| {
                    let at = ((y * width + x) * 4) as usize;
                    [px[at], px[at + 1], px[at + 2]]
                });
                let r = roughness.map_or(255, |(rw, rh, px)| {
                    // nearest texel of the roughness map at the same spot of the tile
                    let rx = (u64::from(x) * u64::from(rw) / u64::from(width)) as u32;
                    let ry = (u64::from(y) * u64::from(rh) / u64::from(height)) as u32;
                    px[((ry * rw + rx) * 4) as usize]
                });
                texels.extend_from_slice(&[n[0], n[1], n[2], r]);
            }
        }
        Some(Self {
            width,
            height,
            texels,
            has_normal: normal.is_some(),
            has_roughness: roughness.is_some(),
        })
    }

    /// The tangent-space normal (unit, z out of the cloth) and roughness (0..1) at tile
    /// coordinates `(s, t)` (1 = one tile; the map repeats), bilinearly filtered.
    #[must_use]
    pub fn sample(&self, s: f32, t: f32) -> ([f32; 3], f32) {
        let (w, h) = (self.width as f32, self.height as f32);
        let (fx, fy) = (s.rem_euclid(1.0) * w - 0.5, t.rem_euclid(1.0) * h - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (ax, ay) = (fx - x0, fy - y0);
        let texel = |x: f32, y: f32| {
            let xi = (x as i64).rem_euclid(i64::from(self.width)) as u32;
            let yi = (y as i64).rem_euclid(i64::from(self.height)) as u32;
            let at = ((yi * self.width + xi) * 4) as usize;
            [0, 1, 2, 3].map(|c| f32::from(self.texels[at + c]) / 255.0)
        };
        let (a, b, c, d) = (
            texel(x0, y0),
            texel(x0 + 1.0, y0),
            texel(x0, y0 + 1.0),
            texel(x0 + 1.0, y0 + 1.0),
        );
        let mix = |k: usize| {
            let top = a[k] + (b[k] - a[k]) * ax;
            let bottom = c[k] + (d[k] - c[k]) * ax;
            top + (bottom - top) * ay
        };
        let n = [mix(0) * 2.0 - 1.0, mix(1) * 2.0 - 1.0, mix(2) * 2.0 - 1.0];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if len > 1e-6 {
            n.map(|v| v / len)
        } else {
            [0.0, 0.0, 1.0]
        };
        (n, mix(3))
    }

    /// The map and its mip levels down to 1 × 1, each a 2 × 2 box average of the one before
    /// (plain averages: the data is linear).
    #[must_use]
    pub fn mips(&self) -> Vec<(u32, u32, Vec<u8>)> {
        let mut levels = vec![(self.width, self.height, self.texels.clone())];
        while let Some((w, h, data)) = levels.last()
            && (*w > 1 || *h > 1)
        {
            let (w, h) = (*w, *h);
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let mut sum = 0u32;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                            sum += u32::from(data[((sy * w + sx) * 4 + c) as usize]);
                        }
                        next[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                    }
                }
            }
            levels.push((nw, nh, next));
        }
        levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_fills_in_what_is_missing() {
        // a 2 × 1 normal map leaning +x on the left, -x on the right
        let normal = [218, 128, 218, 255, 38, 128, 218, 255];
        let rough = [40, 40, 40, 255];
        let m = SurfaceMap::pack(Some((2, 1, &normal)), Some((1, 1, &rough))).expect("packed");
        assert_eq!((m.width, m.height), (2, 1));
        assert_eq!(&m.texels, &[218, 128, 218, 40, 38, 128, 218, 40]);
        assert!(m.has_normal && m.has_roughness);
        let only_rough = SurfaceMap::pack(None, Some((1, 1, &rough))).expect("packed");
        assert_eq!(&only_rough.texels, &[128, 128, 255, 40]);
        assert!(!only_rough.has_normal);
        assert!(SurfaceMap::pack(None, None).is_none());
        assert!(
            SurfaceMap::pack(Some((2, 2, &normal)), None).is_none(),
            "wrong byte count"
        );
    }

    #[test]
    fn samples_are_unit_normals_and_wrap_around_the_tile() {
        let normal = [218, 128, 218, 255, 38, 128, 218, 255];
        let m = SurfaceMap::pack(Some((2, 1, &normal)), None).expect("packed");
        let (left, rough) = m.sample(0.25, 0.5);
        assert!(left[0] > 0.5 && (left[0].powi(2) + left[1].powi(2) + left[2].powi(2) - 1.0).abs() < 1e-5);
        assert!((rough - 1.0).abs() < 1e-6);
        let (right, _) = m.sample(0.75, 0.5);
        assert!(right[0] < -0.5);
        assert_eq!(m.sample(1.25, 0.5), m.sample(0.25, 0.5), "repeats");
        // at the seam between tiles the two texels blend: x leans cancel
        let (seam, _) = m.sample(0.0, 0.5);
        assert!(seam[0].abs() < 0.05, "{seam:?}");
    }

    #[test]
    fn mips_halve_down_to_one_texel() {
        let m = SurfaceMap::pack(None, Some((4, 2, &[200; 32]))).expect("packed");
        let levels = m.mips();
        let sizes: Vec<(u32, u32)> = levels.iter().map(|(w, h, _)| (*w, *h)).collect();
        assert_eq!(sizes, vec![(4, 2), (2, 1), (1, 1)]);
        assert_eq!(&levels[2].2, &[128, 128, 255, 200]);
    }
}
