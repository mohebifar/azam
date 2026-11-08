//! Bringing pattern pieces in from other software: DXF (AAMA blocks) and SVG drawings.

use std::path::{Path, PathBuf};

use azam_doc::{LinePoint, PieceId, Project};
use azam_io::ImportedPiece;

use crate::app::AzamApp;

/// A file format pieces can be imported from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    /// AAMA-style DXF.
    Dxf,
    /// An SVG drawing.
    Svg,
}

impl ImportFormat {
    /// Name, as the file browser and the undo history show it.
    const fn name(self) -> &'static str {
        match self {
            Self::Dxf => "DXF",
            Self::Svg => "SVG",
        }
    }

    /// The file browser's filter.
    const fn filter(self) -> &'static str {
        match self {
            Self::Dxf => "DXF\t*.dxf",
            Self::Svg => "SVG\t*.svg",
        }
    }

    /// The pieces in `text`.
    fn read(self, text: &str) -> Result<Vec<ImportedPiece>, String> {
        match self {
            Self::Dxf => azam_io::import_dxf(text).map_err(|e| e.to_string()),
            Self::Svg => azam_io::import_svg(text).map_err(|e| e.to_string()),
        }
    }
}

impl AzamApp {
    /// Asks for a file in `format` and adds its pieces to the project, as one undoable edit.
    pub(crate) fn import_file(&mut self, format: ImportFormat) {
        let dir = std::env::var_os("HOME").map(PathBuf::from);
        let path = match blenderui::file_dialog(
            &format!("Import {}", format.name()),
            blenderui::FileDialogMode::Open,
            dir.as_deref(),
            None,
            &[format.filter()],
            false,
        ) {
            blenderui::FileDialogResult::Chosen(paths) => match paths.into_iter().next() {
                Some(p) => p,
                None => return,
            },
            blenderui::FileDialogResult::Cancelled => return,
            blenderui::FileDialogResult::Unavailable => {
                self.status = "Importing needs a window for the file browser".into();
                return;
            }
        };
        self.status = match self.import_path(format, &path) {
            Ok(note) => note,
            Err(e) => format!("Import failed: {e}"),
        };
    }

    /// Adds the pieces of the `format` file at `path` to the project, as one undoable edit; an
    /// ASTM DXF with rule numbers is graded by the rule table next to it (same name, `.rul`).
    /// Returns what was imported.
    ///
    /// # Errors
    /// Why the file (or its rule table) could not be read.
    pub fn import_path(&mut self, format: ImportFormat, path: &Path) -> Result<String, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let pieces = format.read(&text)?;
        let graded = pieces.iter().any(|pc| pc.rules.iter().any(Option::is_some));
        let table = if graded {
            ["rul", "RUL"]
                .iter()
                .map(|ext| path.with_extension(ext))
                .find(|p| p.exists())
                .map(|p| {
                    std::fs::read_to_string(&p)
                        .map_err(|e| e.to_string())
                        .and_then(|t| azam_io::import_rul(&t).map_err(|e| format!("{}: {e}", p.display())))
                })
                .transpose()?
        } else {
            None
        };
        let count = pieces.len();
        self.session.edit(&format!("Import {}", format.name()), |p| {
            let created = add_imported_pieces(p, &pieces);
            if let Some(table) = &table {
                apply_rule_table(p, &created, &pieces, table);
            }
        });
        let graded_note = match &table {
            Some(t) => format!(", graded {} sizes by rule table {}", t.sizes.len(), t.name),
            None if graded => " (rule numbers but no .rul table next to it: base size only)".to_owned(),
            None => String::new(),
        };
        let note = format!("Imported {count} piece(s) from {}{graded_note}", path.display());
        Ok(note)
    }
}

/// Grades the imported pieces `created` (from `pieces`, in order) by `table`: the project takes
/// the table's size run and sample size, and each point with a rule number its offsets.
pub fn apply_rule_table(
    project: &mut Project,
    created: &[PieceId],
    pieces: &[ImportedPiece],
    table: &azam_io::GradeRuleTable,
) {
    azam_doc::set_sizes(project, &table.sizes);
    azam_doc::set_base_size(project, &table.sample);
    project.active_size = table.sample.clone();
    for (id, imported) in created.iter().zip(pieces) {
        let Some(pc) = project.pieces.get_mut(id) else {
            continue;
        };
        let mut rules = azam_geom::GradeRules::new();
        for (point, rule) in pc.pts.iter().zip(&imported.rules) {
            let Some(offsets) = rule.and_then(|n| table.rules.get(&n)) else {
                continue;
            };
            let per_size: std::collections::BTreeMap<_, _> = table
                .sizes
                .iter()
                .zip(offsets)
                .filter(|&(size, d)| *size != table.sample && (d.x != 0.0 || d.y != 0.0))
                .map(|(size, d)| (size.clone(), azam_geom::GradeDelta { dx: d.x, dy: d.y }))
                .collect();
            if !per_size.is_empty() {
                rules.insert(point.id.clone(), per_size);
            }
        }
        pc.grade = (!rules.is_empty()).then_some(rules);
    }
}

