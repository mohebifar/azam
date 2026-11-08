//! Operators: every user action, with its name, icon and shortcut, as in Blender.
//!
//! Menus, the toolbar, keyboard shortcuts and the operator search (F3) all produce an
//! [`Operator`]; the app runs it in one place.

use blenderui::Keystroke;

/// A garment template to start a project from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    /// T-shirt.
    Tee,
    /// Dress.
    Dress,
    /// Skirt.
    Skirt,
    /// Labadeh (long robe).
    Labadeh,
    /// Aba (cloak).
    Aba,
    /// An empty board, to draft from scratch.
    Blank,
}

impl Template {
    /// Every template, in menu order.
    pub const ALL: [Self; 6] = [
        Self::Tee,
        Self::Dress,
        Self::Skirt,
        Self::Labadeh,
        Self::Aba,
        Self::Blank,
    ];

    /// The template keyed `key` (`tee`, `dress`, `skirt`, `labadeh`, `aba`).
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// Display name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tee => "T-Shirt",
            Self::Dress => "Dress",
            Self::Skirt => "Skirt",
            Self::Labadeh => "Labadeh",
            Self::Aba => "Aba",
            Self::Blank => "Blank",
        }
    }

    /// Key of the template in the project model.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Tee => "tee",
            Self::Dress => "dress",
            Self::Skirt => "skirt",
            Self::Labadeh => "labadeh",
            Self::Aba => "aba",
            Self::Blank => "blank",
        }
    }
}

/// 3D viewport shading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shading {
    /// Fabric colours under studio light.
    Solid,
    /// Strain map over the garment.
    FitMap,
    /// Path-traced in place, refining while nothing moves (Blender's Rendered shading).
    Rendered,
}

/// A tool of the 2D pattern editor or the 3D viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Select, move and rotate pieces.
    Select,
    /// Edit outline points, segments and internal lines.
    EditPoints,
    /// Bend a segment by dragging it.
    Curvature,
    /// Add and move curve control points.
    CurvePoint,
    /// Add a point on a segment.
    AddPoint,
    /// Draw a piece point by point.
    Polygon,
    /// Draw a rectangular piece.
    Rectangle,
    /// Draw an elliptical piece.
    Ellipse,
    /// Draw an internal polygon or line inside a piece.
    InternalLine,
    /// Draw an internal ellipse inside a piece.
    InternalEllipse,
    /// Put a notch on a segment.
    Notch,
    /// Cut a sewn dart into a segment.
    Dart,
    /// Sew two segments (Shift: several segments per side).
    Sew,
    /// Sew two stretches between freely picked points.
    FreeSew,
    /// Select seams to flip, swap or delete them.
    EditSew,
    /// Topstitch a segment (Shift: the whole outline).
    Topstitch,
    /// Put a zipper between two segments: they are sewn, and each gets a zipper half.
    Zipper,
    /// Select topstitches.
    EditTopstitch,
    /// Place and move buttons.
    Button,
    /// Place and move buttonholes.
    Buttonhole,
    /// Fasten a button to a buttonhole.
    FastenButton,
    /// Place, move and scale graphics.
    Graphic,
    /// Pin a comment on a piece.
    Comment,
    /// Grab cloth in the 3D view while it simulates.
    Grab,
    /// Pin cloth in place.
    Pin,
    /// Measure a distance.
    Measure,
    /// Cut pieces in two along a dragged line.
    Cut,
    /// Round a corner (its radius in the tool settings).
    Fillet,
    /// Slash and spread: drag a line across a piece to open a wedge at its start (flare).
    Spread,
    /// Drag a line across a piece to add a knife pleat along it.
    Pleat,
}

impl Tool {
    /// Tools of the 2D pattern editor in shelf order, grouped as the shelf separates them.
    pub const PATTERN_GROUPS: [&'static [Self]; 5] = [
        &[
            Self::Select,
            Self::EditPoints,
            Self::Curvature,
            Self::CurvePoint,
            Self::AddPoint,
            Self::Fillet,
        ],
        &[
            Self::Polygon,
            Self::Rectangle,
            Self::Ellipse,
            Self::InternalLine,
            Self::InternalEllipse,
            Self::Notch,
            Self::Dart,
            Self::Cut,
            Self::Spread,
            Self::Pleat,
        ],
        &[Self::Sew, Self::FreeSew, Self::EditSew],
        &[
            Self::Topstitch,
            Self::EditTopstitch,
            Self::Zipper,
            Self::Button,
            Self::Buttonhole,
            Self::FastenButton,
            Self::Graphic,
        ],
        &[Self::Comment, Self::Measure],
    ];
    /// Tools of the 3D viewport, in shelf order.
    pub const VIEWPORT: [Self; 3] = [Self::Select, Self::Grab, Self::Pin];

    /// Every pattern tool in shelf order.
    pub fn pattern() -> impl Iterator<Item = Self> {
        Self::PATTERN_GROUPS.into_iter().flatten().copied()
    }

