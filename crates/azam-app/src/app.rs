//! The main window: top bar with menus and workspaces, editor areas, and the status bar.

use std::path::PathBuf;

use azam_doc::{DuplicateMode, PieceId, Project, Sex, Template as DocTemplate};
use blenderui::prelude::*;

use crate::export::ExportKind;
use crate::garment3d::{GrabState, Scene3d};
use crate::icons::IconCache;
use crate::operators::{
    Menu, MenuEntry, Operator, Shading, Template, Tool, ViewPreset, editor_menus, top_menus,
};
use crate::pattern::{PATTERN_ID, PatternScene, PatternView};
use crate::pattern_input::PatternDrag;
use crate::pattern_tools::PatternTools;
use crate::properties::PropTab;
use crate::scene::pattern_scene_with;
use crate::session::Session;
use crate::theme::CSS;
use crate::viewport::{VIEWPORT_ID, Viewport};

/// A workspace: a layout of editors for one stage of the work, as in Blender's top bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Workspace {
    /// 3D viewport, pattern editor, outliner and properties.
    Design,
    /// Pattern editor with the size table.
    Grading,
    /// Fabric colours and prints.
    Colorway,
    /// Large 3D view with the fit map.
    Review,
    /// Bill of materials, measurements and exports.
    TechPack,
}

impl Workspace {
    /// The workspace named `name` (case-insensitive), as on the command line.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| {
            w.label()
                .replace(' ', "")
                .eq_ignore_ascii_case(&name.replace(['-', ' '], ""))
        })
    }

    const ALL: [Self; 5] = [
        Self::Design,
        Self::Grading,
        Self::Colorway,
        Self::Review,
        Self::TechPack,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Design => "Design",
            Self::Grading => "Grading",
            Self::Colorway => "Colorway",
            Self::Review => "Review",
            Self::TechPack => "Tech Pack",
        }
    }
}

/// Which pie menu is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PieKind {
    Shading,
    View,
}

/// An open pie menu, centred where the pointer was.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pie {
    kind: PieKind,
    at: (f32, f32),
}

/// What a file prompt will do with the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    /// Open a project.
    Open,
    /// Save the project.
    Save,
}

/// An open file prompt and the path typed so far.
#[derive(Clone, Debug, PartialEq)]
pub struct FilePrompt {
    /// What to do.
    pub action: FileAction,
    /// The path being typed.
    pub path: String,
}

/// File browser filter for project files.
const PROJECT_FILTER: &str = "azam Projects\t*.azam.json *.json";
/// Below this width a 3D view's header leaves out its motion selector.
const COMPACT_HEADER_PX: f32 = 760.0;
/// Below this width it also leaves out the fit map selector and the avatar and grid buttons.
const NARROW_HEADER_PX: f32 = 600.0;
/// Radius of a pie menu's ring of items, in logical pixels.
const PIE_RADIUS: f32 = 110.0;

/// The application view.
#[allow(clippy::struct_excessive_bools, reason = "independent view toggles")]
pub struct AzamApp {
    pub(crate) menus: Vec<Menu>,
    top_menu_count: usize,
    /// Index of the first right-click menu in `menus`.
    pub(crate) context_menu_start: usize,
    /// Index of the first editor-type menu in `menus`.
    pub(crate) editor_menu_start: usize,
    /// Each workspace's areas (made from its defaults when first shown).
    pub(crate) layouts: crate::areas::Layouts,
    /// The last area or split id handed out.
    pub(crate) next_area_id: crate::areas::AreaId,
    pub(crate) open_menu: Option<usize>,
    pub(crate) menu_anchor: (f32, f32),
    /// Where each menu title was laid out, while a menu is open: the scrim over the window routes
    /// clicks and hovers on another title to that menu, as Blender does.
    menu_title_bounds: Vec<Option<Bounds<f32>>>,
    /// The menu that was open when the mouse button last went down over the scrim.
    menu_at_press: Option<usize>,
    /// The split whose border is being dragged.
    pub(crate) resizing: Option<crate::areas::AreaId>,
    /// The piece being moved or turned in the 3D view.
    pub(crate) piece_drag: Option<crate::place3d::PieceDrag>,
    /// Whether the avatar's arrangement points are shown in the 3D view.
    pub(crate) show_arrangement: bool,
    /// Whether the tool shelves show tool names beside the icons.
    pub(crate) tool_labels: bool,
    /// Whether the 3D view's sidebar (N) is shown.
    pub(crate) show_sidebar: bool,
    /// The last dart cut, for Adjust Last Operation.
    pub(crate) last_op: Option<crate::redo_panel::LastOp>,
    /// The user's preferences (interface options, custom shortcuts).
    pub(crate) prefs: crate::prefs::Prefs,
    /// Where the preferences are saved (none in tests and headless runs).
    pub(crate) prefs_path: Option<PathBuf>,
    /// The Preferences window, while open.
    pub(crate) prefs_view: Option<crate::prefs::PrefsView>,
    /// A modal transform (G, R) in progress.
    pub(crate) modal: Option<crate::modal::Modal>,
    /// The fabric library as last read (`None`: read it when next shown).
    pub(crate) fabric_library: Option<Vec<azam_doc::Fabric>>,
    /// Where the current layout's areas and splits were laid out last frame.
    pub(crate) area_bounds: std::collections::BTreeMap<crate::areas::AreaId, Bounds<f32>>,
    workspace: Workspace,
    pub(crate) tool: Tool,
    shading: Shading,
    /// What the fit map shading shows.
    fit_mode: azam_sim::FitMode,
    pub(crate) playing: bool,
    pub(crate) viewport: Viewport,
    pub(crate) icons: IconCache,
    pub(crate) search: Option<String>,
    pie: Option<Pie>,
    file_prompt: Option<FilePrompt>,
    /// The join prompt's text, while it is open.
    pub(crate) join_prompt: Option<String>,
    /// Collaborators' selected pieces and colours, as last drawn.
    pub(crate) collab_marks: Vec<(PieceId, u32)>,
    /// When the 3D cursor was last ray cast for collaborators.
    pub(crate) cursor_cast: Option<std::time::Instant>,
    /// Frames built so far.
    frames: u64,
    pub(crate) status: String,
    focused_once: bool,
    pub(crate) session: Session,
    pub(crate) prop_tab: PropTab,
    pub(crate) pattern_view: PatternView,
    pub(crate) pattern_scene: PatternScene,
    pub(crate) pattern_bounds: Option<Bounds<f32>>,
    pub(crate) pattern_drag: Option<PatternDrag>,
    pub(crate) tools: PatternTools,
    pub(crate) scene_revision: u64,
    pub(crate) framed: bool,
    /// The editor size the pattern view was just framed for; cleared once a frame confirms that
    /// size. A workspace switch lays the editor out at a new size one frame after it is framed,
    /// so while this is set a different size frames again; later resizes keep the view.
    pub(crate) framed_size: Option<(f32, f32)>,
    pub(crate) scene3d: Scene3d,
    /// The Render window (F12), while open.
    pub(crate) render_view: Option<crate::render_view::RenderView>,
    pub(crate) tooltip: Option<crate::tooltip::Tooltip>,
    pub(crate) open_submenu: Option<&'static str>,
    submenu_anchor: (f32, f32),
}

/// Milliseconds since the Unix epoch, for project timestamps.
fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// The project model's template for a menu template.
const fn doc_template(t: Template) -> DocTemplate {
    match t {
        Template::Tee => DocTemplate::Tee,
        Template::Dress => DocTemplate::Dress,
        Template::Skirt => DocTemplate::Skirt,
        Template::Labadeh => DocTemplate::Labadeh,
        Template::Aba => DocTemplate::Aba,
    }
}

/// A new project drafted from `template`.
#[must_use]
pub fn new_project(template: Template) -> Project {
    let t = doc_template(template);
    azam_doc::from_template(t, t.sex().unwrap_or(Sex::Female), Some(t.name()), None, now_ms())
}