impl AzamApp {
    /// Asks for a U3M material and adds it to the project as a fabric.
    pub(crate) fn import_u3m_dialog(&mut self) {
        let dir = std::env::var_os("HOME").map(PathBuf::from);
        match blenderui::file_dialog(
            "Import U3M Fabric",
            blenderui::FileDialogMode::Open,
            dir.as_deref(),
            None,
            &["U3M Material\t*.u3m *.u3ma"],
            false,
        ) {
            blenderui::FileDialogResult::Chosen(paths) => {
                if let Some(path) = paths.into_iter().next() {
                    self.status = match self.import_u3m(&path) {
                        Ok(note) => note,
                        Err(e) => format!("U3M import failed: {e}"),
                    };
                }
            }
            blenderui::FileDialogResult::Cancelled => {}
            blenderui::FileDialogResult::Unavailable => {
                self.status = "Importing needs a window for the file browser".into();
            }
        }
    }

    /// Adds the U3M material at `path` (a `.u3m` file beside its images, or a `.u3ma` archive,
    /// whose images are unpacked into azam's data folder) to the project as a new fabric, as one
    /// undoable edit. Returns what was imported.
    ///
    /// # Errors
    /// Why the file could not be read or its images not unpacked.
    pub fn import_u3m(&mut self, path: &Path) -> Result<String, String> {
        let root = crate::fabric_library::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("u3m");
        self.import_u3m_into(path, &root)
    }

    /// [`AzamApp::import_u3m`], unpacking an archive's images into a folder under `unpack_root`.
    ///
    /// # Errors
    /// Why the file could not be read or its images not unpacked.
    pub fn import_u3m_into(&mut self, path: &Path, unpack_root: &Path) -> Result<String, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let archive = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("u3ma"));
        let (material, image_dir) = if archive {
            let a = azam_io::read_u3ma(&bytes).map_err(|e| e.to_string())?;
            let stem = path
                .file_stem()
                .map_or_else(|| "u3m".into(), |s| s.to_string_lossy().into_owned());
            let root = unpack_root.join(azam_io::format::slug(&stem));
            let mut dir = root.clone();
            for (file, data) in &a.files {
                let Some(rel) = safe_relative(file) else { continue };
                let target = root.join(rel);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                std::fs::write(&target, data).map_err(|e| format!("{}: {e}", target.display()))?;
                if *file == a.u3m_path
                    && let Some(parent) = target.parent()
                {
                    dir = parent.to_path_buf();
                }
            }
            (a.material, dir)
        } else {
            let json = String::from_utf8_lossy(&bytes);
            let m = azam_io::read_u3m(&json).map_err(|e| e.to_string())?;
            (m, path.parent().map(Path::to_path_buf).unwrap_or_default())
        };
        let fabric = u3m_fabric(&material, &image_dir);
        let name = fabric.name.clone();
        self.session.edit("Import U3M Fabric", |p| {
            azam_doc::add_fabric(p, fabric);
        });
        Ok(format!("Imported fabric \"{name}\" from {}", path.display()))
    }
}

