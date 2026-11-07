//! DXF import and export (AAMA-style pattern blocks).

mod export;
mod import;

pub use export::export_dxf;
pub use import::{DxfError, ImportedLine, ImportedPiece, import_dxf};

/// Layer names of the pattern conventions.
mod layer {
    /// Layer of entities that name none.
    pub const DEFAULT: &str = "0";
    /// Cut line and annotation text.
    pub const CUT: &str = "1";
    /// Notches.
    pub const NOTCH: &str = "4";
    /// Grain line.
    pub const GRAIN: &str = "7";
    /// Internal lines.
    pub const INTERNAL: &str = "8";
    /// Sewing line.
    pub const SEW: &str = "14";
}