/// Where projects are saved by default.
fn default_path(project: &Project) -> String {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let name = if project.meta.name.is_empty() {
        "untitled"
    } else {
        project.meta.name.as_str()
    };
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    home.join(format!("{slug}.azam.json")).display().to_string()
}

impl AzamApp {
    /// A new window's state, starting on the T-shirt template.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let mut session = Session::new(new_project(Template::Tee));
        session.selection.active = session.project.pieces.keys().next().cloned();
        if let Some(id) = session.selection.active.clone() {
            session.selection.pieces.insert(id);
        }
        let mut menus = top_menus();
        let top_menu_count = menus.len();
        menus.extend(editor_menus());
        let context_menu_start = menus.len();
        menus.extend(crate::operators::context_menus());
        let editor_menu_start = menus.len();
        // one menu slot for the editor-type menus, filled for an area when its button is clicked
        menus.push(crate::editors::editor_menu(0));
        Self {
            menus,
            top_menu_count,
            context_menu_start,
            editor_menu_start,
            layouts: crate::areas::Layouts::new(),
            next_area_id: 0,
            open_menu: None,
            menu_title_bounds: Vec::new(),
            menu_at_press: None,
            resizing: None,
            piece_drag: None,
            show_arrangement: false,
            fabric_library: None,
            tool_labels: false,
            show_sidebar: false,
            last_op: None,
            modal: None,
            prefs: crate::prefs::Prefs::default(),
            prefs_path: None,
            prefs_view: None,
            area_bounds: std::collections::BTreeMap::new(),
            menu_anchor: (0.0, 0.0),
            workspace: Workspace::Design,
            tool: Tool::Select,
            shading: Shading::Solid,
            fit_mode: azam_sim::FitMode::Fit,
            playing: false,
            viewport: Viewport::new(),
            icons: IconCache::default(),
            search: None,
            pie: None,
            file_prompt: None,
            join_prompt: None,
            collab_marks: Vec::new(),
            cursor_cast: None,
            frames: 0,
            status: String::from("Ready"),
            focused_once: false,
            session,
            prop_tab: PropTab::Piece,
            pattern_view: PatternView::default(),
            pattern_scene: PatternScene::default(),
            pattern_bounds: None,
            pattern_drag: None,
            tools: PatternTools::default(),
            scene_revision: 0,
            framed: false,
            framed_size: None,
            scene3d: Scene3d::default(),
            render_view: None,
            tooltip: None,
            open_submenu: None,
            submenu_anchor: (0.0, 0.0),
        }
    }

    /// The current workspace.
    #[must_use]
    pub fn workspace(&self) -> Workspace {
        self.workspace
    }

    /// Switches to `workspace`.
    pub fn set_workspace(&mut self, workspace: Workspace) {
        self.workspace = workspace;
        self.layout_mut();
        self.framed = false;
        self.scene_revision = 0;
        match workspace {
            Workspace::Review => self.shading = Shading::FitMap,
            Workspace::Grading | Workspace::TechPack => self.prop_tab = PropTab::Project,
            Workspace::Colorway => self.prop_tab = PropTab::Fabric,
            Workspace::Design => {}
        }
    }

    /// Starts or pauses the simulation.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if playing {
            self.scene3d.restart_auto_stop();
        }
    }

    /// Starts a new project from `template`.
    pub fn open_template(&mut self, template: Template) {
        self.session.replace(new_project(template), None);
        self.framed = false;
    }

    /// Opens the project file at `path`.
    ///
    /// # Errors
    /// Returns why the file could not be read or is not a project.
    pub fn open_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        let project = Session::load(path)?;
        self.session.replace(project, Some(path.to_path_buf()));
        self.framed = false;
        self.status = format!("Opened {}", path.display());
        Ok(())
    }

    /// Applies `f` to the project as one undoable edit named `label` (for tests and tools).
    pub fn edit_project(&mut self, label: &str, f: impl FnOnce(&mut azam_doc::Project)) {
        self.session.edit(label, f);
    }

    /// The open project (for tests and tools).
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The pattern editor's pan and zoom.
    #[must_use]
    pub fn pattern_view(&self) -> PatternView {
        self.pattern_view
    }

    /// The pattern editor's scene as last drawn.
    #[must_use]
    pub fn pattern_scene(&self) -> &PatternScene {
        &self.pattern_scene
    }

    /// How many menus the top bar has; the editors' header menus follow them (the 3D view's View
    /// menu is `menu-{top_menu_count}`).
    #[must_use]
    pub fn top_menu_count(&self) -> usize {
        self.top_menu_count
    }

    /// Where the open pie menu is centred, if one is open.
    #[must_use]
    pub fn pie_centre(&self) -> Option<(f32, f32)> {
        self.pie.as_ref().map(|p| p.at)
    }

    /// The operator search's query, while the search is open.
    #[must_use]
    pub fn search_query(&self) -> Option<&str> {
        self.search.as_deref()
    }

    /// Simulated time in seconds, once a simulation exists.
    #[must_use]
    pub fn sim_time(&self) -> Option<f64> {
        self.scene3d.sim.as_ref().map(|s| s.time)
    }

    /// Whether every particle position is finite.
    #[must_use]
    pub fn sim_is_finite(&self) -> bool {
        self.scene3d
            .sim
            .as_ref()
            .is_none_or(|s| s.particles.x.iter().all(|v| v.is_finite()))
    }

    /// The status-bar message.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The title of the open menu, if any.
    #[must_use]
    pub fn open_menu_title(&self) -> Option<&'static str> {
        self.open_menu.map(|i| self.menus[i].title)
    }

    /// Frames built so far (an idle window builds none).
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frames
    }

    /// One line on the simulation's state, for the status bar and the properties editor.
    pub(crate) fn sim_status(&self) -> String {
        if self.scene3d.building() {
            return "Building avatar…".into();
        }
        let Some(sim) = &self.scene3d.sim else {
            return "No simulation".into();
        };
        if sim.frames == 0 && !self.playing {
            return format!(
                "Ready on {} · {} particles · press Space to simulate",
                self.scene3d.solver_name(),
                self.scene3d.particle_count()
            );
        }
        let state = if self.playing { "Simulating" } else { "Paused" };
        let state = format!("{state} on {}", self.scene3d.solver_name());
        let fit = self
            .scene3d
            .fit_summary
            .map(|f| {
                format!(
                    " · tight {:.0}% · over {:.0}%",
                    f.tight * 100.0,
                    f.over_limit * 100.0
                )
            })
            .unwrap_or_default();
        format!(
            "{state} · {:.1} s · {} particles · {:.1} ms/frame{fit}",
            sim.time,
            self.scene3d.particle_count(),
            self.scene3d.step_ms
        )
    }

    // -- operators ---------------------------------------------------------------------------

    /// Runs `op`.
    #[allow(clippy::too_many_lines, reason = "one arm per operator")]
    pub(crate) fn run(&mut self, op: Operator, window: &mut Window, cx: &mut Context<Self>) {
        let from_menu = self.open_menu.is_some();
        self.open_menu = None;
        self.pie = None;
        match op {
            Operator::New(t) => {
                self.session.replace(new_project(t), None);
                self.framed = false;
                self.status = format!("New {} project", t.label());
            }
            Operator::Open => self.ask_path(FileAction::Open, window),
            Operator::Save if self.session.path.is_some() => {
                let path = self.session.path.clone().unwrap_or_default();
                self.status = match self.session.save(&path) {
                    Ok(()) => format!("Saved {}", path.display()),
                    Err(e) => e,
                };
            }
            Operator::Save | Operator::SaveAs => self.ask_path(FileAction::Save, window),
            Operator::Share | Operator::ShareFile | Operator::Join | Operator::StopSharing => {
                self.run_share(op, window, cx);
            }
            Operator::Undo => {
                self.status = self
                    .session
                    .undo()
                    .map_or_else(|| "Nothing to undo".into(), |l| format!("Undo {l}"));
            }
            Operator::Redo => {
                self.status = self
                    .session
                    .redo()
                    .map_or_else(|| "Nothing to redo".into(), |l| format!("Redo {l}"));
            }
            Operator::SelectAll => {
                let project = &self.session.project;
                self.session.selection.toggle_all(project);
            }
            Operator::Delete => self.delete_selected(),
            Operator::Duplicate => self.duplicate_selected(DuplicateMode::Copy, "Duplicate Pieces"),
            Operator::Mirror => {
                self.duplicate_selected(DuplicateMode::Mirror { linked: true }, "Mirror Pieces");
            }
            Operator::DetectHems => {
                let mut marked = 0;
                self.session
                    .edit("Detect Hems", |p| marked = azam_doc::mark_hems(p));
                self.status = format!("Marked {marked} hem edge(s)");
            }
            Operator::Search => {
                self.search = Some(String::new());
                window.focus_id("search-input");
            }
            Operator::ShadingPie => self.open_pie(PieKind::Shading, from_menu, window),
            Operator::ViewPie => self.open_pie(PieKind::View, from_menu, window),
            Operator::SetTool(tool) => {
                self.tool = tool;
                self.cancel_pattern_tool();
            }
            Operator::FlipHorizontal
            | Operator::FlipVertical
            | Operator::Rotate90
            | Operator::FlipSeam
            | Operator::SwapSeamSides
            | Operator::FinishDraft => self.run_pattern_op(op),
            Operator::SetShading(shading) => self.shading = shading,
            Operator::SetFitMap(mode) => self.set_fit_mode(mode),
            Operator::SetEditor(id, editor) => self.set_editor(id, editor),
            Operator::SplitArea(id, dir) => {
                self.split_area(id, dir);
            }
            Operator::JoinArea(id) => {
                if !self.join_area(id) {
                    self.status = "That area fills the window alone".into();
                }
            }
            Operator::View(preset) => self.viewport.camera.set_preset(preset),
            Operator::ToggleOrtho => self.viewport.camera.orthographic = !self.viewport.camera.orthographic,
            Operator::ToggleGrid => self.viewport.overlays.grid = !self.viewport.overlays.grid,
            Operator::ToggleWireframe => {
                self.viewport.overlays.wireframe = !self.viewport.overlays.wireframe;
            }
            Operator::ToggleArrangementPoints => self.show_arrangement = !self.show_arrangement,
            Operator::ToggleSidebar => self.toggle_sidebar(),
            Operator::ToggleCurvePoint => self.toggle_curve_points(),
            Operator::ToggleToolLabels => {
                self.toggle_tool_labels();
                self.prefs.tool_labels = self.tool_labels;
                self.save_prefs();
            }
            Operator::Preferences => self.prefs_view = Some(crate::prefs::PrefsView::default()),
            Operator::ToggleAvatar => self
                .session
                .edit("Show Avatar", |p| p.avatar.show = !p.avatar.show),
            Operator::FramePattern => self.frame_pattern(),
            Operator::FrameAll => self.frame_all(),
            Operator::TogglePlay | Operator::ResetSim | Operator::StepSim => self.run_simulation(op),
            Operator::ExportDxf
            | Operator::ExportSvg
            | Operator::ExportObj
            | Operator::ExportGltf
            | Operator::ExportPatternPdf
            | Operator::ExportTechPackPdf => self.export(ExportKind::for_operator(op)),
            Operator::ImportDxf => self.import_file(crate::import::ImportFormat::Dxf),
            Operator::ImportSvg => self.import_file(crate::import::ImportFormat::Svg),
            Operator::ImportAvatar => self.import_avatar(),
            Operator::UseBuiltinBody => self.set_custom_body(None),
            Operator::Quit => std::process::exit(0),
            Operator::Shortcuts => {
                self.status =
                    "MMB orbit · Shift MMB pan · Wheel zoom · Z shading pie · ` view pie · F3 search · Space play".into();
            }
            Operator::RenderImage => self.start_render(),
            Operator::About => self.status = format!("azam {} — GPL-3.0-or-later", env!("CARGO_PKG_VERSION")),
        }
        cx.notify();
    }

    fn run_simulation(&mut self, op: Operator) {
        match op {
            Operator::TogglePlay => self.set_playing(!self.playing),
            Operator::ResetSim => {
                self.playing = false;
                if let Some(renderer) = self.viewport.renderer() {
                    self.scene3d.reset(renderer);
                }
                self.status = "Reset the garment to its arranged pose".into();
            }
            _ => {
                if let Some(renderer) = self.viewport.renderer() {
                    self.scene3d.step(&self.session.project, renderer);
                }
            }
        }
    }

    fn selected_pieces(&self) -> Vec<PieceId> {
        self.session.selection.pieces.iter().cloned().collect()
    }

    fn delete_selected(&mut self) {
        if self.delete_selection() {
            return;
        }
        let ids = self.selected_pieces();
        if ids.is_empty() {
            self.status = "Nothing selected".into();
            return;
        }
        self.session
            .edit("Delete Pieces", |p| azam_doc::delete_pieces(p, &ids));
        self.status = format!("Deleted {} piece(s)", ids.len());
    }

    fn duplicate_selected(&mut self, mode: DuplicateMode, label: &str) {
        let ids = self.selected_pieces();
        if ids.is_empty() {
            self.status = "Nothing selected".into();
            return;
        }
        let mut created = Vec::new();
        self.session.edit(label, |p| {
            created = ids
                .iter()
                .filter_map(|id| azam_doc::duplicate_piece(p, id, mode))
                .collect();
        });
        self.session.selection.pieces = created.iter().cloned().collect();
        self.session.selection.active = created.first().cloned();
        self.status = format!("{label}: {}", created.len());
    }

    /// Asks for a path with Blender's File Browser, or the typed prompt where no native window
    /// can host it.
    fn ask_path(&mut self, action: FileAction, window: &mut Window) {
        let current = self
            .session
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from(default_path(&self.session.project)));
        let (title, mode) = match action {
            FileAction::Open => ("Open Project", blenderui::FileDialogMode::Open),
            FileAction::Save => ("Save Project", blenderui::FileDialogMode::Save),
        };
        let name = current.file_name().map(|n| n.to_string_lossy().into_owned());
        match blenderui::file_dialog(
            title,
            mode,
            current.parent(),
            name.as_deref(),
            &[PROJECT_FILTER],
            false,
        ) {
            blenderui::FileDialogResult::Chosen(paths) => {
                if let Some(path) = paths.into_iter().next() {
                    self.file_prompt = Some(FilePrompt {
                        action,
                        path: path.display().to_string(),
                    });
                    self.confirm_file(window);
                }
            }
            blenderui::FileDialogResult::Cancelled => self.status = "Cancelled".into(),
            blenderui::FileDialogResult::Unavailable => {
                self.file_prompt = Some(FilePrompt {
                    action,
                    path: current.display().to_string(),
                });
                window.focus_id("file-input");
            }
        }
    }

    fn confirm_file(&mut self, window: &mut Window) {
        let Some(prompt) = self.file_prompt.take() else {
            return;
        };
        let path = PathBuf::from(prompt.path.trim());
        window.focus_id("root");
        match prompt.action {
            FileAction::Open => match Session::load(&path) {
                Ok(project) => {
                    self.session.replace(project, Some(path.clone()));
                    self.framed = false;
                    self.status = format!("Opened {}", path.display());
                }
                Err(e) => self.status = e,
            },
            FileAction::Save => {
                self.status = match self.session.save(&path) {
                    Ok(()) => format!("Saved {}", path.display()),
                    Err(e) => e,
                };
            }
        }
    }

    fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &e.keystroke;
        // the Preferences window takes keys first (a shortcut being set, Esc), and while it is
        // open no shortcut runs (typing goes to its filter)
        if self.prefs_view.is_some() {
            self.prefs_key(key);
            if self.prefs_view.is_none() {
                // closed: keys go back to the window, not the filter field that went with it
                window.focus_id("root");
            }
            cx.notify();
            return;
        }
        if key.key == "Escape" && self.modal.is_some() {
            self.cancel_modal();
            cx.notify();
            return;
        }
        if key.key == "Escape" {
            self.render_view = None;
            self.search = None;
            self.pie = None;
            self.open_menu = None;
            self.file_prompt = None;
            self.join_prompt = None;
            self.cancel_pattern_tool();
            window.focus_id("root");
            cx.notify();
            return;
        }
        if self.file_prompt.is_some() {
            if key.key == "Return" {
                self.confirm_file(window);
                cx.notify();
            }
            return;
        }
        if self.join_prompt.is_some() {
            if key.key == "Return" {
                self.confirm_join(window, cx);
                cx.notify();
            }
            return;
        }
        if self.search.is_some() {
            if key.key == "Return"
                && let Some(op) = self.search_results().first().map(|(op, _)| *op)
            {
                self.search = None;
                window.focus_id("root");
                self.run(op, window, cx);
            }
            return;
        }
        // a modal transform takes Enter and Esc; G and R start one over the pattern editor
        if self.modal.is_some() {
            match key.key.to_ascii_lowercase().as_str() {
                "escape" => self.cancel_modal(),
                "return" | "kp_enter" => self.confirm_modal(),
                "x" => self.toggle_modal_axis(crate::modal::Axis::X),
                "y" => self.toggle_modal_axis(crate::modal::Axis::Y),
                k => {
                    let ch = key.key_char.as_deref().and_then(|c| c.chars().next());
                    self.modal_type(ch, k == "backspace");
                }
            }
            cx.notify();
            return;
        }
        // the user's own shortcuts come before the built-in keys
        if self.custom_shortcut(key, window, cx) {
            cx.notify();
            return;
        }
        if !key.modifiers.control && !key.modifiers.alt && !key.modifiers.shift {
            let kind = match key.key.to_ascii_lowercase().as_str() {
                "g" => Some(crate::modal::ModalKind::Move),
                "r" => Some(crate::modal::ModalKind::Rotate),
                "s" => Some(crate::modal::ModalKind::Scale),
                _ => None,
            };
            if let Some(kind) = kind
                && self.begin_modal(kind, window.mouse_position())
            {
                cx.notify();
                return;
            }
        }
        // N over the 3D view toggles its sidebar (Blender's per-editor keymap); elsewhere it is
        // the Sew tool
        let over_3d = self.viewport.bounds.is_some_and(|b| {
            let p = window.mouse_position();
            p.x.0 >= b.origin.x
                && p.x.0 < b.origin.x + b.size.width
                && p.y.0 >= b.origin.y
                && p.y.0 < b.origin.y + b.size.height
        });
        if over_3d && key.key.eq_ignore_ascii_case("n") && !key.modifiers.shift && !key.modifiers.control {
            self.run(Operator::ToggleSidebar, window, cx);
            return;
        }
        if let Some(op) = Operator::from_key(key) {
            self.run(op, window, cx);
        }
    }

    fn frame_all(&mut self) {
        let (lo, hi) = self.scene3d.points().fold(
            (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)),
            |(lo, hi), p| (lo.min(p), hi.max(p)),
        );
        if lo.x <= hi.x {
            self.viewport.camera.frame(lo, hi);
        }
    }

    pub(crate) fn frame_pattern(&mut self) {
        if let (Some((lo, hi)), Some(b)) = (self.pattern_scene.bounds(), self.pattern_bounds) {
            let size = azam_geom::Vec2::new(f64::from(b.size.width), f64::from(b.size.height));
            let shelf = if self.tool_labels {
                crate::pattern::LABELLED_SHELF_PX
            } else {
                crate::pattern::TOOL_SHELF_PX
            };
            self.pattern_view.frame(lo, hi, size, shelf);
            self.framed_size = Some((b.size.width, b.size.height));
        }
    }

    // -- viewport input ------------------------------------------------------------------------

    fn on_viewport_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if e.button == MouseButton::Left
            && let Some(axis) = self.viewport.gizmo_hit(e.position)
        {
            if let Some(view) = axis.view {
                self.viewport.camera.set_preset(view);
                cx.notify();
            }
            return;
        }
        if e.button == MouseButton::Left && matches!(self.tool, Tool::Grab | Tool::Pin) {
            let hit = self
                .viewport
                .ray(e.position)
                .and_then(|(o, d)| self.scene3d.hit(o, d));
            if self.tool == Tool::Grab
                && let (Some((_, particle, depth)), Some(sim)) = (hit, self.scene3d.sim.as_mut())
                && sim.grab_at(particle)
            {
                self.scene3d.grab = Some(GrabState { depth });
                if !self.playing {
                    self.playing = true;
                    self.scene3d.restart_auto_stop();
                }
            }
            if self.tool == Tool::Pin
                && let Some((_, particle, _)) = hit
            {
                self.toggle_pin(particle);
            }
            cx.notify();
            return;
        }
        if e.button == MouseButton::Left && matches!(self.tool, Tool::Select) {
            let hit = self
                .viewport
                .ray(e.position)
                .and_then(|(origin, dir)| self.scene3d.pick(origin, dir));
            match hit {
                Some(id) if e.modifiers.shift => self.session.selection.toggle(id),
                Some(id) => {
                    self.session.selection.select_only(id);
                    self.begin_piece_drag(e);
                }
                None if !e.modifiers.shift => self.session.selection.clear(),
                None => {}
            }
            self.scene_revision = 0;
            cx.notify();
            return;
        }
        if e.button == MouseButton::Middle {
            self.viewport.begin_navigate(e.position, e.modifiers.shift);
            cx.notify();
        }
        if e.button == MouseButton::Right {
            self.open_context_menu(crate::operators::ContextMenu::Viewport, e.position);
            cx.notify();
        }
    }

    /// Opens pie `kind`: under the pointer for a key press over the 3D view, else (chosen from a
    /// menu, or the pointer elsewhere) in the middle of the view.
    fn open_pie(&mut self, kind: PieKind, from_menu: bool, window: &Window) {
        let p = window.mouse_position();
        let at = match self.viewport.bounds {
            Some(b) if from_menu || !contains(&b, p) => {
                (b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0)
            }
            _ => (p.x.0, p.y.0),
        };
        self.pie = Some(Pie { kind, at });
    }

    /// Pins the cloth at particle `particle`, or unpins it if a pin is there. Pins are stored on
    /// the piece, so they stay through resets and are saved with the project; the simulation is
    /// rebuilt from its current state, so the drape is kept.
    fn toggle_pin(&mut self, particle: usize) {
        let Some((piece, x, y)) = self.scene3d.particle_pattern_pos(particle) else {
            return;
        };
        let at = self
            .scene3d
            .sim
            .as_ref()
            .map(|sim| sim.particles.position(particle).map(f64::from));
        let reach = self
            .session
            .project
            .pieces
            .get(&piece)
            .map_or(10.0, |pc| pc.pd * 0.6);
        let existing = self.session.project.pieces.get(&piece).and_then(|pc| {
            pc.pins
                .iter()
                .position(|pin| (pin.x - x).hypot(pin.y - y) <= reach)
        });
        if let Some(index) = existing {
            self.session.edit("Unpin Cloth", |p| {
                if let Some(pc) = p.pieces.get_mut(&piece) {
                    pc.pins.remove(index);
                }
            });
            self.status = "Unpinned".into();
        } else {
            self.session.edit("Pin Cloth", |p| {
                if let Some(pc) = p.pieces.get_mut(&piece) {
                    pc.pins.push(azam_doc::Pin {
                        x,
                        y,
                        at,
                        extra: serde_json::Map::new(),
                    });
                }
            });
            self.status = "Pinned (saved with the project)".into();
        }
    }

    fn on_mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.drag_border(e.position) {
            cx.notify();
            return;
        }
        if self.tooltip_left_behind(e.position) {
            cx.notify();
        }
        self.publish_cursor(e.position);
        if self.piece_drag_move(e.position) || self.modal_scale_move(e.position) {
            cx.notify();
            return;
        }
        let mut changed = self.viewport.navigate(e.position);
        changed |= self.pattern_drag_move(e.position, e.modifiers);
        if let Some(grab) = self.scene3d.grab
            && let Some((origin, dir)) = self.viewport.ray(e.position)
            && let Some(sim) = self.scene3d.sim.as_mut()
        {
            let target = origin + dir * grab.depth;
            sim.set_grab_target([f64::from(target.x), f64::from(target.y), f64::from(target.z)]);
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let mut ended = self.viewport.drag.take().is_some()
            | self.pattern_drag_end()
            | self.resizing.take().is_some()
            | self.end_piece_drag();
        if self.scene3d.grab.take().is_some() {
            if let Some(sim) = self.scene3d.sim.as_mut() {
                sim.release_grab();
            }
            ended = true;
        }
        if ended {
            cx.notify();
        }
    }

    fn on_viewport_wheel(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.viewport.camera.dolly(e.delta);
        cx.notify();
    }

    // -- top bar -------------------------------------------------------------------------------

    fn topbar(&mut self, cx: &mut Context<Self>) -> Div {
        let mut bar = div()
            .class("topbar")
            .child(
                div()
                    .class("logo")
                    .child(self.icons.sized("BLENDER", 18.0, Some(0xe87d0d))),
            );
        for i in 0..self.top_menu_count {
            bar = bar.child(self.menu_title(i, cx));
        }
        let mut tabs = div().class("workspaces");
        for ws in Workspace::ALL {
            let class = if ws == self.workspace {
                "ws-tab active"
            } else {
                "ws-tab"
            };
            tabs = tabs.child(div().class(class).child(ws.label()).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.set_workspace(ws);
                    cx.notify();
                },
            )));
        }
        let title = format!(
            "{}{}",
            if self.session.project.meta.name.is_empty() {
                "Untitled"
            } else {
                &self.session.project.meta.name
            },
            if self.session.is_dirty() { " •" } else { "" }
        );
        bar.child(tabs)
            .child(div().class("topbar-spacer"))
            .child(div().class("topbar-info").child(title))
    }

    /// The menu whose title lies under `p`, while a menu is open.
    fn menu_title_at(&self, p: Point<Pixels>) -> Option<usize> {
        self.menu_title_bounds
            .iter()
            .position(|b| b.is_some_and(|b| contains(&b, p)))
    }

    /// A menu's title: click opens it, hovering moves between open menus (as in Blender).
    fn menu_title(&self, i: usize, cx: &mut Context<Self>) -> Div {
        let open = self.open_menu == Some(i);
        div()
            .class(if open { "menu-title open" } else { "menu-title" })
            .id(format!("menu-{i}"))
            .child(self.menus[i].title)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.open_menu = if this.open_menu == Some(i) { None } else { Some(i) };
                cx.notify();
            }))
            .on_hover(cx.listener(move |this, entered: &bool, _, cx| {
                if *entered && this.open_menu.is_some() && this.open_menu != Some(i) {
                    this.open_menu = Some(i);
                    cx.notify();
                }
            }))
    }

    fn dropdown(&mut self, index: usize, cx: &mut Context<Self>) -> Div {
        let (left, top) = self.menu_anchor;
        let mut menu = div()
            .class("menu")
            .css("left", format!("{left}px"))
            .css("top", format!("{top}px"));
        let entries = self.menus[index].entries.clone();
        let mut sub = None;
        for entry in entries {
            menu = match entry {
                MenuEntry::Separator => menu.child(div().class("menu-sep")),
                MenuEntry::Item(op) => menu.child(self.menu_item(op, cx)),
                MenuEntry::Sub(title, icon, items) => {
                    if self.open_submenu == Some(title) {
                        sub = Some(items);
                    }
                    menu.child(self.submenu_title(title, icon, cx))
                }
            };
        }
        if let Some(items) = sub {
            let (left, top) = self.submenu_anchor;
            let mut panel = div()
                .class("menu")
                .css("left", format!("{left}px"))
                .css("top", format!("{top}px"));
            for &op in items {
                panel = panel.child(self.menu_item(op, cx));
            }
            return div().child(menu).child(panel);
        }
        menu
    }

    /// A submenu's row: hovering it opens the submenu beside the menu, as in Blender.
    fn submenu_title(&mut self, title: &'static str, icon: &'static str, cx: &mut Context<Self>) -> Div {
        let open = self.open_submenu == Some(title);
        div()
            .class(if open { "menu-item open" } else { "menu-item" })
            .id(format!("submenu-{title}"))
            .child(self.icons.icon(icon))
            .child(div().class("menu-label").child(title))
            .child(self.icons.sized("RIGHTARROW", 12.0, None))
            .on_hover(cx.listener(move |this, entered: &bool, _, cx| {
                if *entered && this.open_submenu != Some(title) {
                    this.open_submenu = Some(title);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.open_submenu = Some(title);
                cx.notify();
            }))
    }

    fn menu_item(&mut self, op: Operator, cx: &mut Context<Self>) -> Div {
        self.menu_item_in(op, None, cx)
    }

    /// A menu row for `op`; search results also show the menu `path` it lives in.
    fn menu_item_in(&mut self, op: Operator, path: Option<String>, cx: &mut Context<Self>) -> Div {
        let icon = match op.icon() {
            Some(name) => self.icons.icon(name),
            None => div().class("menu-icon"),
        };
        div()
            .class("menu-item")
            .id(format!("op-{}", op.label()))
            .child(icon)
            .when_some(path, |el, p| {
                el.child(div().class("menu-path").child(format!("{p} ▸")))
            })
            .child(div().class("menu-label").child(op.label()))
            .when_some(self.shortcut_label(op), |el, s| {
                el.child(div().class("menu-shortcut").child(s))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.search = None;
                this.run(op, window, cx);
            }))
    }

    // -- editors -------------------------------------------------------------------------------

    fn header_button(&mut self, icon: &'static str, on: bool, op: Operator, cx: &mut Context<Self>) -> Div {
        div()
            .class(if on { "hbtn on" } else { "hbtn" })
            .child(self.icons.icon(icon))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.run(op, window, cx)))
    }

    /// Whether a menu, the operator search, a file prompt or a pie covers part of the window.
    fn overlay_open(&self) -> bool {
        self.open_menu.is_some()
            || self.search.is_some()
            || self.file_prompt.is_some()
            || self.pie.is_some()
            || self.prefs_view.is_some()
    }

    /// The Dart tool's settings in the pattern editor's header, as Blender shows a tool's
    /// settings: the width and depth of the next dart.
    fn dart_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let size = self.tools.dart;
        if self.overlay_open() {
            return div().class("hgroup").child(
                div()
                    .class("hbtn")
                    .child(format!("Width {:.0} · Depth {:.0}", size.width, size.depth)),
            );
        }
        let field = |label: &'static str,
                     value: f64,
                     max: f64,
                     set: fn(&mut crate::pattern_tools::DartSize, f64),
                     cx: &mut Context<Self>| {
            NumberField()
                .attr("label", label)
                .value(format!("{value:.0}"))
                .min(1.0)
                .max(max)
                .step(1.0)
                .css("width", "112px")
                .on_input(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                    set(&mut this.tools.dart, e.number.clamp(1.0, max));
                    cx.notify();
                }))
        };
        div()
            .class("hgroup tool-settings")
            .child(field("Width", size.width, 200.0, |d, v| d.width = v, cx))
            .child(field("Depth", size.depth, 400.0, |d, v| d.depth = v, cx))
    }

    /// The Fillet tool's radius, in the tool settings bar.
    fn fillet_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let radius = self.tools.fillet.radius;
        if self.overlay_open() {
            return div()
                .class("hgroup")
                .child(div().class("hbtn").child(format!("Radius {radius:.0}")));
        }
        div().class("hgroup tool-settings").child(
            NumberField()
                .attr("label", "Radius")
                .value(format!("{radius:.0}"))
                .min(1.0)
                .max(300.0)
                .step(1.0)
                .css("width", "112px")
                .on_input(cx.listener(|this, e: &ChangeEvent, _, cx| {
                    this.tools.fillet.radius = e.number.clamp(1.0, 300.0);
                    cx.notify();
                })),
        )
    }

    /// Which fit map the fit map shading shows (Fit, Strain, Stress, Pressure).
    fn fit_dropdown(&mut self, cx: &mut Context<Self>) -> Div {
        let current = self.fit_mode;
        if self.overlay_open() {
            return div().class("hgroup").child(
                div()
                    .class("hbtn")
                    .w(px(84.0))
                    .child(crate::operators::fit_label(current)),
            );
        }
        let mut select = Dropdown().css("width", "84px");
        for m in crate::operators::FIT_MODES {
            select = select.child(
                option()
                    .child(crate::operators::fit_label(m))
                    .selected(m == current),
            );
        }
        div()
            .class("hgroup")
            .child(select.on_change(cx.listener(|this, e: &ChangeEvent, _, cx| {
                if let Some(&m) = crate::operators::FIT_MODES.get(e.number.max(0.0) as usize) {
                    this.set_fit_mode(m);
                }
                cx.notify();
            })))
    }

    /// The legend of the fit map being shown, in the corner of the 3D view.
    fn fit_legend(&self) -> Div {
        let (title, rows): (&str, Vec<(u32, &str)>) = match self.fit_mode {
            azam_sim::FitMode::Fit => (
                "Fit",
                vec![
                    (0xf2f2f2, "Comfortable"),
                    (0xfacc33, "Tight (60% of limit)"),
                    (0xd9262c, "Over the limit"),
                ],
            ),
            azam_sim::FitMode::Strain => (
                "Strain",
                vec![(0x2b4bd9, "100% (relaxed)"), (0xd9262c, "120% or more")],
            ),
            azam_sim::FitMode::Stress => ("Stress", vec![(0x2b4bd9, "0 kPa"), (0xd9262c, "100 kPa or more")]),
            azam_sim::FitMode::Pressure => (
                "Pressure",
                vec![(0xededed, "No contact"), (0xf2402f, "Touching the body")],
            ),
        };
        let mut legend = div()
            .class("fit-legend")
            .child(div().class("fit-legend-title").child(title));
        for (color, label) in rows {
            legend = legend.child(
                div()
                    .class("fit-legend-row")
                    .child(
                        div()
                            .class("fit-swatch")
                            .css("background", format!("#{color:06x}")),
                    )
                    .child(label),
            );
        }
        legend
    }

    /// Shows fit map `mode` in the fit map shading (switching to it).
    pub fn set_fit_mode(&mut self, mode: azam_sim::FitMode) {
        self.fit_mode = mode;
        self.shading = Shading::FitMap;
        self.status = format!("Fit map: {}", crate::operators::fit_label(mode));
    }

    /// The fit map the fit map shading shows.
    #[must_use]
    pub fn fit_mode(&self) -> azam_sim::FitMode {
        self.fit_mode
    }

    /// The 3D view's mode, as Blender's mode selector: Garment (select, grab and pin cloth) or
    /// Arrange (the avatar's arrangement points are shown for placing pieces).
    fn mode_dropdown(&mut self, cx: &mut Context<Self>) -> Div {
        const MODES: [&str; 2] = ["Garment Mode", "Arrange Mode"];
        let current = usize::from(self.show_arrangement);
        if self.overlay_open() {
            return div()
                .class("hgroup")
                .child(div().class("hbtn").w(px(112.0)).child(MODES[current]));
        }
        let mut select = Dropdown().css("width", "112px");
        for (i, m) in MODES.into_iter().enumerate() {
            select = select.child(option().child(m).selected(i == current));
        }
        div()
            .class("hgroup")
            .child(select.on_change(cx.listener(|this, e: &ChangeEvent, _, cx| {
                this.show_arrangement = e.number >= 1.0;
                if this.show_arrangement {
                    this.tool = Tool::Select;
                    this.status =
                    "Arrange: click an arrangement point to send the selected pieces there, or drag a piece"
                        .into();
                }
                cx.notify();
            })))
    }

    /// The avatar motion played with the simulation (None, Raise Arms, Walk, Sit, Turntable).
    fn motion_dropdown(&mut self, cx: &mut Context<Self>) -> Div {
        let current = self.scene3d.motion.motion();
        // Native widgets draw above page content, so while a menu, search or pie covers the
        // header the control shows as a plain label of the same size.
        if self.overlay_open() {
            return div()
                .class("hgroup")
                .child(div().class("hbtn").w(px(96.0)).child(motion_label(current)));
        }
        let mut select = Dropdown().css("width", "96px");
        for m in azam_avatar::Motion::ALL {
            select = select.child(option().child(motion_label(m)).selected(m == current));
        }
        let group =
            div()
                .class("hgroup")
                .child(select.on_change(cx.listener(|this, e: &ChangeEvent, _, cx| {
                    let i = e.number.max(0.0) as usize;
                    if let Some(&m) = azam_avatar::Motion::ALL.get(i) {
                        this.scene3d.set_motion(m);
                        this.status = format!("Avatar motion: {}", motion_label(m));
                    }
                    cx.notify();
                })));
        Self::with_tooltip(
            group,
            "Avatar Motion\tplayed while the garment simulates".into(),
            cx,
        )
    }

    pub(crate) fn viewport_area(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        let show_avatar = self.session.project.avatar.show;
        // a narrow 3D view leaves out the header's less used controls (they stay in the menus)
        let width = self.area_width(area);
        let (roomy, wide) = (width >= NARROW_HEADER_PX, width >= COMPACT_HEADER_PX);
        let header = div()
            .class("area-header")
            .child(self.editor_type_button(area, cx))
            .child(self.mode_dropdown(cx))
            .child(self.menu_title(self.top_menu_count, cx))
            .child(self.menu_title(self.top_menu_count + 1, cx))
            .child(div().class("header-spacer"))
            .when(roomy, |el| {
                el.child(self.header_button("ARMATURE_DATA", show_avatar, Operator::ToggleAvatar, cx))
                    .child(self.header_button("GRID", self.viewport.overlays.grid, Operator::ToggleGrid, cx))
                    .child(self.header_button(
                        "SHADING_WIRE",
                        self.viewport.overlays.wireframe,
                        Operator::ToggleWireframe,
                        cx,
                    ))
                    .child(div().class("header-sep"))
            })
            .child(
                div()
                    .class("hgroup")
                    .child(
                        self.header_button(
                            "SHADING_SOLID",
                            self.shading == Shading::Solid,
                            Operator::SetShading(Shading::Solid),
                            cx,
                        )
                        .class("first"),
                    )
                    .child(
                        self.header_button(
                            "SHADING_TEXTURE",
                            self.shading == Shading::FitMap,
                            Operator::SetShading(Shading::FitMap),
                            cx,
                        )
                        .class("last"),
                    ),
            )
            .when(self.shading == Shading::FitMap && roomy, |el| {
                el.child(self.fit_dropdown(cx))
            })
            .child(div().class("header-sep"))
            .when(wide, |el| {
                el.child(self.motion_dropdown(cx))
                    .child(div().class("header-sep"))
            })
            .child(self.header_button("FILE_REFRESH", false, Operator::ResetSim, cx))
            .child(self.header_button(
                if self.playing { "PAUSE" } else { "PLAY" },
                self.playing,
                Operator::TogglePlay,
                cx,
            ));
        let projection = if self.viewport.camera.orthographic {
            "User Orthographic"
        } else {
            "User Perspective"
        };
        let badge = self.sim_status();
        let body = div()
            .class("area-body")
            .child(self.toolbar(&[&Tool::VIEWPORT], cx))
            .child(
                div().class("vp-text").child(div().child(projection)).child(
                    div()
                        .class("vp-sub")
                        .child(format!("(1) {}", self.session.project.meta.name)),
                ),
            )
            .child(
                self.viewport
                    .canvas()
                    .on_any_mouse_down(cx.listener(Self::on_viewport_down))
                    .on_scroll_wheel(cx.listener(Self::on_viewport_wheel)),
            )
            .when(self.shading == Shading::FitMap, |el| el.child(self.fit_legend()))
            .children(self.arrangement_overlay(cx))
            .children(self.sidebar(cx))
            .child(
                div()
                    .class(if self.playing { "vp-badge live" } else { "vp-badge" })
                    .child(badge),
            )
            .when_some(self.viewport.error().map(str::to_owned), |el, e| {
                el.child(div().class("vp-badge").child(format!("GPU unavailable: {e}")))
            });
        div().class("area").flex_1().child(header).child(body)
    }

    /// A tool shelf: groups of tools with separators between them, as in Blender's toolbar.
    fn toolbar(&mut self, groups: &[&[Tool]], cx: &mut Context<Self>) -> Div {
        let mut shelf = div().class(if self.tool_labels {
            "toolbar labelled"
        } else {
            "toolbar"
        });
        for (i, &tool) in groups
            .iter()
            .enumerate()
            .flat_map(|(i, g)| g.iter().map(move |t| (i, t)))
        {
            if i > 0 && groups[i].first() == Some(&tool) {
                shelf = shelf.child(div().class("tool-sep"));
            }
            let class = if tool == self.tool { "tool active" } else { "tool" };
            let tip = match self.shortcut_label(Operator::SetTool(tool)) {
                Some(key) => format!("{}\t{key}", tool.label()),
                None => tool.label().to_owned(),
            };
            let button = div()
                .class(class)
                .child(self.icons.sized(tool.icon(), 18.0, None))
                .when(self.tool_labels, |el| {
                    el.child(div().class("tool-label").child(tool.label()))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.run(Operator::SetTool(tool), window, cx);
                }));
            shelf = shelf.child(Self::with_tooltip(button, tip, cx));
        }
        shelf
    }

    pub(crate) fn pattern_area(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        let sizes = self.session.project.active_size.0.clone();
        let header = div()
            .class("area-header")
            .child(self.editor_type_button(area, cx))
            .child(div().class("mode-label").child("Pattern"))
            .child(self.menu_title(self.top_menu_count + 2, cx))
            .child(self.menu_title(self.top_menu_count + 3, cx))
            .child(self.menu_title(self.top_menu_count + 4, cx))
            .child(self.menu_title(self.top_menu_count + 5, cx))
            .child(div().class("header-spacer"))
            .child(div().class("mode-label").child(format!("Size {sizes}")))
            .child(self.header_button("ZOOM_ALL", false, Operator::FramePattern, cx));
        let entity = cx.entity();
        let canvas = canvas_with(move |b, p, app| {
            let this = entity.read(app);
            this.paint_pattern(b, p);
        })
        .id(PATTERN_ID)
        .flex_1()
        .h_full()
        .on_any_mouse_down(cx.listener(Self::on_pattern_down))
        .on_scroll_wheel(cx.listener(Self::on_pattern_wheel));
        let hint = self.tool_hint();
        let body = div()
            .class("area-body")
            .child(self.toolbar(&Tool::PATTERN_GROUPS, cx))
            .child(canvas)
            .child(div().class("vp-badge").child(hint))
            .children(self.redo_panel(cx));
        // Blender's tool settings: a bar under the header with the active tool's options
        // always there, so the canvas does not jump when a tool with settings is picked
        let tool_bar = Some(
            div()
                .class("area-header tool-header")
                .child(self.icons.sized(self.tool.icon(), 14.0, None))
                .child(div().class("mode-label").child(self.tool.label()))
                .when(self.tool == Tool::Dart, |el| el.child(self.dart_settings(cx)))
                .when(self.tool == Tool::Fillet, |el| el.child(self.fillet_settings(cx))),
        );
        div()
            .class("area")
            .flex_1()
            .child(header)
            .children(tool_bar)
            .child(body)
    }

    pub(crate) fn outliner_area(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        div()
            .class("area")
            .flex_1()
            .child(
                div()
                    .class("area-header")
                    .child(self.editor_type_button(area, cx))
                    .child(div().class("header-spacer"))
                    .child(self.icons.icon("FILTER")),
            )
            .child(self.outliner_body(area, cx))
    }

    pub(crate) fn properties_area(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        div()
            .class("area")
            .flex_1()
            .child(
                div()
                    .class("area-header")
                    .child(self.editor_type_button(area, cx))
                    .child(div().class("header-spacer")),
            )
            .child(self.properties_body(area, cx))
    }

    pub(crate) fn techpack_area(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        let header = div()
            .class("area-header")
            .child(self.editor_type_button(area, cx))
            .child(div().class("mode-label").child("Tech Pack"))
            .child(div().class("header-spacer"))
            .child(self.header_button("EXPORT", false, Operator::ExportDxf, cx));
        div()
            .class("area")
            .flex_1()
            .child(header)
            .child(self.techpack_body(area))
    }

    fn statusbar(&mut self) -> Div {
        let project = &self.session.project;
        let stats = format!(
            "Pieces {} | Seams {} | Fabrics {} | Viewport {:.1} ms",
            project.pieces.len(),
            project.sewing.len(),
            project.fabrics.len(),
            self.viewport.frame_ms
        );
        div()
            .class("statusbar")
            .child(
                div()
                    .class("status-hint")
                    .child(self.icons.icon("MOUSE_LMB"))
                    .child(self.tool.lmb_hint()),
            )
            .child(
                div()
                    .class("status-hint")
                    .child(self.icons.icon("MOUSE_MMB"))
                    .child("Rotate / Pan View"),
            )
            .child(
                div()
                    .class("status-hint")
                    .child(self.icons.icon("MOUSE_MMB_SCROLL"))
                    .child("Zoom"),
            )
            .child(div().class("status-msg").child(self.status.clone()))
            .child(div().class("status-spacer"))
            .children(self.collab_status().map(|c| {
                div()
                    .class("status-hint")
                    .child(self.icons.icon("COMMUNITY"))
                    .child(c)
            }))
            .child(div().child(stats))
    }

    // -- overlays ------------------------------------------------------------------------------

    fn search_popup(&mut self, cx: &mut Context<Self>) -> Div {
        let query = self.search.clone().unwrap_or_default();
        let mut results = div().class("search-results");
        for (op, path) in self.search_results() {
            results = results.child(self.menu_item_in(op, path, cx));
        }
        div()
            .class("search")
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(
                input()
                    .id("search-input")
                    .attr("autofocus", "")
                    .attr("live", "")
                    .placeholder("Search operators…")
                    .value(query)
                    .w_full()
                    .on_input(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        this.search = Some(e.value.clone());
                        cx.notify();
                    })),
            )
            .child(results)
    }

    fn file_popup(&mut self, prompt: &FilePrompt, cx: &mut Context<Self>) -> Div {
        let title = match prompt.action {
            FileAction::Open => "Open Project",
            FileAction::Save => "Save Project",
        };
        div()
            .class("search")
            .on_click(|_: &ClickEvent, _, cx| cx.stop_propagation())
            .child(
                div()
                    .class("prop-title")
                    .child(self.icons.icon("FILE_FOLDER"))
                    .child(title),
            )
            .child(
                input()
                    .id("file-input")
                    .attr("autofocus", "")
                    .attr("live", "")
                    .value(prompt.path.clone())
                    .w_full()
                    .on_input(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        if let Some(p) = &mut this.file_prompt {
                            p.path.clone_from(&e.value);
                        }
                        cx.notify();
                    })),
            )
            .child(div().class("pbuttons").mt(px(6.0)).child(
                div().class("pbtn primary").child(title).on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| {
                        this.confirm_file(window);
                        cx.notify();
                    },
                )),
            ))
    }

    fn pie_menu(&mut self, pie: Pie, cx: &mut Context<Self>) -> Div {
        // Blender's pie order: west, east, south, north, then the diagonals.
        const DIRECTIONS: [(f32, f32); 8] = [
            (-1.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (0.0, -1.0),
            (-0.7, -0.7),
            (0.7, -0.7),
            (-0.7, 0.7),
            (0.7, 0.7),
        ];
        let (title, items): (&str, Vec<Operator>) = match pie.kind {
            PieKind::Shading => (
                "Shading",
                vec![
                    Operator::SetShading(Shading::Solid),
                    Operator::SetShading(Shading::FitMap),
                    Operator::ToggleAvatar,
                    Operator::ToggleGrid,
                    Operator::SetFitMap(azam_sim::FitMode::Strain),
                    Operator::SetFitMap(azam_sim::FitMode::Stress),
                    Operator::SetFitMap(azam_sim::FitMode::Pressure),
                    Operator::ToggleArrangementPoints,
                ],
            ),
            PieKind::View => (
                "View",
                vec![
                    Operator::View(ViewPreset::Left),
                    Operator::View(ViewPreset::Right),
                    Operator::View(ViewPreset::Front),
                    Operator::View(ViewPreset::Top),
                    Operator::View(ViewPreset::Back),
                    Operator::FrameAll,
                    Operator::ToggleOrtho,
                ],
            ),
        };
        let mut layer = div()
            .class("menu-scrim")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.pie = None;
                cx.notify();
            }))
            .child(
                div()
                    .class("pie-title")
                    .css("left", format!("{}px", pie.at.0 - 40.0))
                    .css("top", format!("{}px", pie.at.1 - PIE_RADIUS * 0.75 - 54.0))
                    .child(title),
            )
            .child(
                div()
                    .class("pie-ring")
                    .css("left", format!("{}px", pie.at.0 - 18.0))
                    .css("top", format!("{}px", pie.at.1 - 18.0)),
            );
        for (op, (dx, dy)) in items.into_iter().zip(DIRECTIONS) {
            let x = pie.at.0 + dx * PIE_RADIUS;
            let y = pie.at.1 + dy * PIE_RADIUS * 0.75;
            let icon = op.icon().map(|i| self.icons.icon(i));
            let left = if dx < -0.1 {
                x - 150.0
            } else if dx > 0.1 {
                x
            } else {
                x - 60.0
            };
            layer = layer.child(
                div()
                    .class("pie-item")
                    .css("left", format!("{left}px"))
                    .css("top", format!("{}px", y - 12.0))
                    .when_some(icon, Div::child)
                    .child(op.label())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.run(op, window, cx);
                    })),
            );
        }
        layer
    }

    /// Rebuilds derived drawing data after the project changed.
    fn sync_scene(&mut self) {
        if self.scene_revision != self.session.revision {
            self.scene_revision = self.session.revision;
            self.pattern_scene = pattern_scene_with(
                &self.session.project,
                &self.session.selection,
                self.workspace == Workspace::Grading,
                &self.collab_marks,
            );
        }
    }
}

