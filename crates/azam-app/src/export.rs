//! Export: DXF and SVG patterns, the draped garment as OBJ, and pattern and tech-pack PDFs.

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
            Self::PatternPdf | Self::TechPackPdf => "pdf",
        }
    }

    const fn suffix(self) -> &'static str {
        match self {
            Self::PatternPdf => "-pattern",
            Self::TechPackPdf => "-techpack",
            Self::Dxf | Self::Svg | Self::Obj => "",
        }
    }

    const fn filter(self) -> &'static str {
        match self {
            Self::Dxf => "DXF\t*.dxf",
            Self::Svg => "SVG\t*.svg",
            Self::Obj => "Wavefront OBJ\t*.obj",
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
        let pieces: Vec<ObjPiece<'_>> = buffers
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
            .collect();
        Some(azam_io::export_obj(&pieces, None))
    }
}
