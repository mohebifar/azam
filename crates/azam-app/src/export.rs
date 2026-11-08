//! Export: DXF and SVG patterns, the draped garment as OBJ or glTF, and pattern and tech-pack
//! PDFs.

use std::path::{Path, PathBuf};

use azam_io::{ObjMaterial, ObjPiece, PatternPdfOptions, TechPackInfo};
use blenderui::{FileDialogMode, FileDialogResult, file_dialog};

use crate::app::AzamApp;

/// A file azam can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    /// AAMA-style DXF of the pattern.
    Dxf,
    /// SVG of the pattern.
    Svg,
    /// The draped garment as OBJ (with an MTL next to it).
    Obj,
    /// The draped garment, and the avatar if it is shown, as binary glTF.
    Gltf,
    /// The recorded simulation as an animated binary glTF.
    Animation,
    /// The pattern as a tiled PDF to print at 1:1.
    PatternPdf,
    /// The tech pack as a PDF.
    TechPackPdf,
}

impl ExportKind {
    /// The export an export operator runs.
    pub(crate) const fn for_operator(op: crate::operators::Operator) -> Self {
        use crate::operators::Operator as O;
        match op {
            O::ExportSvg => Self::Svg,
            O::ExportObj => Self::Obj,
            O::ExportGltf => Self::Gltf,
            O::ExportPatternPdf => Self::PatternPdf,
            O::ExportTechPackPdf => Self::TechPackPdf,
            _ => Self::Dxf,
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::Dxf => "dxf",
            Self::Svg => "svg",
            Self::Obj => "obj",
            Self::Gltf | Self::Animation => "glb",
            Self::PatternPdf | Self::TechPackPdf => "pdf",
        }
    }

    const fn suffix(self) -> &'static str {
        match self {
            Self::PatternPdf => "-pattern",
            Self::TechPackPdf => "-techpack",
            Self::Dxf | Self::Svg | Self::Obj | Self::Gltf => "",
            Self::Animation => "-animation",
        }
    }

    const fn filter(self) -> &'static str {
        match self {
            Self::Dxf => "DXF\t*.dxf",
            Self::Svg => "SVG\t*.svg",
            Self::Obj => "Wavefront OBJ\t*.obj",
            Self::Gltf | Self::Animation => "glTF Binary\t*.glb",
            Self::PatternPdf | Self::TechPackPdf => "PDF\t*.pdf",
        }
    }
}

/// One piece's geometry gathered for OBJ export.
struct Buffers {
    name: String,
    material: ObjMaterial,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f64; 2]>,
    triangles: Vec<[u32; 3]>,
}

