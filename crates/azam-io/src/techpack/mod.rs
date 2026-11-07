//! Tech pack: bill of materials, trims, pattern list, points of measure and sewing, as tables and
//! CSV. The PDF version is [`crate::techpack_pdf`].

mod csv;
mod data;

pub use csv::{csv_from, techpack_csv};
pub(crate) use data::side_names;
pub use data::{FabricRow, PatternRow, PomRow, SewingRow, TechData, TrimRow, tech_data};
