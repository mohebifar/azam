//! PNG encoding and base64, for the images tools return.

/// Base64 alphabet (RFC 4648, with padding).
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` in base64.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * k)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `rgba` (`width` x `height`, top row first) as a PNG file.
///
/// # Errors
/// Fails when the pixel count does not match the size.
pub fn png_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(rgba))
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Window pixels (`0x00RRGGBB`) as opaque RGBA.
#[must_use]
pub fn xrgb_to_rgba(pixels: &[u32]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255])
        .collect()
}

/// `rgba` (`width` x `height`) shrunk by a whole factor (a box filter) until it is at most
/// `max_width` wide: a Retina window's pixels at about the size an agent looks at.
#[must_use]
pub fn shrink_rgba(width: u32, height: u32, rgba: &[u8], max_width: u32) -> (u32, u32, Vec<u8>) {
    let k = width.div_ceil(max_width.max(1)).max(1);
    if k == 1 {
        return (width, height, rgba.to_vec());
    }
    let (w, h) = (width / k, height / k);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            for dy in 0..k {
                for dx in 0..k {
                    let i = (((y * k + dy) * width + x * k + dx) * 4) as usize;
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u32::from(rgba[i + c]);
                    }
                }
            }
            out.extend(sum.map(|s| (s / (k * k)) as u8));
        }
    }
    (w, h, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648_examples() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn shrinking_averages_blocks() {
        let rgba = [0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
        assert_eq!(shrink_rgba(2, 2, &rgba, 1), (1, 1, vec![127, 127, 127, 255]));
        assert_eq!(shrink_rgba(2, 2, &rgba, 4).0, 2);
    }
}