    /// Display name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Select => "Select / Transform",
            Self::EditPoints => "Edit Pattern",
            Self::Curvature => "Edit Curvature",
            Self::CurvePoint => "Edit Curve Point",
            Self::AddPoint => "Add Point / Split Line",
            Self::Polygon => "Polygon",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::InternalLine => "Internal Polygon / Line",
            Self::InternalEllipse => "Internal Ellipse",
            Self::Notch => "Notch",
            Self::Dart => "Dart",
            Self::Sew => "Segment Sewing",
            Self::FreeSew => "Free Sewing",
            Self::EditSew => "Edit Sewing",
            Self::Topstitch => "Segment Topstitch",
            Self::Zipper => "Zipper",
            Self::EditTopstitch => "Edit Topstitch",
            Self::Button => "Button",
            Self::Buttonhole => "Buttonhole",
            Self::FastenButton => "Fasten Button",
            Self::Graphic => "Graphic",
            Self::Comment => "Comment",
            Self::Grab => "Grab",
            Self::Pin => "Pin",
            Self::Measure => "Measure",
            Self::Cut => "Cut",
            Self::Fillet => "Fillet",
            Self::Spread => "Slash & Spread",
            Self::Pleat => "Pleat",
        }
    }

    /// Blender icon.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Select => "RESTRICT_SELECT_OFF",
            Self::EditPoints => "VERTEXSEL",
            Self::Curvature => "SMOOTHCURVE",
            Self::CurvePoint => "CURVE_BEZCURVE",
            Self::AddPoint => "SNAP_MIDPOINT",
            Self::Polygon => "MESH_DATA",
            Self::Rectangle => "MESH_PLANE",
            Self::Ellipse => "MESH_CIRCLE",
            Self::InternalLine => "OUTLINER_DATA_GP_LAYER",
            Self::InternalEllipse => "CURVE_NCIRCLE",
            Self::Notch => "SNAP_PERPENDICULAR",
            Self::Dart => "MOD_TRIANGULATE",
            Self::Sew => "AUTOMERGE_ON",
            Self::FreeSew => "SNAP_EDGE",
            Self::EditSew => "LINKED",
            Self::Topstitch => "MOD_DASH",
            Self::Zipper => "ALIGN_JUSTIFY",
            Self::EditTopstitch => "MOD_LINEART",
            Self::Button => "RADIOBUT_ON",
            Self::Buttonhole => "REMOVE",
            Self::FastenButton => "HOOK",
            Self::Graphic => "IMAGE_DATA",
            Self::Comment => "TEXT",
            Self::Grab => "HAND",
            Self::Pin => "PINNED",
            Self::Measure => "DRIVER_DISTANCE",
            Self::Cut => "MOD_EDGESPLIT",
            Self::Fillet => "MOD_BEVEL",
            Self::Spread => "MOD_SIMPLEDEFORM",
            Self::Pleat => "MOD_SOLIDIFY",
        }
    }

    /// What the left mouse button does with this tool, for the status bar.
    #[must_use]
    pub const fn lmb_hint(self) -> &'static str {
        match self {
            Self::Select => "Select / Move",
            Self::EditPoints => "Select Points",
            Self::Curvature => "Bend Segment",
            Self::CurvePoint => "Add Curve Point",
            Self::AddPoint | Self::Polygon | Self::InternalLine => "Add Point",
            Self::Rectangle | Self::Ellipse | Self::InternalEllipse => "Drag Shape",
            Self::Notch => "Add Notch",
            Self::Dart => "Add Dart",
            Self::Sew | Self::FreeSew | Self::Topstitch | Self::Zipper => "Pick Segment",
            Self::EditSew => "Select Seam",
            Self::EditTopstitch => "Select Topstitch",
            Self::Button => "Place Button",
            Self::Buttonhole => "Place Buttonhole",
            Self::FastenButton => "Fasten Button",
            Self::Graphic => "Place Graphic",
            Self::Comment => "Add Comment",
            Self::Grab => "Drag Cloth",
            Self::Pin => "Pin / Unpin",
            Self::Measure => "Measure",
            Self::Cut => "Draw Cut",
            Self::Fillet => "Round Corner",
            Self::Spread => "Draw Spread",
            Self::Pleat => "Draw Pleat",
        }
    }

    /// Key that activates the tool, as menus show it.
    #[must_use]
    pub const fn key(self) -> Option<&'static str> {
        Some(match self {
            Self::Select => "W",
            Self::EditPoints => "E",
            Self::Curvature => "C",
            Self::CurvePoint => "V",
            Self::AddPoint => "P",
            Self::Polygon => "H",
            Self::Rectangle => "S",
            Self::Ellipse => "O",
            Self::InternalLine => "L",
            Self::InternalEllipse => "R",
            Self::Sew => "N",
            Self::FreeSew => "Shift N",
            Self::EditSew => "B",
            Self::Topstitch => "K",
            Self::EditTopstitch => "J",
            Self::Graphic => "T",
            Self::Grab => "G",
            Self::Pin => "I",
            Self::Measure => "M",
            Self::Dart => "D",
            Self::Cut => "U",
            Self::Fillet => "F",
            Self::Notch
            | Self::Zipper
            | Self::Button
            | Self::Buttonhole
            | Self::FastenButton
            | Self::Spread
            | Self::Pleat
            | Self::Comment => {
                return None;
            }
        })
    }

    /// The tool a plain (or Shift) letter key chooses.
    #[must_use]
    fn for_key(letter: &str, shift: bool) -> Option<Self> {
        Some(match (letter, shift) {
            ("w", false) => Self::Select,
            ("e", false) => Self::EditPoints,
            ("c", false) => Self::Curvature,
            ("v", false) => Self::CurvePoint,
            ("p", false) => Self::AddPoint,
            ("h", false) => Self::Polygon,
            ("s", false) => Self::Rectangle,
            ("o", false) => Self::Ellipse,
            ("l", false) => Self::InternalLine,
            ("r", false) => Self::InternalEllipse,
            ("n", false) => Self::Sew,
            ("n", true) => Self::FreeSew,
            ("b", false) => Self::EditSew,
            ("k", false) => Self::Topstitch,
            ("j", false) => Self::EditTopstitch,
            ("t", false) => Self::Graphic,
            ("g", false) => Self::Grab,
            ("i", false) => Self::Pin,
            ("m", false) => Self::Measure,
            ("d", false) => Self::Dart,
            ("u", false) => Self::Cut,
            ("f", false) => Self::Fillet,
            _ => return None,
        })
    }
}

/// A fixed 3D view.
pub use azam_render::ViewPreset;

