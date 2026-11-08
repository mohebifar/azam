//! A small ZIP reader: the archive's files by name, stored or deflated (what U3M archives and
//! most other exchange packages use). ZIP64 and encrypted archives are not read.

use std::collections::BTreeMap;

/// End of central directory record.
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
/// Central directory file header.
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
/// Local file header.
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
/// Fixed part of the end of central directory record, in bytes.
const EOCD_LEN: usize = 22;
/// Longest archive comment, which the end record may be followed by.
const MAX_COMMENT: usize = 0xffff;
/// Compression methods.
const STORED: u16 = 0;
const DEFLATED: u16 = 8;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The files in ZIP archive `bytes`, by their path in the archive (directories left out), or
/// `None` if it is not a ZIP archive this reader understands.
#[must_use]
pub fn read_zip(bytes: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    let lowest = bytes.len().saturating_sub(EOCD_LEN + MAX_COMMENT);
    let eocd = (lowest..=bytes.len().checked_sub(EOCD_LEN)?)
        .rev()
        .find(|&at| u32_at(bytes, at) == Some(EOCD_SIGNATURE))?;
    let count = usize::from(u16_at(bytes, eocd + 10)?);
    let mut at = u32_at(bytes, eocd + 16)? as usize;
    let mut files = BTreeMap::new();
    for _ in 0..count {
        if u32_at(bytes, at)? != CENTRAL_SIGNATURE {
            return None;
        }
        let method = u16_at(bytes, at + 10)?;
        let packed = u32_at(bytes, at + 20)? as usize;
        let size = u32_at(bytes, at + 24)? as usize;
        let name_len = usize::from(u16_at(bytes, at + 28)?);
        let extra_len = usize::from(u16_at(bytes, at + 30)?);
        let comment_len = usize::from(u16_at(bytes, at + 32)?);
        let local = u32_at(bytes, at + 42)? as usize;
        let name = String::from_utf8_lossy(bytes.get(at + 46..at + 46 + name_len)?).replace('\\', "/");
        at += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        if u32_at(bytes, local)? != LOCAL_SIGNATURE {
            return None;
        }
        let start =
            local + 30 + usize::from(u16_at(bytes, local + 26)?) + usize::from(u16_at(bytes, local + 28)?);
        let data = bytes.get(start..start + packed)?;
        let content = match method {
            STORED => data.to_vec(),
            DEFLATED => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size.max(1)).ok()?,
            _ => continue,
        };
        files.insert(name, content);
    }
    Some(files)
}

/// CRC-32 (IEEE) of `data`, as ZIP headers carry it.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A ZIP archive of `files` (path, contents), each deflated.
#[must_use]
pub fn write_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let crc = crc32(data);
        let local = out.len() as u32;
        let header = |sig: u32, out: &mut Vec<u8>| {
            out.extend_from_slice(&sig.to_le_bytes());
            if sig == CENTRAL_SIGNATURE {
                out.extend_from_slice(&20u16.to_le_bytes());
            }
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&DEFLATED.to_le_bytes());
            out.extend_from_slice(&[0; 4]); // time, date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
        };
        header(LOCAL_SIGNATURE, &mut out);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);
        header(CENTRAL_SIGNATURE, &mut central);
        central.extend_from_slice(&[0; 6]); // comment len, disk, internal attrs
        central.extend_from_slice(&[0; 4]); // external attrs
        central.extend_from_slice(&local.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_come_back_by_name() {
        let big = vec![7u8; 5000];
        let zip = write_zip(&[("a.txt", b"hello"), ("dir/b.bin", &big)]);
        let files = read_zip(&zip).expect("a zip");
        assert_eq!(files.len(), 2);
        assert_eq!(files["a.txt"], b"hello");
        assert_eq!(files["dir/b.bin"], big);
        assert!(read_zip(b"not a zip at all, sorry").is_none());
        // the check value of the CRC-32 standard
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
