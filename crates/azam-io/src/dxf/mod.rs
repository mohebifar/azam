//! DXF import and export (AAMA-style pattern blocks, and ASTM D6673 with grade rules).

mod astm;
mod export;
mod import;

pub use astm::{AstmFiles, GradeRuleTable, RulError, export_astm, import_rul};
pub use export::{export_dxf, export_dxf_graded};
pub use import::{DxfError, ImportedLine, ImportedPiece, import_dxf};

/// Layer names of the pattern conventions.
mod layer {
    /// Layer of entities that name none.
    pub const DEFAULT: &str = "0";
    /// Cut line and annotation text.
    pub const CUT: &str = "1";
    /// Turn points of the boundary (ASTM export).
    pub const TURN: &str = "2";
    /// Curve points of the boundary (ASTM export).
    pub const CURVE: &str = "3";
    /// Notches.
    pub const NOTCH: &str = "4";
    /// Grain line.
    pub const GRAIN: &str = "7";
    /// Internal lines.
    pub const INTERNAL: &str = "8";
    /// Sewing line.
    pub const SEW: &str = "14";
}