/// Something the user can do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operator {
    /// Start a new project from a template.
    New(Template),
    /// Open a project file.
    Open,
    /// Save the project.
    Save,
    /// Save the project under a new name.
    SaveAs,
    /// Share the project with collaborators on the LAN.
    Share,
    /// Share the project through an update log next to its file (same machine or shared folder).
    ShareFile,
    /// Join a collaborator's session by `host:port` or share code.
    Join,
    /// Stop sharing.
    StopSharing,
    /// Import pieces from a DXF file.
    ImportDxf,
    /// Import pattern pieces from an SVG drawing.
    ImportSvg,
    /// Use a body mesh from an OBJ file as the avatar.
    ImportAvatar,
    /// Import a fabric from a U3M material (`.u3m` or `.u3ma`).
    ImportU3m,
    /// Go back to the built-in avatar body.
    UseBuiltinBody,
    /// Export the pattern as DXF.
    ExportDxf,
    /// Export every size of every piece as a graded nest (DXF).
    ExportDxfGraded,
    /// Export the base size with grade rules: an ASTM D6673 DXF and its rule table (`.rul`).
    ExportAstm,
    /// Export the marker (the pattern nested on the fabric) as DXF.
    ExportMarkerDxf,
    /// Export the marker as SVG.
    ExportMarkerSvg,
    /// Export the pattern as SVG.
    ExportSvg,
    /// Export the draped garment as OBJ.
    ExportObj,
    /// Export the draped garment (and the avatar, if shown) as binary glTF.
    ExportGltf,
    /// Export the draped garment (and the avatar, if shown) as binary FBX.
    ExportFbx,
    /// Export the pattern as a PDF to print at 1:1.
    ExportPatternPdf,
    /// Export the tech pack as a PDF.
    ExportTechPackPdf,
    /// Quit.
    Quit,
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Select every piece, or none if all are selected.
    SelectAll,
    /// Duplicate the selected pieces.
    Duplicate,
    /// Delete what is selected: a seam, part or points, else the selected pieces.
    Delete,
    /// Flip the selected pieces left to right.
    FlipHorizontal,
    /// Flip the selected pieces top to bottom.
    FlipVertical,
    /// Turn the selected pieces a quarter turn.
    Rotate90,
    /// Reverse side B of the selected seam.
    FlipSeam,
    /// Swap the two sides of the selected seam.
    SwapSeamSides,
    /// Finish the shape or seam being drawn.
    FinishDraft,
    /// Mirror the selected pieces.
    Mirror,
    /// Edit the active piece symmetrically about its vertical centre line (or stop).
    ToggleSymmetry,
    /// Detect hems on the selected pieces.
    DetectHems,
    /// Open the operator search.
    Search,
    /// Start or pause the simulation.
    TogglePlay,
    /// Put the garment back in its arranged pose.
    ResetSim,
    /// Advance the simulation by one frame.
    StepSim,
    /// Choose a tool.
    SetTool(Tool),
    /// Choose the 3D shading.
    SetShading(Shading),
    /// Turn the 3D view.
    View(ViewPreset),
    /// Toggle orthographic projection.
    ToggleOrtho,
    /// Frame everything in the 3D view.
    FrameAll,
    /// Frame every piece in the 2D view.
    FramePattern,
    /// Show or hide the avatar.
    ToggleAvatar,
    /// Show or hide the floor grid.
    ToggleGrid,
    /// Show or hide every mesh's edges over its shading (Blender's Wireframe overlay).
    ToggleWireframe,
    /// Show or hide the avatar's arrangement points (click one to arrange the selected pieces).
    ToggleArrangementPoints,
    /// Show the tool shelves' tool names beside their icons (Blender's wide toolbar).
    ToggleToolLabels,
    /// Turn the selected outline points from corners into curve points, or back.
    ToggleCurvePoint,
    /// Show or hide the 3D view's sidebar (Blender's N panel).
    ToggleSidebar,
    /// Open the shading pie menu.
    ShadingPie,
    /// Open the view pie menu.
    ViewPie,
    /// Show the keyboard shortcuts.
    Shortcuts,
    /// About azam.
    About,
    /// Open the Preferences window (interface options and the keymap).
    Preferences,
    /// Lay the selected piece's drape flat as a new piece (CLO's 3D to 2D).
    FlattenPiece,
    /// Open the last auto save as an unsaved project.
    RecoverAutoSave,
    /// Start or stop recording the simulation's frames for animation export.
    RecordAnimation,
    /// Export the recorded frames as an animated glTF.
    ExportAnimation,
    /// Path-trace the 3D view into an image (Blender's F12).
    RenderImage,
    /// Show a fit map (switching to fit map shading).
    SetFitMap(azam_sim::FitMode),
    /// Show an editor in an area (the editor-type menu).
    SetEditor(crate::areas::AreaId, crate::editors::Editor),
    /// Split an area in two, side by side or one over the other.
    SplitArea(crate::areas::AreaId, crate::areas::Dir),
    /// Join an area with its neighbour, keeping its editor.
    JoinArea(crate::areas::AreaId),
    /// Show only this area, filling the window, or bring the others back (Ctrl Space).
    MaximizeArea(crate::areas::AreaId),
    /// Ctrl Space: maximize the area under the pointer, or bring the others back.
    MaximizeHovered,
    /// Put the current workspace's areas back as they start.
    ResetLayout,
    /// Add a workspace tab of a built-in kind.
    AddWorkspace(crate::app::Workspace),
    /// Copy the active workspace tab and its layout.
    DuplicateWorkspace,
    /// Rename the active workspace tab.
    RenameWorkspace,
    /// Delete the active workspace tab.
    DeleteWorkspace,
    /// Move the active workspace tab one place left.
    MoveWorkspaceLeft,
    /// Move the active workspace tab one place right.
    MoveWorkspaceRight,
}

/// Edit ▸ Workspace: what the tabs' right-click menu offers.
pub const WORKSPACE_MENU: &[Operator] = &[
    Operator::DuplicateWorkspace,
    Operator::RenameWorkspace,
    Operator::MoveWorkspaceLeft,
    Operator::MoveWorkspaceRight,
    Operator::DeleteWorkspace,
];

/// The workspace tabs' "+" menu: one entry per built-in kind.
pub const ADD_WORKSPACE_MENU: &[Operator] = &[
    Operator::AddWorkspace(crate::app::Workspace::Design),
    Operator::AddWorkspace(crate::app::Workspace::Grading),
    Operator::AddWorkspace(crate::app::Workspace::Colorway),
    Operator::AddWorkspace(crate::app::Workspace::Review),
    Operator::AddWorkspace(crate::app::Workspace::TechPack),
];

