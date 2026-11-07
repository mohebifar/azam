//! 2D pattern geometry for azam.
//!
//! Units are millimetres. The 2D frame has x to the right and y down, as in the pattern window.
//!
//! A pattern piece is a closed list of [`OutlinePoint`]s. Corner points split the outline into
//! segments; curve points between two corners are the Bézier control points of that segment, as in
//! CLO. [`Outline`] flattens the segments into polylines and answers arc-length queries.
//!
//! Golden tests in `tests/` check the results against recorded reference data for every garment
//! template.

mod allowance;
mod curve;
mod grading;
mod numeric;
mod open_line;
mod outline;
mod point;
mod polygon;
mod side;

pub use allowance::{EdgeAllowances, EdgeFinish, cut_outline, offset_outline};
pub use curve::{PolyPoint, bezier_point, cumulative_lengths, flatten_bezier, point_at_length};
pub use grading::{GradeDelta, GradeRules, SizeName, graded_points};
pub use numeric::hypot;
pub use open_line::flatten_open_line;
pub use outline::{NearestPoint, Outline, PerimeterPoint, Segment};
pub use point::{OutlinePoint, PointId, Vec2};
pub use polygon::{BBox, SegmentDistance, bbox, contains, distance_to_segment, signed_area};
pub use side::{DEFAULT_SIDE_STEP_MM, SewingSide, SideDirection, SideEnd, SideRange};
