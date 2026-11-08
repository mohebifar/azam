//! Managing the project's fabrics and colorways: new, delete, and the fabric library shared by
//! every project.

use azam_doc::{ColorwayId, FabricId};
use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::fabric_library;

/// The first entry of the library dropdown, which adds nothing.
const LIBRARY_PROMPT: &str = "Add from Library…";

impl AzamApp {
    /// A row of buttons.
    fn button_row(buttons: Vec<Div>) -> Div {
        buttons.into_iter().fold(div().class("pbuttons"), Div::child)
    }

    /// A button labelled `label` that runs `act` on the app.
    fn action(label: &'static str, cx: &mut Context<Self>, act: impl Fn(&mut Self) + 'static) -> Div {
        div()
            .class("pbtn")
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                act(this);
                cx.notify();
            }))
    }

    /// Fabric `fid`'s buttons (New: a copy given to the selected piece; Delete; Save to Library)
    /// and the dropdown that adds a library fabric to the project.
    pub(super) fn fabric_library_rows(&self, fid: &FabricId, cx: &mut Context<Self>) -> Vec<Div> {
        let piece = self.active_piece();
        let (copy_of, delete, save) = (fid.clone(), fid.clone(), fid.clone());
        let new_piece = piece.clone();
        let buttons = Self::button_row(vec![
            Self::action("New", cx, move |this| {
                let mut created = None;
                this.session.edit("New Fabric", |p| {
                    created = azam_doc::duplicate_fabric(p, &copy_of);
                    if let (Some(new), Some(pc)) =
                        (&created, new_piece.as_ref().and_then(|id| p.pieces.get_mut(id)))
                    {
                        pc.fabric = new.clone();
                    }
                });
                if let Some(new) = created {
                    this.status = format!("Added fabric {}", this.session.project.fabrics[&new].name);
                }
            }),
            Self::action("Delete", cx, move |this| {
                let name = this
                    .session
                    .project
                    .fabrics
                    .get(&delete)
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                if this.session.project.fabrics.len() < 2 {
                    this.status = "The last fabric cannot be deleted".into();
                    return;
                }
                let mut heir = None;
                this.session
                    .edit("Delete Fabric", |p| heir = azam_doc::delete_fabric(p, &delete));
                this.status = match heir {
                    Some(h) => format!(
                        "Deleted {name}; its pieces are now {}",
                        this.session.project.fabrics[&h].name
                    ),
                    None => "The last fabric cannot be deleted".into(),
                };
            }),
            Self::action("Save to Library", cx, move |this| {
                let (Some(dir), Some(fabric)) = (
                    fabric_library::fabrics_dir(),
                    this.session.project.effective_fabric(&save),
                ) else {
                    this.status = "No folder for the fabric library".into();
                    return;
                };
                this.fabric_library = None;
                this.status = match fabric_library::save(&dir, &fabric) {
                    Ok(path) => format!("Saved {} to {}", fabric.name, path.display()),
                    Err(e) => format!("Could not save to the library: {e}"),
                };
            }),
        ]);
        let library = self.fabric_library.clone().unwrap_or_default();
        let mut select = Dropdown().class("pfield");
        select = select.child(option().child(LIBRARY_PROMPT).selected(true));
        for f in &library {
            select = select.child(option().child(f.name.clone()));
        }
        let add =
            div()
                .class("prow")
                .child(select.on_change(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                    let i = e.number.max(0.0) as usize;
                    let Some(fabric) = i.checked_sub(1).and_then(|k| library.get(k)).cloned() else {
                        return;
                    };
                    let name = fabric.name.clone();
                    let piece = piece.clone();
                    this.session.edit("Add Library Fabric", |p| {
                        let id = azam_doc::add_fabric(p, fabric);
                        if let Some(pc) = piece.as_ref().and_then(|id| p.pieces.get_mut(id)) {
                            pc.fabric = id;
                        }
                    });
                    this.status = format!("Added {name} from the library");
                    cx.notify();
                })));
        vec![buttons, add]
    }

    /// New Colorway (a copy of the active one, made active) and Delete Colorway.
    pub(super) fn colorway_buttons(active: &ColorwayId, cx: &mut Context<Self>) -> Div {
        let active = active.clone();
        Self::button_row(vec![
            Self::action("New Colorway", cx, |this| {
                let mut created = None;
                this.session.edit("New Colorway", |p| {
                    let active = p.active_colorway.clone();
                    created = azam_doc::duplicate_colorway(p, &active);
                    if let Some(id) = &created {
                        p.active_colorway = id.clone();
                    }
                });
                if let Some(id) = created {
                    this.status = format!("Added colorway {}", this.session.project.colorways[&id].name);
                }
            }),
            Self::action("Delete Colorway", cx, move |this| {
                if this.session.project.colorways.len() < 2 {
                    this.status = "The last colorway cannot be deleted".into();
                    return;
                }
                let mut done = false;
                this.session.edit("Delete Colorway", |p| {
                    done = azam_doc::delete_colorway(p, &active);
                });
                this.status = if done {
                    "Deleted the colorway".into()
                } else {
                    "The last colorway cannot be deleted".into()
                };
            }),
        ])
    }

    /// Fabric `fid`'s texture: an image tiled over the cloth along each piece's grain (shown in
    /// the 3D view and in renders), and how wide one tile is.
    pub(super) fn texture_panel(&mut self, fid: &FabricId, cx: &mut Context<Self>) -> Div {
        let fabric = self.session.project.fabrics.get(fid);
        let texture = fabric.and_then(|f| f.texture()).map(|(p, mm)| (p.to_owned(), mm));
        let shown = texture.as_ref().map_or_else(
            || "None".to_owned(),
            |(p, _)| {
                std::path::Path::new(p)
                    .file_name()
                    .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned())
            },
        );
        let choose = {
            let fid = fid.clone();
            Self::action("Choose Image…", cx, move |this| {
                let picked = blenderui::file_dialog(
                    "Fabric Texture",
                    blenderui::FileDialogMode::Open,
                    None,
                    None,
                    &["Images\t*.png *.jpg *.jpeg *.bmp *.tga"],
                    false,
                );
                if let blenderui::FileDialogResult::Chosen(paths) = picked
                    && let Some(path) = paths.into_iter().next()
                {
                    this.set_fabric_texture(&fid, Some(&path.to_string_lossy()));
                }
            })
        };
        let clear = {
            let fid = fid.clone();
            Self::action("Clear", cx, move |this| this.set_fabric_texture(&fid, None))
        };
        let mut rows = vec![
            Self::info_row("Image", shown),
            div().class("pbuttons").child(choose).child(clear),
        ];
        if let Some((_, mm)) = texture {
            let fid = fid.clone();
            rows.push(Self::number(
                super::field("Tile Width (mm)", 5.0, 2000.0, 5.0),
                mm,
                cx,
                move |p, v| {
                    if let Some(f) = p.fabrics.get_mut(&fid) {
                        f.extra.insert(
                            azam_doc::FABRIC_TEXTURE_MM_KEY.to_owned(),
                            serde_json::Value::from(v),
                        );
                    }
                },
            ));
        }
        self.panel("Texture", rows)
    }

    /// Fabric `fid`'s surface relief: a normal map and a roughness map tiled over the cloth (they
    /// bend its shading and set its gloss texel by texel, in the 3D view and in renders), their
    /// tile width and the normal map's strength.
    pub(super) fn surface_panel(&mut self, fid: &FabricId, cx: &mut Context<Self>) -> Div {
        let fabric = self.session.project.effective_fabric(fid);
        let maps = fabric.as_ref().and_then(azam_doc::Fabric::surface_maps).map(|m| {
            (
                m.normal.map(str::to_owned),
                m.roughness.map(str::to_owned),
                m.tile_mm,
                m.strength,
            )
        });
        let file_name = |p: &Option<String>| {
            p.as_ref().map_or_else(
                || "None".to_owned(),
                |p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned())
                },
            )
        };
        let mut rows = Vec::new();
        for (label, key, current) in [
            (
                "Normal Map",
                azam_doc::FABRIC_NORMAL_MAP_KEY,
                maps.as_ref().and_then(|m| m.0.clone()),
            ),
            (
                "Roughness Map",
                azam_doc::FABRIC_ROUGHNESS_MAP_KEY,
                maps.as_ref().and_then(|m| m.1.clone()),
            ),
        ] {
            let choose = {
                let fid = fid.clone();
                Self::action("Choose…", cx, move |this| {
                    let picked = blenderui::file_dialog(
                        label,
                        blenderui::FileDialogMode::Open,
                        None,
                        None,
                        &["Images\t*.png *.jpg *.jpeg *.bmp *.tga"],
                        false,
                    );
                    if let blenderui::FileDialogResult::Chosen(paths) = picked
                        && let Some(path) = paths.into_iter().next()
                    {
                        this.set_fabric_map(&fid, key, label, Some(&path.to_string_lossy()));
                    }
                })
            };
            let clear = {
                let fid = fid.clone();
                Self::action("Clear", cx, move |this| {
                    this.set_fabric_map(&fid, key, label, None);
                })
            };
            rows.push(Self::info_row(label, file_name(&current)));
            rows.push(div().class("pbuttons").child(choose).child(clear));
        }
        if let Some((_, _, tile_mm, strength)) = maps {
            let at = fid.clone();
            rows.push(Self::number(
                super::field("Map Tile Width (mm)", 5.0, 2000.0, 5.0),
                tile_mm,
                cx,
                move |p, v| {
                    if let Some(f) = p.fabrics.get_mut(&at) {
                        f.extra
                            .insert(azam_doc::FABRIC_MAP_MM_KEY.to_owned(), serde_json::Value::from(v));
                    }
                },
            ));
            let at = fid.clone();
            rows.push(Self::number(
                super::field("Normal Strength", 0.0, azam_doc::MAX_NORMAL_STRENGTH, 0.05),
                strength,
                cx,
                move |p, v| {
                    if let Some(f) = p.fabrics.get_mut(&at) {
                        f.extra.insert(
                            azam_doc::FABRIC_NORMAL_STRENGTH_KEY.to_owned(),
                            serde_json::Value::from(v),
                        );
                    }
                },
            ));
        }
        self.panel("Surface", rows)
    }

    /// Sets fabric `fid`'s map under `key` (named `label` in the undo history) to the image at
    /// `path`, or clears it.
    pub fn set_fabric_map(&mut self, fid: &FabricId, key: &str, label: &str, path: Option<&str>) {
        let value = path.map(str::to_owned);
        self.session.edit(label, |p| {
            if let Some(f) = p.fabrics.get_mut(fid) {
                match &value {
                    Some(v) => {
                        f.extra.insert(key.to_owned(), serde_json::Value::from(v.clone()));
                    }
                    None => {
                        f.extra.remove(key);
                    }
                }
            }
        });
        self.status = match path {
            Some(p) => format!("{label}: {p}"),
            None => format!("{label} cleared"),
        };
    }

    /// Gives fabric `fid` the texture image at `path`, or none.
    pub fn set_fabric_texture(&mut self, fid: &FabricId, path: Option<&str>) {
        let value = path.map(str::to_owned);
        self.session.edit("Fabric Texture", |p| {
            if let Some(f) = p.fabrics.get_mut(fid) {
                match &value {
                    Some(v) => {
                        f.extra.insert(
                            azam_doc::FABRIC_TEXTURE_KEY.to_owned(),
                            serde_json::Value::from(v.clone()),
                        );
                    }
                    None => {
                        f.extra.remove(azam_doc::FABRIC_TEXTURE_KEY);
                    }
                }
            }
        });
        self.status = match path {
            Some(p) => format!("Texture: {p}"),
            None => "Texture cleared".into(),
        };
    }
}
