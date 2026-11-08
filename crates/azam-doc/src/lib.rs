//! Project document model for azam.
//!
//! A [`Project`] holds everything about one garment: pattern pieces with their outlines, seams,
//! fabrics and colorways, topstitches, buttons, points of measure, sizes and avatar settings. It
//! reads and writes the stored project format: field names follow the file, and fields this
//! version does not interpret are kept in `extra` maps so a loaded file saves back unchanged.
//!
//! Every change is a [`Command`]. [`apply`] runs one atomically (a refused command leaves the
//! project untouched) and returns the previous state; [`History`] keeps those states for undo and
//! redo. The editing operations the commands are built from are public too.
//!
//! New ids come from the project's [`IdGen`], so the same edits always produce the same ids.
//! Garment [`Template`]s draft complete projects from the avatar's measurements.
//!
//! Units are millimetres, grams and seconds. The 2D frame has x to the right and y down.

mod avatar;
mod command;
mod edit;
mod fabric;
mod hems;
mod history;
mod ids;
mod links;
mod measure;
mod mirror;
mod normalize;
mod num;
mod parts;
mod piece;
mod project;
mod seam_edit;
mod seams;
mod slots;
mod symmetry;
mod templates;
mod torso;
mod wire;

pub use avatar::Avatar;
pub use avatar::{DEFAULT_SKIN, FOLLOW_SIZE_KEY, Measurements, Sex, SizeSpec, size_chart};
pub use command::{Applied, Command, Created, EditError, apply};
pub use edit::{
    CutError, DEFAULT_DART_DEPTH_MM, DEFAULT_DART_WIDTH_MM, DartError, DeletePointsError, DuplicateMode,
    ELLIPSE_KAPPA, FlipAxis, MIN_CORNERS, MIN_SHAPE_MM, PieceTransform, RigidMap, Shape, Spread,
    add_curve_point, add_dart, add_fabric, add_internal_ellipse, add_internal_line, create_piece,
    create_shape_piece, cut_piece, de_casteljau_split, delete_colorway, delete_fabric, delete_internal_line,
    delete_pieces, delete_points, duplicate_colorway, duplicate_fabric, duplicate_piece, fillet_corner,
    flip_piece, grade_points_linear, grade_step, internal_fold, remap_refs, rigid_map, rotate_piece_90,
    seg_bez_param, set_base_size, set_curvature, set_grade_delta, set_grade_step_axis, set_internal_closed,
    set_internal_fold, set_point_curve, set_points_median, set_segment_length, set_sizes, split_segment,
    spread_piece, transform_piece, translate_internal_line,
};
pub use fabric::{
    DEFAULT_NORMAL_STRENGTH, DEFAULT_TEXTURE_MM, FABRIC_MAP_MM_KEY, FABRIC_NORMAL_MAP_KEY,
    FABRIC_NORMAL_STRENGTH_KEY, FABRIC_ROUGHNESS_MAP_KEY, FABRIC_TEXTURE_KEY, FABRIC_TEXTURE_MM_KEY, Fabric,
    FabricPhysics, FabricPreset, MAX_NORMAL_STRENGTH, PhysKey, PresetSpec, SurfaceMaps, e2d, limit_strain,
    make_fabric, r_bend, r_stretch, stress_kpa,
};
pub use hems::{default_hem, detect_hems, mark_hems, set_hem};
pub use history::{DEFAULT_HISTORY_LIMIT, History};
pub use ids::{
    ColorwayId, FabricId, IdGen, ItemId, PartId, PieceId, PointId, PomId, SeamId, SizeName, StitchId, prefix,
};
pub use links::{
    LINK_TOLERANCE_MM, LengthLink, LinkLengths, SegmentRef, add_length_link, fit_segment, link_lengths,
    remove_length_link, segment_length, sync_length_links,
};
pub use measure::{SeamLengths, piece_area, pom_value, sew_lengths};
pub use mirror::{MIRROR_PREFIX, mirror_seg_id, mirrored_point_id, src_seg_id, sync_mirrors};
pub use normalize::{LoadError, normalize_value};
pub use parts::{
    BUTTON_FASTEN_KEY, BUTTON_FINISH_KEY, BUTTON_HOLES_KEY, BUTTON_SHAPE_KEY, BUTTON_SIZE_KEY, COLOR_KEY,
    COMMENT_AT_KEY, COMMENT_NUMBER_KEY, COMMENT_RESOLVED_KEY, COMMENT_TEXT_KEY, DEFAULT_BUTTON_COLOR,
    DEFAULT_BUTTON_FINISH, DEFAULT_BUTTON_HOLES, DEFAULT_BUTTON_SHAPE, DEFAULT_BUTTON_SIZE_MM,
    DEFAULT_GRAPHIC_WIDTH_MM, DEFAULT_HOLE_COLOR, DEFAULT_HOLE_LENGTH_MM, DEFAULT_STITCH_COLOR,
    DEFAULT_STITCH_GAP_MM, DEFAULT_STITCH_LENGTH_MM, DEFAULT_STITCH_OFFSET_MM, DEFAULT_STITCH_ROW_GAP_MM,
    DEFAULT_STITCH_ROWS, DEFAULT_STITCH_WIDTH_MM, Extras, FALLBACK_STITCH_COLOR, FALLBACK_STITCH_GAP_MM,
    FALLBACK_STITCH_OFFSET_MM, FALLBACK_STITCH_WIDTH_MM, GRAPHIC_HEIGHT_KEY, GRAPHIC_NAME_KEY,
    GRAPHIC_OPACITY_KEY, GRAPHIC_PATH_KEY, GRAPHIC_ROTATION_KEY, GRAPHIC_WIDTH_KEY, HOLE_ANGLE_KEY,
    HOLE_LENGTH_KEY, ItemKind, LINE_CLOSED_KEY, LINE_FOLD_KEY, STITCH_GAP_KEY, STITCH_KIND_KEY,
    STITCH_LENGTH_KEY, STITCH_OFFSET_KEY, STITCH_ROW_GAP_KEY, STITCH_ROWS_KEY, STITCH_WIDTH_KEY, StitchStyle,
    ZIPPER_KIND, ZIPPER_TAPE_COLOR, ZIPPER_TAPE_MM, ZIPPER_TEETH_COLOR, add_button, add_comment, add_graphic,
    add_hole, add_notch, add_stitch, add_zipper_half, delete_item, fasten_button, graphic_size, is_zipper,
    move_item,
};
pub use piece::{
    Arrangement, DEFAULT_ARRANGEMENT_OFFSET_MM, DEFAULT_PARTICLE_DISTANCE_MM, DEFAULT_SEAM_ALLOWANCE_MM,
    DEFAULT_SLOT, Graphic, InternalLine, LinePoint, MAIN_FABRIC, MirrorLink, Notch, Piece, PieceState, Pin,
    mirror_points, translate_points,
};
pub use project::{
    Colorway, DEFAULT_BASE_SIZE, DEFAULT_SIZES, FORMAT_VERSION, Item, MAIN_COLORWAY, MAIN_COLORWAY_NAME,
    Meta, Pom, PomRef, Project, SEW_COLORS, Seam, SeamGroup, SeamSide, SimSettings, Stitch, TechPack,
    UNTITLED, Units, WHOLE_OUTLINE,
};
pub use seam_edit::{OutlineSpot, flip_seam, free_side, segment_side_from, swap_seam_groups};
pub use seams::{
    CoveredSide, EdgeCoverage, FreePart, edge_coverage, seam_numbers, seam_sa, set_all_sa, set_seam_sa,
    set_segment_sa, side_seg_ids,
};
pub use slots::{SlotKind, mirror_slot, slot_for_name, slot_kind};
pub use symmetry::{
    SYMMETRY_KEY, SymmetrySnapshot, symmetry_axis, symmetry_snapshot, sync_symmetry, toggle_symmetry,
};
pub use templates::{
    DraftOptions, Template, draft_dress, draft_skirt, draft_tee, from_template, reference_template, refine,
};
pub use torso::{BodyProfile, Landmarks, TorsoProfile, ellipse_axes};