impl Operator {
    /// Display name.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::New(t) => format!("New {}", t.label()),
            Self::AddWorkspace(w) => format!("New Workspace: {}", w.name()),
            Self::SetTool(t) => format!("Tool: {}", t.label()),
            Self::SetShading(Shading::Solid) => "Solid Shading".into(),
            Self::SetShading(Shading::FitMap) => "Fit Map Shading".into(),
            Self::SetShading(Shading::Rendered) => "Rendered Shading".into(),
            Self::SetFitMap(m) => format!("Fit Map: {}", fit_label(m)),
            Self::SetEditor(_, e) => e.label().into(),
            Self::View(v) => match v {
                ViewPreset::Front => "Front View",
                ViewPreset::Back => "Back View",
                ViewPreset::Right => "Right View",
                ViewPreset::Left => "Left View",
                ViewPreset::Top => "Top View",
            }
            .into(),
            other => other.static_label().into(),
        }
    }

    fn static_label(self) -> &'static str {
        match self {
            Self::Open => "Open…",
            Self::Save => "Save",
            Self::SaveAs => "Save As…",
            Self::Share => "Share",
            Self::ShareFile => "Share via File Log",
            Self::Join => "Join Session…",
            Self::StopSharing => "Stop Sharing",
            Self::ImportDxf => "Import DXF…",
            Self::ImportSvg => "Import SVG…",
            Self::ImportAvatar => "Import Avatar OBJ…",
            Self::ImportU3m => "Import U3M Fabric…",
            Self::UseBuiltinBody => "Use Built-in Body",
            Self::ExportDxf => "Export DXF…",
            Self::ExportDxfGraded => "Export DXF (All Sizes)…",
            Self::ExportAstm => "Export ASTM DXF + Rules…",
            Self::ExportMarkerDxf => "Export Marker DXF…",
            Self::ExportMarkerSvg => "Export Marker SVG…",
            Self::ExportSvg => "Export SVG…",
            Self::ExportObj => "Export OBJ…",
            Self::ExportGltf => "Export glTF (.glb)…",
            Self::ExportFbx => "Export FBX…",
            Self::ExportPatternPdf => "Export Pattern PDF…",
            Self::ExportTechPackPdf => "Export Tech Pack PDF…",
            Self::Quit => "Quit",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::SelectAll => "Select All",
            Self::Duplicate => "Duplicate Pieces",
            Self::Delete => "Delete",
            Self::FlipHorizontal => "Flip Horizontally",
            Self::FlipVertical => "Flip Vertically",
            Self::Rotate90 => "Rotate 90°",
            Self::FlipSeam => "Flip Seam Side B",
            Self::SwapSeamSides => "Swap Seam Sides",
            Self::FinishDraft => "Finish Shape / Seam Side",
            Self::Mirror => "Mirror Pieces",
            Self::ToggleSymmetry => "Symmetric Editing",
            Self::DetectHems => "Detect Hems",
            Self::Search => "Search…",
            Self::TogglePlay => "Play / Pause Simulation",
            Self::ResetSim => "Reset Simulation",
            Self::StepSim => "Step Simulation",
            Self::ToggleOrtho => "Perspective / Orthographic",
            Self::FrameAll => "Frame All",
            Self::FramePattern => "Frame Pattern",
            Self::ToggleAvatar => "Show Avatar",
            Self::ToggleGrid => "Show Floor Grid",
            Self::ToggleWireframe => "Wireframe Overlay",
            Self::ToggleArrangementPoints => "Show Arrangement Points",
            Self::ToggleToolLabels => "Tool Names in Shelf",
            Self::ToggleCurvePoint => "Toggle Corner / Curve",
            Self::ToggleSidebar => "Sidebar",
            Self::ShadingPie => "Shading Pie",
            Self::ViewPie => "View Pie",
            Self::Shortcuts => "Keyboard Shortcuts",
            Self::About => "About azam",
            Self::Preferences => "Preferences…",
            Self::FlattenPiece => "Flatten Draped Piece",
            Self::RecordAnimation => "Record Animation",
            Self::RecoverAutoSave => "Recover Auto Save",
            Self::ExportAnimation => "Export Animation (.glb)…",
            Self::RenderImage => "Render Image",
            Self::DuplicateWorkspace => "Duplicate Workspace",
            Self::RenameWorkspace => "Rename Workspace…",
            Self::DeleteWorkspace => "Delete Workspace",
            Self::MoveWorkspaceLeft => "Move Workspace Left",
            Self::MoveWorkspaceRight => "Move Workspace Right",
            Self::New(_)
            | Self::AddWorkspace(_)
            | Self::SetTool(_)
            | Self::SetShading(_)
            | Self::SetFitMap(_)
            | Self::SetEditor(..)
            | Self::View(_) => "",
            Self::SplitArea(_, crate::areas::Dir::Row) => "Vertical Split",
            Self::SplitArea(_, crate::areas::Dir::Column) => "Horizontal Split",
            Self::JoinArea(_) => "Join with Neighbour",
            Self::MaximizeArea(_) | Self::MaximizeHovered => "Toggle Maximize Area",
            Self::ResetLayout => "Reset Layout",
        }
    }

    /// Blender icon, if any.
    #[must_use]
    pub const fn icon(self) -> Option<&'static str> {
        Some(match self {
            Self::New(_) => "FILE_NEW",
            Self::AddWorkspace(_) => "ADD",
            Self::RenameWorkspace => "GREASEPENCIL",
            Self::DeleteWorkspace => "X",
            Self::MoveWorkspaceLeft => "BACK",
            Self::MoveWorkspaceRight => "FORWARD",
            Self::Open => "FILE_FOLDER",
            Self::Save | Self::SaveAs => "FILE_TICK",
            Self::Share | Self::ShareFile => "COMMUNITY",
            Self::Join => "LINKED",
            Self::StopSharing => "UNLINKED",
            Self::ImportDxf | Self::ImportSvg => "IMPORT",
            Self::ImportAvatar => "OUTLINER_OB_ARMATURE",
            Self::ImportU3m => "MATERIAL",
            Self::UseBuiltinBody => "ARMATURE_DATA",
            Self::ExportDxf
            | Self::ExportDxfGraded
            | Self::ExportAstm
            | Self::ExportMarkerDxf
            | Self::ExportMarkerSvg
            | Self::ExportSvg
            | Self::ExportObj
            | Self::ExportGltf
            | Self::ExportFbx
            | Self::ExportAnimation
            | Self::ExportPatternPdf
            | Self::ExportTechPackPdf => "EXPORT",
            Self::Quit => "QUIT",
            Self::Undo => "LOOP_BACK",
            Self::Redo => "LOOP_FORWARDS",
            Self::Duplicate | Self::DuplicateWorkspace => "DUPLICATE",
            Self::Delete => "TRASH",
            Self::Mirror => "MOD_MIRROR",
            Self::ToggleSymmetry => "MOD_TRIANGULATE",
            Self::FlipHorizontal => "ARROW_LEFTRIGHT",
            Self::FlipVertical => "EMPTY_SINGLE_ARROW",
            Self::Rotate90 => "ORIENTATION_GIMBAL",
            Self::FlipSeam => "UV_SYNC_SELECT",
            Self::SwapSeamSides => "UV_ISLANDSEL",
            Self::FinishDraft => "CHECKMARK",
            Self::Search => "VIEWZOOM",
            Self::TogglePlay => "PLAY",
            Self::ResetSim => "FILE_REFRESH",
            Self::StepSim => "FRAME_NEXT",
            Self::SetTool(t) => t.icon(),
            Self::SetShading(Shading::Solid) => "SHADING_SOLID",
            Self::SetShading(Shading::Rendered) => "SHADING_RENDERED",
            Self::SetShading(Shading::FitMap) | Self::SetFitMap(_) => "SHADING_TEXTURE",
            Self::View(ViewPreset::Top) => "AXIS_TOP",
            Self::View(ViewPreset::Right | ViewPreset::Left) => "AXIS_SIDE",
            Self::View(_) => "AXIS_FRONT",
            Self::ToggleOrtho => "VIEW_ORTHO",
            Self::FrameAll | Self::FramePattern => "ZOOM_ALL",
            Self::ToggleAvatar => "USER",
            Self::ToggleGrid => "GRID",
            Self::ToggleWireframe => "SHADING_WIRE",
            Self::ToggleArrangementPoints => "PIVOT_CURSOR",
            Self::ToggleToolLabels => "ALIGN_LEFT",
            Self::ToggleCurvePoint => "HANDLE_AUTO",
            Self::ToggleSidebar => "MENU_PANEL",
            Self::Shortcuts => "KEY_COMMAND",
            Self::About => "INFO",
            Self::Preferences => "PREFERENCES",
            Self::FlattenPiece => "UV_DATA",
            Self::RecordAnimation => "REC",
            Self::RecoverAutoSave => "RECOVER_LAST",
            Self::RenderImage => "RENDER_STILL",
            Self::SetEditor(_, e) => e.icon(),
            Self::SplitArea(_, crate::areas::Dir::Row) => "SPLIT_VERTICAL",
            Self::SplitArea(_, crate::areas::Dir::Column) => "SPLIT_HORIZONTAL",
            Self::JoinArea(_) => "AREA_JOIN",
            Self::MaximizeArea(_) | Self::MaximizeHovered => "FULLSCREEN_ENTER",
            Self::ResetLayout => "WORKSPACE",
            Self::SelectAll | Self::DetectHems | Self::ShadingPie | Self::ViewPie => return None,
        })
    }

    /// The shortcut shown in menus.
    #[must_use]
    pub fn shortcut(self) -> Option<String> {
        Some(
            match self {
                Self::New(Template::Tee) => "Ctrl N",
                Self::Open => "Ctrl O",
                Self::Save => "Ctrl S",
                Self::SaveAs => "Shift Ctrl S",
                Self::Quit => "Ctrl Q",
                Self::Preferences => "Ctrl Comma",
                Self::MaximizeArea(_) | Self::MaximizeHovered => "Ctrl Space",
                Self::Undo => "Ctrl Z",
                Self::Redo => "Shift Ctrl Z",
                Self::SelectAll => "A",
                Self::Duplicate => "Shift D",
                Self::Delete => "X",
                Self::Mirror => "Ctrl M",
                Self::Search => "F3",
                Self::RenderImage => "F12",
                Self::TogglePlay => "Space",
                Self::ResetSim => "Alt R",
                Self::StepSim => "→",
                Self::SetTool(t) => return t.key().map(str::to_owned),
                Self::FinishDraft => "Enter",
                Self::View(ViewPreset::Front) => "Numpad 1",
                Self::View(ViewPreset::Back) => "Ctrl Numpad 1",
                Self::View(ViewPreset::Right) => "Numpad 3",
                Self::View(ViewPreset::Left) => "Ctrl Numpad 3",
                Self::View(ViewPreset::Top) => "Numpad 7",
                Self::ToggleOrtho => "Numpad 5",
                Self::FrameAll => "Home",
                Self::FramePattern => "Shift Home",
                Self::ShadingPie => "Z",
                Self::ToggleSidebar => "N",
                Self::ViewPie => "`",
                _ => return None,
            }
            .to_owned(),
        )
    }

    /// The operator a key press runs, if any.
    #[must_use]
    pub fn from_key(key: &Keystroke) -> Option<Self> {
        let m = key.modifiers;
        let k = key.key.as_str();
        let lower = k.to_ascii_lowercase();
        Some(match (lower.as_str(), m.control, m.shift, m.alt) {
            ("n", true, false, _) => Self::New(Template::Tee),
            ("o", true, false, _) => Self::Open,
            ("s", true, false, _) => Self::Save,
            ("s", true, true, _) => Self::SaveAs,
            ("q", true, _, _) => Self::Quit,
            ("comma" | ",", true, false, _) => Self::Preferences,
            ("z", true, false, _) => Self::Undo,
            ("z", true, true, _) | ("y", true, _, _) => Self::Redo,
            ("a", false, false, false) => Self::SelectAll,
            ("d", false, true, _) => Self::Duplicate,
            ("x" | "delete" | "backspace", false, false, _) => Self::Delete,
            ("return" | "kp_enter", false, _, _) => Self::FinishDraft,
            ("m", true, false, _) => Self::Mirror,
            ("f3", ..) => Self::Search,
            ("f12", ..) => Self::RenderImage,
            ("space", true, false, _) => Self::MaximizeHovered,
            ("space", ..) => Self::TogglePlay,
            ("r", false, false, true) => Self::ResetSim,
            ("right", false, false, false) => Self::StepSim,
            ("kp_1" | "kp_end", true, ..) => Self::View(ViewPreset::Back),
            ("kp_1" | "kp_end", false, ..) | ("1", false, false, false) => Self::View(ViewPreset::Front),
            ("kp_3" | "kp_next", true, ..) => Self::View(ViewPreset::Left),
            ("kp_3" | "kp_next", false, ..) | ("3", false, false, false) => Self::View(ViewPreset::Right),
            ("kp_7" | "kp_home", ..) | ("7", false, false, false) => Self::View(ViewPreset::Top),
            ("kp_5" | "kp_begin", ..) | ("5", false, false, false) => Self::ToggleOrtho,
            ("home", false, false, _) => Self::FrameAll,
            ("home", false, true, _) => Self::FramePattern,
            ("z", false, false, false) => Self::ShadingPie,
            ("grave" | "`", false, false, false) => Self::ViewPie,
            (letter, false, shift, false) => Self::SetTool(Tool::for_key(letter, shift)?),
            _ => return None,
        })
    }

    /// Every operator the search lists.
    #[must_use]
    pub fn searchable() -> Vec<Self> {
        let mut all: Vec<Self> = Template::ALL.iter().map(|&t| Self::New(t)).collect();
        all.extend([
            Self::Open,
            Self::Save,
            Self::SaveAs,
            Self::Share,
            Self::ShareFile,
            Self::Join,
            Self::StopSharing,
            Self::ImportDxf,
            Self::ImportSvg,
            Self::ImportAvatar,
            Self::ImportU3m,
            Self::DuplicateWorkspace,
            Self::RenameWorkspace,
            Self::DeleteWorkspace,
            Self::MoveWorkspaceLeft,
            Self::MoveWorkspaceRight,
            Self::UseBuiltinBody,
            Self::ExportDxf,
            Self::ExportDxfGraded,
            Self::ExportAstm,
            Self::ExportMarkerDxf,
            Self::ExportMarkerSvg,
            Self::ExportSvg,
            Self::ExportObj,
            Self::ExportGltf,
            Self::ExportFbx,
            Self::ExportPatternPdf,
            Self::ExportTechPackPdf,
            Self::Undo,
            Self::Redo,
            Self::SelectAll,
            Self::Duplicate,
            Self::Delete,
            Self::Mirror,
            Self::ToggleSymmetry,
            Self::FlipHorizontal,
            Self::FlipVertical,
            Self::Rotate90,
            Self::FlipSeam,
            Self::SwapSeamSides,
            Self::DetectHems,
            Self::TogglePlay,
            Self::ResetSim,
            Self::StepSim,
            Self::SetShading(Shading::Solid),
            Self::SetShading(Shading::FitMap),
            Self::SetShading(Shading::Rendered),
            Self::SetFitMap(azam_sim::FitMode::Fit),
            Self::SetFitMap(azam_sim::FitMode::Strain),
            Self::SetFitMap(azam_sim::FitMode::Stress),
            Self::SetFitMap(azam_sim::FitMode::Pressure),
            Self::View(ViewPreset::Front),
            Self::View(ViewPreset::Back),
            Self::View(ViewPreset::Right),
            Self::View(ViewPreset::Left),
            Self::View(ViewPreset::Top),
            Self::ToggleOrtho,
            Self::FrameAll,
            Self::FramePattern,
            Self::ToggleAvatar,
            Self::ToggleGrid,
            Self::ToggleWireframe,
            Self::ToggleArrangementPoints,
            Self::ToggleToolLabels,
            Self::ToggleSidebar,
            Self::Shortcuts,
            Self::About,
            Self::Preferences,
            Self::ResetLayout,
            Self::FlattenPiece,
            Self::RecordAnimation,
            Self::ExportAnimation,
            Self::RecoverAutoSave,
            Self::RenderImage,
        ]);
        all.extend(
            Tool::pattern()
                .chain(Tool::VIEWPORT[1..].iter().copied())
                .map(Self::SetTool),
        );
        all
    }
}

