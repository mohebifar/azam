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
}

impl Template {
    /// Every template, in menu order.
    pub const ALL: [Self; 5] = [Self::Tee, Self::Dress, Self::Skirt, Self::Labadeh, Self::Aba];

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
        ],
        &[
            Self::Polygon,
            Self::Rectangle,
            Self::Ellipse,
            Self::InternalLine,
            Self::InternalEllipse,
            Self::Notch,
            Self::Dart,
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
            Self::Notch
            | Self::Zipper
            | Self::Button
            | Self::Buttonhole
            | Self::FastenButton
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
    /// Export the pattern as DXF.
    ExportDxf,
    /// Export the pattern as SVG.
    ExportSvg,
    /// Export the draped garment as OBJ.
    ExportObj,
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
    /// Show or hide the avatar's arrangement points (click one to arrange the selected pieces).
    ToggleArrangementPoints,
    /// Show the tool shelves' tool names beside their icons (Blender's wide toolbar).
    ToggleToolLabels,
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
    /// Path-trace the 3D view into an image (Blender's F12).
    RenderImage,
    /// Show a fit map (switching to fit map shading).
    SetFitMap(azam_sim::FitMode),
    /// Show an editor in an area (the editor-type menu).
    SetEditor(crate::editors::Slot, crate::editors::Editor),
    /// Split an area: its partner area opens beside it.
    SplitArea(crate::editors::Slot),
    /// Join an area with its partner, keeping its editor.
    JoinArea(crate::editors::Slot),
}

impl Operator {
    /// Display name.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::New(t) => format!("New {}", t.label()),
            Self::SetTool(t) => format!("Tool: {}", t.label()),
            Self::SetShading(Shading::Solid) => "Solid Shading".into(),
            Self::SetShading(Shading::FitMap) => "Fit Map Shading".into(),
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
            Self::ExportDxf => "Export DXF…",
            Self::ExportSvg => "Export SVG…",
            Self::ExportObj => "Export OBJ…",
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
            Self::ToggleArrangementPoints => "Show Arrangement Points",
            Self::ToggleToolLabels => "Tool Names in Shelf",
            Self::ToggleSidebar => "Sidebar",
            Self::ShadingPie => "Shading Pie",
            Self::ViewPie => "View Pie",
            Self::Shortcuts => "Keyboard Shortcuts",
            Self::About => "About azam",
            Self::RenderImage => "Render Image",
            Self::New(_)
            | Self::SetTool(_)
            | Self::SetShading(_)
            | Self::SetFitMap(_)
            | Self::SetEditor(..)
            | Self::View(_) => "",
            Self::SplitArea(_) => "Split Area",
            Self::JoinArea(_) => "Join Areas",
        }
    }

    /// Blender icon, if any.
    #[must_use]
    pub const fn icon(self) -> Option<&'static str> {
        Some(match self {
            Self::New(_) => "FILE_NEW",
            Self::Open => "FILE_FOLDER",
            Self::Save | Self::SaveAs => "FILE_TICK",
            Self::Share | Self::ShareFile => "COMMUNITY",
            Self::Join => "LINKED",
            Self::StopSharing => "UNLINKED",
            Self::ImportDxf | Self::ImportSvg => "IMPORT",
            Self::ExportDxf
            | Self::ExportSvg
            | Self::ExportObj
            | Self::ExportPatternPdf
            | Self::ExportTechPackPdf => "EXPORT",
            Self::Quit => "QUIT",
            Self::Undo => "LOOP_BACK",
            Self::Redo => "LOOP_FORWARDS",
            Self::Duplicate => "DUPLICATE",
            Self::Delete => "TRASH",
            Self::Mirror => "MOD_MIRROR",
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
            Self::SetShading(Shading::FitMap) | Self::SetFitMap(_) => "SHADING_TEXTURE",
            Self::View(ViewPreset::Top) => "AXIS_TOP",
            Self::View(ViewPreset::Right | ViewPreset::Left) => "AXIS_SIDE",
            Self::View(_) => "AXIS_FRONT",
            Self::ToggleOrtho => "VIEW_ORTHO",
            Self::FrameAll | Self::FramePattern => "ZOOM_ALL",
            Self::ToggleAvatar => "USER",
            Self::ToggleGrid => "GRID",
            Self::ToggleArrangementPoints => "PIVOT_CURSOR",
            Self::ToggleToolLabels => "ALIGN_LEFT",
            Self::ToggleSidebar => "MENU_PANEL",
            Self::Shortcuts => "KEY_COMMAND",
            Self::About => "INFO",
            Self::RenderImage => "RENDER_STILL",
            Self::SetEditor(_, e) => e.icon(),
            Self::SplitArea(_) => "SPLIT_VERTICAL",
            Self::JoinArea(_) => "AREA_JOIN",
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
            ("z", true, false, _) => Self::Undo,
            ("z", true, true, _) | ("y", true, _, _) => Self::Redo,
            ("a", false, false, false) => Self::SelectAll,
            ("d", false, true, _) => Self::Duplicate,
            ("x" | "delete" | "backspace", false, false, _) => Self::Delete,
            ("return" | "kp_enter", false, _, _) => Self::FinishDraft,
            ("m", true, false, _) => Self::Mirror,
            ("f3", ..) => Self::Search,
            ("f12", ..) => Self::RenderImage,
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
            Self::ExportDxf,
            Self::ExportSvg,
            Self::ExportObj,
            Self::ExportPatternPdf,
            Self::ExportTechPackPdf,
            Self::Undo,
            Self::Redo,
            Self::SelectAll,
            Self::Duplicate,
            Self::Delete,
            Self::Mirror,
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
            Self::ToggleArrangementPoints,
            Self::ToggleToolLabels,
            Self::ToggleSidebar,
            Self::Shortcuts,
            Self::About,
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
    /// In the 3D view.
    Viewport,
}

impl ContextMenu {
    /// Every right-click menu, in the order [`context_menus`] lists them.
    pub const ALL: [Self; 3] = [Self::Piece, Self::Pattern, Self::Viewport];
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
                Separator,
                Item(O::FlipHorizontal),
                Item(O::FlipVertical),
                Item(O::Rotate90),
                Separator,
                Item(O::DetectHems),
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
                Item(O::ToggleArrangementPoints),
                Separator,
                Item(O::RenderImage),
            ],
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
    let mut entries = Vec::new();
    for (i, group) in Tool::PATTERN_GROUPS.iter().enumerate() {
        if i > 0 {
            entries.push(MenuEntry::Separator);
        }
        entries.extend(group.iter().map(|&t| MenuEntry::Item(Operator::SetTool(t))));
    }
    entries
}

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
pub fn top_menus() -> Vec<Menu> {
    use MenuEntry::{Item, Separator};
    use Operator as O;
    let mut file: Vec<MenuEntry> = vec![MenuEntry::Sub("New", "FILE_NEW", NEW_MENU)];
    file.extend([
        Separator,
        Item(O::Open),
        Item(O::Save),
        Item(O::SaveAs),
        Separator,
        Item(O::Share),
        Item(O::ShareFile),
        Item(O::Join),
        Item(O::StopSharing),
        Separator,
        Item(O::ImportDxf),
        Item(O::ImportSvg),
        Item(O::ExportDxf),
        Item(O::ExportSvg),
        Item(O::ExportObj),
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
                MenuEntry::Sub("Fit Map", "SHADING_TEXTURE", FIT_MAP_MENU),
                Item(O::ToggleAvatar),
                Item(O::ToggleGrid),
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
