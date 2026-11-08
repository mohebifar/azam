//! Pattern edits: the operations commands are built from.

mod dart;
mod grading;
mod library;
mod pieces;
mod refs;
mod segment;
mod shapes;
mod transform;

pub use dart::{DEFAULT_DART_DEPTH_MM, DEFAULT_DART_WIDTH_MM, DartError, add_dart};
pub use grading::{grade_points_linear, grade_step, set_base_size, set_grade_step_axis, set_sizes};
pub use library::{add_fabric, delete_colorway, delete_fabric, duplicate_colorway, duplicate_fabric};
pub use pieces::{
    DeletePointsError, DuplicateMode, MIN_CORNERS, add_internal_line, create_piece, delete_pieces,
    delete_points, duplicate_piece,
};
pub use refs::remap_refs;
pub use segment::{add_curve_point, de_casteljau_split, seg_bez_param, set_curvature, split_segment};
pub use shapes::{
    ELLIPSE_KAPPA, MIN_SHAPE_MM, Shape, add_internal_ellipse, create_shape_piece, delete_internal_line,
    internal_fold, set_internal_closed, set_internal_fold, set_point_curve, translate_internal_line,
};
pub use transform::{
    FlipAxis, PieceTransform, RigidMap, flip_piece, rigid_map, rotate_piece_90, transform_piece,
};
