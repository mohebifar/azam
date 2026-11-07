//! Finding the morph-target avatar pack shipped with azam.
//!
//! The pack is built from the CC0 assets of `MakeHuman` by `tools/mhpack` and lives at
//! `assets/makehuman/makehuman-cc0.bin` in the repository. Nothing is downloaded at run time.

use std::path::{Path, PathBuf};

use super::parse::{MhModel, MhParseError};

/// File name of the bundled pack.
pub const PACK_FILE: &str = "makehuman-cc0.bin";
/// Directory of the bundled pack, relative to the repository (or installation) root.
pub const PACK_DIR: &str = "assets/makehuman";
/// Environment variable that, when set, names the pack file to load instead.
pub const PACK_ENV: &str = "AZAM_MAKEHUMAN_PACK";
/// How many parent directories of the executable are searched for [`PACK_DIR`].
const EXE_ANCESTORS: usize = 5;

/// Places the pack is looked for, in order: the [`PACK_ENV`] variable, [`PACK_DIR`] under the
/// working directory, under the executable's directory and its parents, and under the source
/// tree this crate was built from.
#[must_use]
pub fn pack_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os(PACK_ENV) {
        out.push(PathBuf::from(p));
    }
    let rel = Path::new(PACK_DIR).join(PACK_FILE);
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(&rel));
    }
    if let Ok(exe) = std::env::current_exe() {
        // a macOS app bundle keeps resources in Contents/Resources, beside Contents/MacOS
        if let Some(contents) = exe.parent().and_then(Path::parent) {
            out.push(contents.join("Resources").join(&rel));
        }
        out.extend(exe.ancestors().skip(1).take(EXE_ANCESTORS).map(|d| d.join(&rel)));
    }
    out.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(&rel));
    out
}

/// The first existing pack file among [`pack_candidates`].
#[must_use]
pub fn find_pack() -> Option<PathBuf> {
    pack_candidates().into_iter().find(|p| p.is_file())
}

impl MhModel {
    /// Reads and parses a pack file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is not a valid pack.
    pub fn load(path: &Path) -> Result<Self, MhParseError> {
        let bytes = std::fs::read(path).map_err(|e| MhParseError::Io(format!("{}: {e}", path.display())))?;
        Self::parse(&bytes)
    }

    /// Loads the bundled pack ([`find_pack`]); `None` if no pack file exists.
    #[must_use]
    pub fn load_bundled() -> Option<Result<Self, MhParseError>> {
        find_pack().map(|p| Self::load(&p))
    }
}