impl AzamApp {
    /// Brings layout bounds, derived scenes and the 3D frame up to date before building elements.
    fn prepare_frame(&mut self, window: &mut Window) {
        if !self.focused_once || window.focused_id().is_none() {
            self.focused_once = true;
            window.focus_id("root");
        }
        self.icons.set_scale(window.scale_factor());
        self.viewport.bounds = window.bounds_of(VIEWPORT_ID);
        self.pattern_bounds = window.bounds_of(PATTERN_ID);
        self.note_areas(window);
        self.frames += 1;
        self.sync_collab();
        self.sync_scene();
        if let Some(i) = self.open_menu
            && let Some(b) = window.bounds_of(&format!("menu-{i}"))
        {
            self.menu_anchor = (b.origin.x, b.origin.y + b.size.height + 1.0);
        }
        self.menu_title_bounds = if self.open_menu.is_some() {
            (0..self.menus.len())
                .map(|i| window.bounds_of(&format!("menu-{i}")))
                .collect()
        } else {
            Vec::new()
        };
        if self.open_menu.is_none() {
            self.open_submenu = None;
        } else if let Some(title) = self.open_submenu
            && let Some(b) = window.bounds_of(&format!("submenu-{title}"))
        {
            self.submenu_anchor = (b.origin.x + b.size.width + 6.0, b.origin.y - 4.0);
        }
        if let Some(b) = self.pattern_bounds {
            let size = (b.size.width, b.size.height);
            if !self.framed || self.framed_size.is_some_and(|s| s != size) {
                self.framed = true;
                self.frame_pattern();
                // the layout this frame builds may give the editor a new size; one more frame
                // reframes for it
                window.request_animation_frame();
            } else {
                self.framed_size = None;
            }
        }
        let mut pending = false;
        if let Some(renderer) = self.viewport.renderer() {
            pending = self
                .scene3d
                .sync(&self.session.project, self.session.revision, renderer);
            if self.playing
                && let Some(reason) = self.scene3d.step(&self.session.project, renderer)
            {
                self.playing = false;
                self.status = format!("Simulation stopped: {reason:?}");
            }
            let fit = (self.shading == Shading::FitMap).then_some(self.fit_mode);
            self.scene3d.update_fit(&self.session.project, fit, renderer);
        }
        pending |= self.playing;
        pending |= self.poll_render();
        let mut draws = self
            .scene3d
            .draws(&self.session.project, &self.session.selection, self.shading);
        self.tint_collaborator_pieces(&mut draws);
        self.viewport.render(&draws, window.scale_factor());
        if pending {
            window.request_animation_frame();
        }
    }

