//! Import, export and tech packs for azam projects.
//!
//! Every function is pure: it reads a [`Project`](azam_doc::Project) (or plain mesh buffers) and
//! returns text or bytes for the caller to save.
//!
//! - [`export_dxf`] / [`import_dxf`]: AAMA-style DXF pattern blocks.
//! - [`export_svg`] / [`import_svg`]: the pattern at 1:1; pieces from a drawing's closed shapes.
//! - [`export_obj`] / [`import_obj`]: the draped garment as Wavefront OBJ with a material library;
//!   a mesh (an avatar body) from an OBJ file.
//! - [`tech_data`] / [`techpack_csv`] / [`techpack_pdf`]: bill of materials, points of measure,
//!   pattern list and sewing, as tables, CSV and PDF.
//! - [`dimensions`]: net and cut sizes and the CAD dimension model of a piece.
//! - [`pattern_pdf`]: print-ready pattern (full-scale tiles, plotter sheets, CAD sheets per piece
//!   or an overview) with seam allowance, notches, grain lines, labels and dimensions.
//!
//! Units are millimetres. Golden tests in `tests/` check the output against recorded reference
//! data for every garment template.

pub mod dimensions;
mod dxf;
mod fbx;
pub mod format;
mod gltf;
mod model;
mod obj;
pub mod pdf;
mod svg;
mod techpack;

pub use dxf::{
    AstmFiles, DxfError, GradeRuleTable, ImportedLine, ImportedPiece, RulError, export_astm, export_dxf,
    export_dxf_graded, import_dxf, import_rul,
};
pub use fbx::export_fbx;
pub use gltf::{Animation, MeshFrame, export_glb, export_glb_animated};
pub use model::Bounds;
pub use obj::{
    MTL_FILE_NAME, ObjExport, ObjImport, ObjImportError, ObjMaterial, ObjMesh, ObjPiece, export_obj,
    import_obj,
};
pub use pdf::pattern::{Layout, Paper, PatternPdfError, PatternPdfOptions, pattern_pdf, pattern_pdf_pages};
pub use pdf::techpack::{TechPackInfo, techpack_pdf, techpack_pdf_pages};
pub use svg::{SvgError, export_svg, import_svg};
pub use techpack::{
    FabricRow, PatternRow, PomRow, SewingRow, TechData, TrimRow, csv_from, tech_data, techpack_csv,
};
