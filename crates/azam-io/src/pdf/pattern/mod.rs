//! The pattern PDF: pieces at full scale on tiles or plotter sheets, one CAD sheet per piece, or
//! all pieces on one overview sheet; with seam allowance bands, cut and sewing lines, notches,
//! grain lines, labels, seam numbers and optional CAD dimensions, followed by piece and sewing
//! schedules and dimension tables.

mod dims;
mod layouts;
mod piece;
mod plan;
mod render;
mod schedules;
mod sheets;

use azam_doc::{PieceId, Project, Units};

pub use plan::{
    DIM_FULL, DIM_ROOM, DimBlock, NestOutline, Pack, PackRow, PdfItem, PdfPlan, PieceFit, Placement,
    PlanSheets, PlotterPlan, Sheet, TiledPlan, dim_table_pages, pack, pdf_items, piece_fit, plan,
};

use super::canvas::Page;
use super::writer::{PdfInfo, write_pdf};

/// Paper size of tiles and sheets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Paper {
    /// ISO A4, 210 × 297 mm.
    #[default]
    A4,
    /// ISO A3, 297 × 420 mm.
    A3,
    /// US Letter, 8.5 × 11 in.
    Letter,
    /// US Tabloid, 11 × 17 in.
    Tabloid,
}

impl Paper {
    /// Short and long side in mm.
    #[must_use]
    pub fn size(self) -> [f64; 2] {
        match self {
            Self::A4 => super::A4_MM,
            Self::A3 => [297.0, 420.0],
            Self::Letter => [215.9, 279.4],
            Self::Tabloid => [279.4, 431.8],
        }
    }

    /// Display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::Letter => "Letter",
            Self::Tabloid => "Tabloid",
        }
    }
}

/// Page layout of the pattern PDF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Layout {
    /// Full scale on paper tiles to tape together, with a cover and assembly map.
    #[default]
    Tiled,
    /// One CAD-style sheet per piece, scaled to fit, with a title block.
    Pieces,
    /// All pieces on one landscape sheet at a common scale.
    Overview,
    /// Full scale on sheets as wide as a plotter roll.
    Plotter,
}

/// Plotter roll widths offered, in mm (36″, 44″, 60″).
pub const PLOTTER_WIDTHS_MM: [f64; 3] = [914.0, 1118.0, 1524.0];

/// Options of the pattern PDF.
#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches of the export dialog"
)]
pub struct PatternPdfOptions {
    /// Page layout.
    pub layout: Layout,
    /// Paper of tiles and schedule pages.
    pub paper: Paper,
    /// Units of printed values.
    pub units: Units,
    /// Draw the seam allowance (cut line outside the sewing line, tinted bands).
    pub sa: bool,
    /// Print the length of every segment.
    pub lengths: bool,
    /// Full CAD dimensions and dimension tables.
    pub dims: bool,
    /// Nest every size of the size range in each piece.
    pub all_sizes: bool,
    /// Plotter roll width, mm.
    pub plotter_width_mm: f64,
    /// Pieces to print (empty: every piece taking part in the simulation).
    pub piece_ids: Vec<PieceId>,
    /// Date printed in page headers.
    pub date: String,
}

impl Default for PatternPdfOptions {
    fn default() -> Self {
        Self {
            layout: Layout::Tiled,
            paper: Paper::A4,
            units: Units::Cm,
            sa: true,
            lengths: false,
            dims: false,
            all_sizes: false,
            plotter_width_mm: PLOTTER_WIDTHS_MM[0],
            piece_ids: Vec::new(),
            date: String::new(),
        }
    }
}

/// Why a pattern PDF could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternPdfError {
    /// No piece qualifies for printing.
    NoPieces,
}

impl std::fmt::Display for PatternPdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPieces => f.write_str("no pieces to export"),
        }
    }
}

impl std::error::Error for PatternPdfError {}

/// The pages of the pattern PDF as drawing operations.
///
/// # Errors
/// [`PatternPdfError::NoPieces`] when no piece qualifies.
pub fn pattern_pdf_pages(
    project: &Project,
    options: &PatternPdfOptions,
) -> Result<Vec<Page>, PatternPdfError> {
    let plan = plan::plan(project, options);
    if plan.items.is_empty() {
        return Err(PatternPdfError::NoPieces);
    }
    let mut r = render::Renderer::new(project, options, plan.pages);
    r.render(&plan);
    Ok(r.canvas.into_pages())
}

/// The pattern PDF file.
///
/// # Errors
/// [`PatternPdfError::NoPieces`] when no piece qualifies.
pub fn pattern_pdf(project: &Project, options: &PatternPdfOptions) -> Result<Vec<u8>, PatternPdfError> {
    let pages = pattern_pdf_pages(project, options)?;
    Ok(write_pdf(
        &pages,
        &PdfInfo {
            title: format!("{} — pattern", project.meta.name),
        },
    ))
}