    /// The editor areas of the current workspace.
    fn workspace_areas(&mut self, cx: &mut Context<Self>) -> Div {
        let layout = self.layout_mut().clone();
        div().class("areas").child(self.area_tree(&layout, cx).flex_1())
    }

    /// Menus, popups and pies drawn over the window.
    fn overlays(&mut self, mut root: Div, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if let Some(i) = self.open_menu {
            root = root
                .child(
                    div()
                        .class("menu-scrim")
                        .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                            // a click on the title of the menu that was open when it began closes
                            // it; a release on another title (after hovering or dragging across
                            // the bar onto it) leaves that menu open; anywhere else closes
                            let at = this.menu_title_at(e.up.position);
                            this.open_menu = at.filter(|&t| this.menu_at_press != Some(t));
                            cx.notify();
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseDownEvent, _, _| this.menu_at_press = this.open_menu),
                        )
                        .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                            if let Some(t) = this.menu_title_at(e.position)
                                && this.open_menu != Some(t)
                            {
                                this.open_menu = Some(t);
                                this.open_submenu = None;
                                cx.notify();
                            }
                        })),
                )
                .child(self.dropdown(i, cx));
        }
        if self.search.is_some() || self.file_prompt.is_some() {
            root = root.child(div().class("menu-scrim").on_click(cx.listener(
                |this, _: &ClickEvent, window, cx| {
                    this.search = None;
                    this.file_prompt = None;
                    window.focus_id("root");
                    cx.notify();
                },
            )));
        }
        if self.search.is_some() {
            root = root.child(self.search_popup(cx));
        }
        if let Some(prompt) = self.file_prompt.clone() {
            root = root.child(self.file_popup(&prompt, cx));
        }
        if let Some(text) = self.join_prompt.clone() {
            root = root.child(self.join_popup(&text, cx));
        }
        if let Some(pie) = self.pie {
            root = root.child(self.pie_menu(pie, cx));
        }
        if let Some(view) = self.prefs_view.clone() {
            root = root.child(self.prefs_popup(&view, cx));
        }
        if let Some(tip) = self.tooltip_overlay(window) {
            root = root.child(tip);
        }
        root
    }
}

impl Render for AzamApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_frame(window);
        let root = div()
            .class("root")
            .id("root")
            .relative()
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, _| this.tooltip = None))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(self.topbar(cx))
            .child(if self.render_view.is_some() {
                self.render_area(cx)
            } else {
                self.workspace_areas(cx)
            })
            .child(self.statusbar());
        let root = self.overlays(root, window, cx);
        html()
            .styled()
            .child(head().child(title().child("azam")).child(style().child(CSS)))
            .child(body().child(root))
    }
}

/// Whether `p` lies inside `b`.
fn contains(b: &Bounds<f32>, p: Point<Pixels>) -> bool {
    let (x, y) = (p.x.0, p.y.0);
    x >= b.origin.x && x < b.origin.x + b.size.width && y >= b.origin.y && y < b.origin.y + b.size.height
}

/// How the 3D header names avatar motion `m` ("Rest Pose" for none).
fn motion_label(m: azam_avatar::Motion) -> &'static str {
    if m == azam_avatar::Motion::None {
        "Rest Pose"
    } else {
        m.label()
    }
}
