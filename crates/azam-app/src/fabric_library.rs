//! The fabric library: fabrics saved outside any project, as JSON files in the user's data
//! folder, so any project can take them in.
//!
//! The folder is `$AZAM_LIBRARY/fabrics` when that variable is set, else
//! `~/Library/Application Support/azam/fabrics` on macOS, `%APPDATA%\azam\fabrics` on Windows
//! and `$XDG_DATA_HOME/azam/fabrics` (or `~/.local/share/azam/fabrics`) elsewhere.

use std::io;
use std::path::{Path, PathBuf};

use azam_doc::Fabric;

/// Variable naming a library folder to use instead of the platform's.
pub const LIBRARY_ENV: &str = "AZAM_LIBRARY";

/// The library's fabrics folder, if a home or data folder is known.
#[must_use]
pub fn fabrics_dir() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let root = if let Some(dir) = var(LIBRARY_ENV) {
        dir
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/azam")
    } else if cfg!(windows) {
        var("APPDATA")?.join("azam")
    } else {
        var("XDG_DATA_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".local/share")))?
            .join("azam")
    };
    Some(root.join("fabrics"))
}

/// A file name for `name`: letters, digits, spaces, `-` and `_` kept, the rest dropped.
fn file_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let kept = kept.trim();
    format!("{}.json", if kept.is_empty() { "Fabric" } else { kept })
}

/// Saves `fabric` into the library folder `dir` (replacing a saved fabric of the same name) and
/// returns its file.
///
/// # Errors
/// Fails if the folder cannot be created or the file written.
pub fn save(dir: &Path, fabric: &Fabric) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file_name(&fabric.name));
    let json = serde_json::to_string_pretty(fabric).map_err(io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// The fabrics saved in the library folder `dir`, by name. Files that are not fabrics are
/// skipped; a missing folder is an empty library.
#[must_use]
pub fn list(dir: &Path) -> Vec<Fabric> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut fabrics: Vec<Fabric> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok())
        .collect();
    fabrics.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    fabrics
}

#[cfg(test)]
mod tests {
    use super::*;
    use azam_doc::{Project, Sex};

    #[test]
    fn saved_fabrics_come_back_by_name() {
        let dir = std::env::temp_dir().join(format!("azam-fabrics-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list(&dir).is_empty(), "no folder, no fabrics");
        let p = Project::new(None, Sex::Female, 0.0);
        let mut fabric = p.fabrics.values().next().cloned().expect("a fabric");
        fabric.name = "Wool / Crepe?".into();
        let path = save(&dir, &fabric).expect("saved");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("Wool  Crepe.json")
        );
        std::fs::write(dir.join("notes.json"), "not a fabric").expect("write");
        let back = list(&dir);
        assert_eq!(back.len(), 1, "files that are not fabrics are skipped");
        assert_eq!(back[0].phys, fabric.phys);
        assert_eq!(back[0].name, fabric.name);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