/// `path` (inside an archive) as a relative path that stays inside the folder it is unpacked
/// into, or `None` for one that would climb out of it.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.contains(':') => return None,
            p => out.push(p),
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// The azam fabric for U3M `material`, whose images live under `dir`: its colour, finish, and
/// texture, normal and roughness maps with their tile widths (from the file, else from each
/// image's resolution). Physics stay those of the default preset.
#[must_use]
pub fn u3m_fabric(material: &azam_io::U3mMaterial, dir: &Path) -> azam_doc::Fabric {
    let mut fabric = azam_doc::Fabric::default();
    fabric.name.clone_from(&material.name);
    let encode = |c: f64| {
        let s = if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (s.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let [r, g, b] = material.base_color.map(encode);
    fabric.color = format!("#{r:02x}{g:02x}{b:02x}");
    fabric.rough = material.roughness;
    fabric.sheen = material.sheen;
    fabric.opacity = material.alpha.max(0.2);
    let located = |img: &azam_io::U3mImage| {
        let p = dir.join(&img.path);
        let width = blenderui::Image::open(&p.to_string_lossy()).map(|i| i.width);
        (p.to_string_lossy().into_owned(), img.tile_mm(width))
    };
    let mut set = |key: &str, value: serde_json::Value| {
        fabric.extra.insert(key.to_owned(), value);
    };
    let mut base_tile = None;
    if let Some(img) = &material.base_image {
        let (p, tile) = located(img);
        set(azam_doc::FABRIC_TEXTURE_KEY, p.into());
        if let Some(t) = tile {
            set(azam_doc::FABRIC_TEXTURE_MM_KEY, t.into());
            base_tile = Some(t);
        }
    }
    let mut map_tile = None;
    if let Some(img) = &material.normal_image {
        let (p, tile) = located(img);
        set(azam_doc::FABRIC_NORMAL_MAP_KEY, p.into());
        set(
            azam_doc::FABRIC_NORMAL_STRENGTH_KEY,
            material.normal_strength.into(),
        );
        map_tile = tile;
    }
    if let Some(img) = &material.roughness_image {
        let (p, tile) = located(img);
        set(azam_doc::FABRIC_ROUGHNESS_MAP_KEY, p.into());
        map_tile = map_tile.or(tile);
    }
    if let Some(t) = map_tile.or(base_tile) {
        set(azam_doc::FABRIC_MAP_MM_KEY, t.into());
    }
    fabric
}

impl AzamApp {
    /// Asks for an OBJ body mesh and makes it the avatar (kept with the project as a file path).
    pub(crate) fn import_avatar(&mut self) {
        let dir = std::env::var_os("HOME").map(PathBuf::from);
        match blenderui::file_dialog(
            "Import Avatar OBJ",
            blenderui::FileDialogMode::Open,
            dir.as_deref(),
            None,
            &["OBJ\t*.obj"],
            false,
        ) {
            blenderui::FileDialogResult::Chosen(paths) => {
                if let Some(path) = paths.into_iter().next() {
                    self.set_custom_body(Some(&path.to_string_lossy()));
                }
            }
            blenderui::FileDialogResult::Cancelled => {}
            blenderui::FileDialogResult::Unavailable => {
                self.status = "Importing needs a window for the file browser".into();
            }
        }
    }

    /// Which body model the avatar was built from, once built.
    #[must_use]
    pub fn avatar_kind(&self) -> Option<azam_avatar::AvatarKind> {
        self.scene3d.avatar.as_ref().map(|a| a.kind)
    }

    /// Uses the OBJ body at `path` as the avatar, or the built-in body with `None`.
    pub fn set_custom_body(&mut self, path: Option<&str>) {
        let label = if path.is_some() {
            "Custom Body"
        } else {
            "Built-in Body"
        };
        self.session.edit(label, |p| match path {
            Some(path) => {
                p.avatar
                    .extra
                    .insert(crate::garment3d::CUSTOM_BODY_KEY.to_owned(), path.into());
            }
            None => {
                p.avatar.extra.remove(crate::garment3d::CUSTOM_BODY_KEY);
            }
        });
        self.status = match path {
            Some(path) => format!("Avatar body from {path}"),
            None => "Built-in avatar body".into(),
        };
    }
}

/// Adds `pieces` (read from a DXF or SVG file) to `project` with their internal lines, and places each
/// around the avatar by its name ("Back", "Sleeve L", "Collar"…). A second piece that names no
/// side and would land where an earlier one did goes on the other side ("Sleeve", "Sleeve" →
/// left and right arm; "Front Leg", "Front Leg" → left and right leg). Returns the new pieces' ids.
pub fn add_imported_pieces(project: &mut Project, pieces: &[ImportedPiece]) -> Vec<PieceId> {
    let mut created = Vec::new();
    let mut taken: Vec<&'static str> = Vec::new();
    for piece in pieces {
        let points: Vec<(azam_geom::Vec2, bool)> = piece.points.iter().map(|q| (q.pos, q.curve)).collect();
        let Some(id) = azam_doc::create_piece(project, &points) else {
            continue;
        };
        for line in &piece.internals {
            let pts: Vec<LinePoint> = line
                .points
                .iter()
                .map(|&pos| LinePoint {
                    id: None,
                    pos,
                    curve: false,
                })
                .collect();
            azam_doc::add_internal_line(project, &id, &pts, line.closed);
        }
        let mut slot = azam_doc::slot_for_name(&piece.name);
        if let Some(s) = slot
            && taken.contains(&s)
            && !names_a_side(&piece.name)
            && let Some(other) = azam_doc::mirror_slot(s)
        {
            slot = Some(other);
        }
        if let Some(s) = slot {
            taken.push(s);
        }
        let outline = project.pieces.get(&id).map(azam_doc::Piece::outline);
        for &q in &piece.notches {
            if let Some((segment, t)) = outline
                .as_ref()
                .and_then(|ol| ol.nearest(q).map(|n| (ol.segments[n.segment].id.clone(), n.t)))
            {
                azam_doc::add_notch(project, &id, &segment, t);
            }
        }
        if let Some(pc) = project.pieces.get_mut(&id) {
            if let Some(g) = piece.grain {
                pc.grain = g;
            }
            if piece.allowance_included {
                // the outline is the cut line already
                pc.sa = 0.0;
            }
            pc.name.clone_from(&piece.name);
            if let Some(slot) = slot {
                slot.clone_into(&mut pc.arr.slot);
            }
        }
        created.push(id);
    }
    created
}

/// Whether `name` says which side of the body the piece is for.
fn names_a_side(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| matches!(w, "l" | "r" | "lt" | "rt") || w.starts_with("left") || w.starts_with("right"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use azam_geom::{OutlinePoint, Vec2};
    use azam_io::ImportedLine;

    fn square(name: &str, x: f64) -> ImportedPiece {
        let pts = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        ImportedPiece {
            name: name.to_owned(),
            points: pts
                .iter()
                .enumerate()
                .map(|(i, &(px, py))| OutlinePoint {
                    id: azam_doc::PointId::new(format!("p{i}")),
                    pos: Vec2::new(px + x, py),
                    curve: false,
                })
                .collect(),
            internals: vec![ImportedLine {
                points: vec![Vec2::new(x + 50.0, 10.0), Vec2::new(x + 50.0, 90.0)],
                closed: false,
            }],
            notches: vec![Vec2::new(x + 30.0, 0.0)],
            grain: Some(15.0),
            rules: Vec::new(),
            allowance_included: false,
        }
    }

    #[test]
    fn archive_paths_cannot_climb_out_of_their_folder() {
        assert_eq!(safe_relative("a/./b.png"), Some(PathBuf::from("a/b.png")));
        assert_eq!(safe_relative("../etc/passwd"), None);
        assert_eq!(safe_relative("x/../../y"), None);
        assert_eq!(safe_relative("C:/windows"), None);
        assert_eq!(safe_relative(""), None);
    }

    #[test]
    fn a_u3m_material_becomes_a_fabric_with_its_maps() {
        let m = azam_io::U3mMaterial {
            name: "Twill".into(),
            base_color: [0.2, 0.2, 0.2],
            base_image: Some(azam_io::U3mImage {
                path: "tex/base.png".into(),
                width_mm: 60.0,
                dpi: 0.0,
            }),
            normal_image: Some(azam_io::U3mImage {
                path: "tex/normal.png".into(),
                width_mm: 0.0,
                dpi: 300.0,
            }),
            normal_strength: 0.7,
            roughness: 0.4,
            roughness_image: None,
            sheen: 0.1,
            alpha: 1.0,
        };
        let f = u3m_fabric(&m, Path::new("/lib/twill"));
        assert_eq!(f.name, "Twill");
        assert_eq!(f.color, "#7c7c7c", "linear 0.2 is sRGB 124");
        assert!((f.rough - 0.4).abs() < 1e-12);
        let (tex, mm) = f.texture().expect("texture");
        assert_eq!((tex, mm), ("/lib/twill/tex/base.png", 60.0));
        let maps = f.surface_maps().expect("maps");
        assert_eq!(maps.normal, Some("/lib/twill/tex/normal.png"));
        // the normal map's size is unknown (no image on disk), so its tile is the texture's
        assert!((maps.tile_mm - 60.0).abs() < 1e-12);
        assert!((maps.strength - 0.7).abs() < 1e-12);
    }

    #[test]
    fn imported_pieces_keep_internal_lines_and_get_slots() {
        let mut project = Project::new(None, azam_doc::Sex::Female, 0.0);
        let pieces = [
            square("Back", 0.0),
            square("Sleeve", 200.0),
            square("Sleeve", 400.0),
            square("Pocket", 600.0),
            square("Front Leg", 800.0),
            square("Front Leg", 1000.0),
        ];
        let ids = add_imported_pieces(&mut project, &pieces);
        assert_eq!(ids.len(), 6);
        let slot = |i: usize| project.pieces[&ids[i]].arr.slot.clone();
        assert_eq!(slot(0), "back");
        assert_eq!(slot(1), "armL");
        assert_eq!(slot(2), "armR", "the second sleeve goes on the other arm");
        assert_eq!(slot(3), azam_doc::DEFAULT_SLOT);
        assert_eq!(
            (slot(4).as_str(), slot(5).as_str()),
            ("legLF", "legRF"),
            "the legs pair up too"
        );
        assert_eq!(
            project.pieces[&ids[0]].internals.len(),
            1,
            "the internal line comes along"
        );
        assert_eq!(project.pieces[&ids[0]].name, "Back");
        assert_eq!(project.pieces[&ids[0]].notches.len(), 1, "the notch comes along");
        assert!(
            (project.pieces[&ids[0]].grain - 15.0).abs() < 1e-9,
            "and the grain line"
        );
    }
}