/// An entry of a dropdown menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuEntry {
    /// Runs an operator.
    Item(Operator),
    /// A divider.
    Separator,
    /// A submenu (Blender's "New ▸"): its title, icon and items.
    Sub(&'static str, &'static str, &'static [Operator]),
}

/// The File ▸ New submenu: one entry per template.
pub const NEW_MENU: &[Operator] = &[
    Operator::New(Template::Tee),
    Operator::New(Template::Dress),
    Operator::New(Template::Skirt),
    Operator::New(Template::Labadeh),
    Operator::New(Template::Aba),
    Operator::New(Template::Blank),
];

/// The 3D view's View ▸ Pie Menus submenu: the pies Z and the backquote key open, for finding
/// them by menu.
pub const PIE_MENU: &[Operator] = &[Operator::ShadingPie, Operator::ViewPie];

/// The fit maps, in the order the 3D header's dropdown lists them.
pub const FIT_MODES: [azam_sim::FitMode; 4] = [
    azam_sim::FitMode::Fit,
    azam_sim::FitMode::Strain,
    azam_sim::FitMode::Stress,
    azam_sim::FitMode::Pressure,
];

/// View ▸ Fit Map: one entry per fit map.
pub const FIT_MAP_MENU: &[Operator] = &[
    Operator::SetFitMap(azam_sim::FitMode::Fit),
    Operator::SetFitMap(azam_sim::FitMode::Strain),
    Operator::SetFitMap(azam_sim::FitMode::Stress),
    Operator::SetFitMap(azam_sim::FitMode::Pressure),
];