impl AzamApp {
    /// Asks where to write `kind` and writes it.
    pub(crate) fn export(&mut self, kind: ExportKind) {
        let name = azam_io::format::slug(&self.session.project.meta.name);
        let file = format!("{name}{}.{}", kind.suffix(), kind.extension());
        let dir = self
            .session
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        let path = match file_dialog(
            "Export",
            FileDialogMode::Save,
            dir.as_deref(),
            Some(&file),
            &[kind.filter()],
            false,
        ) {
            FileDialogResult::Chosen(paths) => match paths.into_iter().next() {
                Some(p) => p,
                None => return,
            },
            FileDialogResult::Cancelled => {
                self.status = "Export cancelled".into();
                return;
            }
            // No window to host the browser (headless): write next to the project.
            FileDialogResult::Unavailable => dir.unwrap_or_default().join(file),
        };
        self.status = match self.write_export(kind, &path) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    /// Writes `kind` to `path` without asking (scripts and tests).
    ///
    /// # Errors
    /// Why the file could not be written, or that there is no draped garment to export.
    pub fn export_to(&self, kind: ExportKind, path: &Path) -> Result<(), String> {
        self.write_export(kind, path)
    }

    fn write_export(&self, kind: ExportKind, path: &Path) -> Result<(), String> {
        let project = &self.session.project;
        let write =
            |p: &Path, bytes: &[u8]| std::fs::write(p, bytes).map_err(|e| format!("{}: {e}", p.display()));
        match kind {
            ExportKind::Dxf => write(path, azam_io::export_dxf(project).as_bytes()),
            ExportKind::Svg => write(path, azam_io::export_svg(project).as_bytes()),
            ExportKind::PatternPdf => {
                let pdf = azam_io::pattern_pdf(project, &PatternPdfOptions::default())
                    .map_err(|e| e.to_string())?;
                write(path, &pdf)
            }
            ExportKind::TechPackPdf => write(path, &azam_io::techpack_pdf(project, &TechPackInfo::default())),
            ExportKind::Gltf | ExportKind::Animation => {
                let anim = match kind {
                    ExportKind::Animation => Some(
                        self.scene3d
                            .recording
                            .as_ref()
                            .filter(|r| r.pieces.len() > 1)
                            .ok_or("nothing is recorded (Simulation ▸ Record Animation, then play)")?,
                    ),
                    _ => None,
                };
                let buffers = self.garment_buffers().ok_or("there is no draped garment yet")?;
                let pieces = piece_views(&buffers);
                let avatar = self
                    .scene3d
                    .avatar
                    .as_ref()
                    .filter(|_| project.avatar.show)
                    .map(|av| {
                        let m = &av.mesh;
                        (
                            m.positions
                                .chunks_exact(3)
                                .map(|c| [c[0], c[1], c[2]])
                                .collect::<Vec<_>>(),
                            m.normals
                                .chunks_exact(3)
                                .map(|c| [c[0], c[1], c[2]])
                                .collect::<Vec<_>>(),
                            m.indices
                                .chunks_exact(3)
                                .map(|t| [t[0], t[1], t[2]])
                                .collect::<Vec<_>>(),
                        )
                    });
                let mesh = avatar
                    .as_ref()
                    .map(|(positions, normals, triangles)| azam_io::ObjMesh {
                        positions,
                        normals,
                        triangles,
                    });
                let glb = match anim {
                    Some(a) => azam_io::export_glb_animated(&pieces, mesh.as_ref(), a),
                    None => azam_io::export_glb(&pieces, mesh.as_ref()),
                };
                write(path, &glb)
            }
            ExportKind::Obj => {
                let export = self.obj_export().ok_or("there is no draped garment yet")?;
                write(path, export.obj.as_bytes())?;
                write(
                    &path.with_file_name(azam_io::MTL_FILE_NAME),
                    export.mtl.as_bytes(),
                )
            }
        }
    }

    fn obj_export(&self) -> Option<azam_io::ObjExport> {
        let buffers = self.garment_buffers()?;
        Some(azam_io::export_obj(&piece_views(&buffers), None))
    }

    /// Every simulated piece's geometry, as the exporters take it.
    fn garment_buffers(&self) -> Option<Vec<Buffers>> {
        let sim = self.scene3d.sim.as_ref()?;
        let project = &self.session.project;
        let buffers: Vec<Buffers> = sim
            .piece_views()
            .map(|v| {
                let piece = project.pieces.get(&v.record.id);
                let xy = &v.record.mesh.xy;
                Buffers {
                    name: piece.map_or_else(|| v.record.id.as_str().to_owned(), |p| p.name.clone()),
                    material: ObjMaterial::for_fabric(project, &v.record.fabric),
                    positions: v.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
                    normals: v.normals.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
                    uvs: xy.chunks_exact(2).map(|c| [c[0], c[1]]).collect(),
                    triangles: v.triangles.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
                }
            })
            .collect();
        Some(buffers)
    }
}

/// `buffers` as the exporters' pieces.
fn piece_views(buffers: &[Buffers]) -> Vec<ObjPiece<'_>> {
    buffers
        .iter()
        .map(|b| ObjPiece {
            name: &b.name,
            material: b.material.clone(),
            hidden: false,
            positions: &b.positions,
            normals: &b.normals,
            uvs: &b.uvs,
            triangles: &b.triangles,
        })
        .collect()
}

impl AzamApp {
    /// Starts recording the simulation's frames (play to record), or stops and keeps them for
    /// File ▸ Export Animation.
    pub fn toggle_recording(&mut self) {
        if let Some(rec) = &self.scene3d.recording
            && self.scene3d.record
        {
            self.scene3d.record = false;
            self.status = format!(
                "Recorded {} frames ({:.1} s): File ▸ Export Animation writes them",
                rec.pieces.len(),
                rec.pieces.len() as f64 * rec.dt
            );
        } else {
            self.scene3d.recording = Some(azam_io::Animation::default());
            self.scene3d.record = true;
            self.status = "Recording: play the simulation (Space); Record Animation again stops".into();
        }
    }

    /// Frames recorded so far, if any are kept.
    #[must_use]
    pub fn recorded_frames(&self) -> Option<usize> {
        self.scene3d.recording.as_ref().map(|r| r.pieces.len())
    }
}
