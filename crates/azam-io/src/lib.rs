//! Import, export and tech packs for azam projects.
//!
//! Every function is pure: it reads a [`Project`](azam_doc::Project) (or plain mesh buffers) and
//! returns text or bytes for the caller to save.
//!
//! - [`export_dxf`] / [`import_dxf`]: AAMA-style DXF pattern blocks.
//! - [`export_svg`] / [`import_svg`]: the pattern at 1:1; pieces from a drawing's closed shapes.
//! - [`export_obj`]: the draped garment as Wavefront OBJ with a material library.
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
pub mod format;
mod model;
mod obj;
pub mod pdf;
mod svg;
mod techpack;

pub use dxf::{DxfError, ImportedLine, ImportedPiece, export_dxf, import_dxf};
pub use model::Bounds;
pub use obj::{MTL_FILE_NAME, ObjExport, ObjMaterial, ObjMesh, ObjPiece, export_obj};
pub use pdf::pattern::{Layout, Paper, PatternPdfError, PatternPdfOptions, pattern_pdf, pattern_pdf_pages};
pub use pdf::techpack::{TechPackInfo, techpack_pdf, techpack_pdf_pages};
pub use svg::{SvgError, export_svg, import_svg};
pub use techpack::{
    FabricRow, PatternRow, PomRow, SewingRow, TechData, TrimRow, csv_from, tech_data, techpack_csv,
};