/// The name of fit map `mode`.
#[must_use]
pub const fn fit_label(mode: azam_sim::FitMode) -> &'static str {
    match mode {
        azam_sim::FitMode::Fit => "Fit",
        azam_sim::FitMode::Strain => "Strain",
        azam_sim::FitMode::Stress => "Stress",
        azam_sim::FitMode::Pressure => "Pressure",
    }
}

/// A top-bar menu: its title and entries.
pub struct Menu {
    /// Title in the top bar.
    pub title: &'static str,
    /// Entries in order.
    pub entries: Vec<MenuEntry>,
}

/// Which right-click menu to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMenu {
    /// Over a piece in the pattern editor.
    Piece,
    /// Over empty pattern space.
    Pattern,
    /// Over a seam.
    Seam,
    /// Over an outline point.
    Point,
    /// In the 3D view.
    Viewport,
    /// On a workspace tab.
    Workspace,
    /// The workspace tabs' "+" button.
    AddWorkspace,
}

impl ContextMenu {
    /// Every right-click menu, in the order [`context_menus`] lists them.
    pub const ALL: [Self; 7] = [
        Self::Piece,
        Self::Pattern,
        Self::Viewport,
        Self::Seam,
        Self::Point,
        Self::Workspace,
        Self::AddWorkspace,
    ];
}

/// The right-click menus, in [`ContextMenu::ALL`] order: commands for what is under the pointer,
/// as Blender's context menus offer.
#[must_use]
pub fn context_menus() -> Vec<Menu> {
    use MenuEntry::{Item, Separator};
    use Operator as O;
    vec![
        Menu {
            title: "Piece",
            entries: vec![
                Item(O::Duplicate),
                Item(O::Mirror),
                Item(O::ToggleSymmetry),
                Separator,
                Item(O::FlipHorizontal),
                Item(O::FlipVertical),
                Item(O::Rotate90),
                Separator,
                Item(O::DetectHems),
                Item(O::FlattenPiece),
                Separator,
                Item(O::Delete),
            ],
        },
        Menu {
            title: "Pattern",
            entries: vec![
                Item(O::SelectAll),
                Item(O::FramePattern),
                Separator,
                Item(O::SetTool(Tool::Polygon)),
                Item(O::SetTool(Tool::Rectangle)),
                Item(O::SetTool(Tool::Sew)),
                Separator,
                Item(O::ImportDxf),
                Item(O::ImportSvg),
            ],
        },
        Menu {
            title: "3D View",
            entries: vec![
                Item(O::TogglePlay),
                Item(O::ResetSim),
                Separator,
                Item(O::FrameAll),
                Item(O::ToggleOrtho),
                Separator,
                Item(O::SetShading(Shading::Solid)),
                Item(O::SetShading(Shading::FitMap)),
                Item(O::SetShading(Shading::Rendered)),
                Item(O::ToggleArrangementPoints),
                Separator,
                Item(O::RenderImage),
            ],
        },
        Menu {
            title: "Seam",
            entries: vec![
                Item(O::FlipSeam),
                Item(O::SwapSeamSides),
                Separator,
                Item(O::SetTool(Tool::EditSew)),
                Separator,
                Item(O::Delete),
            ],
        },
        Menu {
            title: "Point",
            entries: vec![
                Item(O::ToggleCurvePoint),
                Separator,
                Item(O::SetTool(Tool::EditPoints)),
                Item(O::SetTool(Tool::AddPoint)),
                Separator,
                Item(O::Delete),
            ],
        },
        Menu {
            title: "Workspace",
            entries: vec![
                Item(O::DuplicateWorkspace),
                Item(O::RenameWorkspace),
                Separator,
                Item(O::MoveWorkspaceLeft),
                Item(O::MoveWorkspaceRight),
                Separator,
                MenuEntry::Sub("New Workspace", "ADD", ADD_WORKSPACE_MENU),
                Separator,
                Item(O::DeleteWorkspace),
            ],
        },
        Menu {
            title: "Add Workspace",
            entries: ADD_WORKSPACE_MENU.iter().map(|&op| Item(op)).collect(),
        },
    ]
}

/// Menus in the editors' headers, in order: the 3D view's View and Simulation, then the pattern
/// editor's View, Select, Piece and Sewing.
#[must_use]
pub fn editor_menus() -> Vec<Menu> {
    use MenuEntry::{Item, Separator};
    use Operator as O;
    vec![
        Menu {
            title: "View",
            entries: vec![
                Item(O::View(ViewPreset::Front)),
                Item(O::View(ViewPreset::Back)),
                Item(O::View(ViewPreset::Right)),
                Item(O::View(ViewPreset::Left)),
                Item(O::View(ViewPreset::Top)),
                Separator,
                Item(O::ToggleOrtho),
                Item(O::FrameAll),
                Separator,
                Item(O::ToggleAvatar),
                Item(O::ToggleGrid),
                Item(O::ToggleWireframe),
                Item(O::ToggleArrangementPoints),
                Item(O::ToggleSidebar),
                Separator,
                MenuEntry::Sub("Pie Menus", "MENU_PANEL", PIE_MENU),
            ],
        },
        Menu {
            title: "Simulation",
            entries: vec![
                Item(O::TogglePlay),
                Item(O::StepSim),
                Item(O::ResetSim),
                Item(O::RecordAnimation),
                Separator,
                Item(O::SetTool(Tool::Grab)),
                Item(O::SetTool(Tool::Pin)),
            ],
        },
        Menu {
            title: "View",
            entries: vec![Item(O::FramePattern), Separator, Item(O::ToggleToolLabels)],
        },
        Menu {
            title: "Select",
            entries: vec![Item(O::SelectAll)],
        },
        Menu {
            title: "Piece",
            entries: vec![
                Item(O::Duplicate),
                Item(O::Mirror),
                Item(O::ToggleSymmetry),
                Item(O::Delete),
                Separator,
                Item(O::FlipHorizontal),
                Item(O::FlipVertical),
                Item(O::Rotate90),
                Separator,
                Item(O::DetectHems),
            ],
        },
        Menu {
            title: "Sewing",
            entries: sewing_entries(),
        },
    ]
}

/// The pattern tools grouped as on the shelf, for menus.
fn tool_entries() -> Vec<MenuEntry> {
    TOOL_MENUS
        .iter()
        .map(|&(title, icon, tools)| MenuEntry::Sub(title, icon, tools))
        .collect()
}

/// The Garment menu's tool submenus, one per group of the tool shelf ([`Tool::PATTERN_GROUPS`]).
pub const TOOL_MENUS: [(&str, &str, &[Operator]); 5] = [
    (
        "Edit",
        "VERTEXSEL",
        &[
            Operator::SetTool(Tool::Select),
            Operator::SetTool(Tool::EditPoints),
            Operator::SetTool(Tool::Curvature),
            Operator::SetTool(Tool::CurvePoint),
            Operator::SetTool(Tool::AddPoint),
            Operator::SetTool(Tool::Fillet),
        ],
    ),
    (
        "Draft",
        "MESH_DATA",
        &[
            Operator::SetTool(Tool::Polygon),
            Operator::SetTool(Tool::Rectangle),
            Operator::SetTool(Tool::Ellipse),
            Operator::SetTool(Tool::InternalLine),
            Operator::SetTool(Tool::InternalEllipse),
            Operator::SetTool(Tool::Notch),
            Operator::SetTool(Tool::Dart),
            Operator::SetTool(Tool::Cut),
            Operator::SetTool(Tool::Spread),
            Operator::SetTool(Tool::Pleat),
        ],
    ),
    (
        "Sewing",
        "AUTOMERGE_ON",
        &[
            Operator::SetTool(Tool::Sew),
            Operator::SetTool(Tool::FreeSew),
            Operator::SetTool(Tool::EditSew),
        ],
    ),
    (
        "Trims",
        "MOD_DASH",
        &[
            Operator::SetTool(Tool::Topstitch),
            Operator::SetTool(Tool::EditTopstitch),
            Operator::SetTool(Tool::Zipper),
            Operator::SetTool(Tool::Button),
            Operator::SetTool(Tool::Buttonhole),
            Operator::SetTool(Tool::FastenButton),
            Operator::SetTool(Tool::Graphic),
        ],
    ),
    (
        "Notes & Measure",
        "TEXT",
        &[Operator::SetTool(Tool::Comment), Operator::SetTool(Tool::Measure)],
    ),
];

/// Sewing tools and seam edits.
fn sewing_entries() -> Vec<MenuEntry> {
    use MenuEntry::{Item, Separator};
    use Operator as O;
    vec![
        Item(O::SetTool(Tool::Sew)),
        Item(O::SetTool(Tool::FreeSew)),
        Item(O::SetTool(Tool::EditSew)),
        Item(O::SetTool(Tool::Zipper)),
        Item(O::FinishDraft),
        Separator,
        Item(O::FlipSeam),
        Item(O::SwapSeamSides),
        Item(O::Delete),
        Separator,
        Item(O::DetectHems),
    ]
}

/// The top bar's menus.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the menu bar, read top to bottom")]
pub fn top_menus() -> Vec<Menu> {
    use MenuEntry::{Item, Separator};
    use Operator as O;
    let mut file: Vec<MenuEntry> = vec![MenuEntry::Sub("New", "FILE_NEW", NEW_MENU)];
    file.extend([
        Separator,
        Item(O::Open),
        Item(O::Save),
        Item(O::SaveAs),
        Item(O::RecoverAutoSave),
        Separator,
        Item(O::Share),
        Item(O::ShareFile),
        Item(O::Join),
        Item(O::StopSharing),
        Separator,
        Item(O::ImportDxf),
        Item(O::ImportSvg),
        Item(O::ImportAvatar),
        Item(O::ImportU3m),
        Item(O::ExportDxf),
        Item(O::ExportDxfGraded),
        Item(O::ExportAstm),
        Item(O::ExportMarkerDxf),
        Item(O::ExportMarkerSvg),
        Item(O::ExportSvg),
        Item(O::ExportObj),
        Item(O::ExportGltf),
        Item(O::ExportFbx),
        Item(O::ExportAnimation),
        Item(O::ExportPatternPdf),
        Item(O::ExportTechPackPdf),
        Separator,
        Item(O::Quit),
    ]);
    vec![
        Menu {
            title: "File",
            entries: file,
        },
        Menu {
            title: "Edit",
            entries: vec![
                Item(O::Undo),
                Item(O::Redo),
                Separator,
                Item(O::SelectAll),
                Item(O::Duplicate),
                Item(O::Mirror),
                Item(O::Delete),
                Separator,
                Item(O::Search),
                Separator,
                MenuEntry::Sub("Workspace", "WORKSPACE", WORKSPACE_MENU),
                Item(O::ResetLayout),
                Item(O::Preferences),
            ],
        },
        Menu {
            title: "Garment",
            entries: {
                let mut entries = tool_entries();
                entries.extend([
                    Separator,
                    Item(O::FlipHorizontal),
                    Item(O::FlipVertical),
                    Item(O::Rotate90),
                    Separator,
                    Item(O::FlipSeam),
                    Item(O::SwapSeamSides),
                    Item(O::DetectHems),
                    Item(O::FlattenPiece),
                    Separator,
                    Item(O::FramePattern),
                ]);
                entries
            },
        },
        Menu {
            title: "Simulation",
            entries: vec![
                Item(O::TogglePlay),
                Item(O::StepSim),
                Item(O::ResetSim),
                Item(O::RecordAnimation),
                Separator,
                Item(O::SetTool(Tool::Grab)),
                Item(O::SetTool(Tool::Pin)),
            ],
        },
        Menu {
            title: "View",
            entries: vec![
                Item(O::View(ViewPreset::Front)),
                Item(O::View(ViewPreset::Back)),
                Item(O::View(ViewPreset::Right)),
                Item(O::View(ViewPreset::Left)),
                Item(O::View(ViewPreset::Top)),
                Separator,
                Item(O::ToggleOrtho),
                Item(O::FrameAll),
                Separator,
                Item(O::SetShading(Shading::Solid)),
                Item(O::SetShading(Shading::FitMap)),
                Item(O::SetShading(Shading::Rendered)),
                MenuEntry::Sub("Fit Map", "SHADING_TEXTURE", FIT_MAP_MENU),
                Item(O::ToggleAvatar),
                Item(O::ToggleGrid),
                Item(O::ToggleWireframe),
                Item(O::ToggleArrangementPoints),
            ],
        },
        Menu {
            title: "Render",
            entries: vec![Item(O::RenderImage)],
        },
        Menu {
            title: "Help",
            entries: vec![Item(O::Shortcuts), Item(O::About)],
        },
    ]
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_garment_menu_lists_every_shelf_tool_in_its_group() {
        assert_eq!(TOOL_MENUS.len(), Tool::PATTERN_GROUPS.len());
        for ((_, _, menu), group) in TOOL_MENUS.iter().zip(Tool::PATTERN_GROUPS) {
            let tools: Vec<Operator> = group.iter().map(|&t| Operator::SetTool(t)).collect();
            assert_eq!(*menu, tools.as_slice());
        }
    }

    use super::*;
    use blenderui::Modifiers;

    fn key(k: &str, control: bool, shift: bool) -> Keystroke {
        Keystroke {
            key: k.into(),
            key_char: None,
            modifiers: Modifiers {
                shift,
                control,
                alt: false,
            },
        }
    }

    #[test]
    fn shortcuts_map_to_operators() {
        assert_eq!(Operator::from_key(&key("z", true, false)), Some(Operator::Undo));
        assert_eq!(Operator::from_key(&key("z", true, true)), Some(Operator::Redo));
        assert_eq!(
            Operator::from_key(&key("space", false, false)),
            Some(Operator::TogglePlay)
        );
        assert_eq!(
            Operator::from_key(&key("KP_1", false, false)),
            Some(Operator::View(ViewPreset::Front))
        );
        assert_eq!(
            Operator::from_key(&key("F12", false, false)),
            Some(Operator::RenderImage)
        );
    }

    #[test]
    fn pattern_tool_keys_follow_the_menus() {
        for tool in Tool::pattern() {
            let Some(shown) = tool.key() else { continue };
            let (shift, letter) = match shown.strip_prefix("Shift ") {
                Some(l) => (true, l),
                None => (false, shown),
            };
            let pressed = key(&letter.to_ascii_lowercase(), false, shift);
            assert_eq!(
                Operator::from_key(&pressed),
                Some(Operator::SetTool(tool)),
                "{shown}"
            );
        }
        assert_eq!(
            Operator::from_key(&key("a", false, false)),
            Some(Operator::SelectAll)
        );
        assert_eq!(
            Operator::from_key(&key("z", false, false)),
            Some(Operator::ShadingPie)
        );
        assert_eq!(
            Operator::from_key(&key("x", false, false)),
            Some(Operator::Delete)
        );
    }

    #[test]
    fn every_searchable_operator_has_a_label() {
        for op in Operator::searchable() {
            assert!(!op.label().is_empty(), "{op:?}");
        }
    }
}
