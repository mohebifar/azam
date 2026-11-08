//! The properties editor: Blender's icon tabs down the side and panels of fields for the project,
//! the active piece, the selected seam or part, its fabric, the avatar and the simulation.

mod library;
mod parts;

use azam_doc::{FabricPreset, PieceId, PieceState, Project, Sex};
use blenderui::prelude::*;
use serde_json::Value;

use crate::app::AzamApp;

/// A tab of the properties editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropTab {
    /// Project name, sizes and colorways.
    Project,
    /// The active piece.
    Piece,
    /// The selected seam, segment or part placed on a piece.
    Part,
    /// The active piece's fabric.
    Fabric,
    /// Avatar measurements.
    Avatar,
    /// Simulation settings.
    Simulation,
}

impl PropTab {
    /// Tabs in display order.
    pub const ALL: [Self; 6] = [
        Self::Project,
        Self::Piece,
        Self::Part,
        Self::Fabric,
        Self::Avatar,
        Self::Simulation,
    ];

    const fn icon(self) -> &'static str {
        match self {
            Self::Project => "SCENE_DATA",
            Self::Piece => "MESH_DATA",
            Self::Part => "MODIFIER",
            Self::Fabric => "MATERIAL",
            Self::Avatar => "ARMATURE_DATA",
            Self::Simulation => "PHYSICS",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Piece => "Piece",
            Self::Part => "Item",
            Self::Fabric => "Fabric",
            Self::Avatar => "Avatar",
            Self::Simulation => "Simulation",
        }
    }
}

/// A number field: its label, range and step.
#[derive(Clone, Copy)]
pub(crate) struct NumSpec {
    label: &'static str,
    min: f64,
    max: f64,
    step: f64,
    /// Show as a filled slider (bounded ranges) rather than a plain field.
    slider: bool,
}

pub(crate) const fn field(label: &'static str, min: f64, max: f64, step: f64) -> NumSpec {
    NumSpec {
        label,
        min,
        max,
        step,
        slider: false,
    }
}

const fn slider(label: &'static str, min: f64, max: f64) -> NumSpec {
    NumSpec {
        label,
        min,
        max,
        step: 1.0,
        slider: true,
    }
}

/// Simulation settings shown as overrides of the preset: key, field, and the preset default.
/// Reads one resolved simulation setting.
type SimField = fn(&azam_sim::SimValues) -> f64;

const SIM_FIELDS: [(&str, NumSpec, SimField); 7] = [
    ("gravity", field("Gravity (g)", 0.0, 2.0, 0.05), |v| v.gravity),
    ("air", field("Air Damping", 0.0, 3.0, 0.05), |v| v.air),
    ("substeps", field("Substeps", 2.0, 40.0, 1.0), |v| v.substeps),
    ("iterations", field("Iterations", 1.0, 12.0, 1.0), |v| {
        v.iterations
    }),
    (
        "clothThickness",
        field("Cloth Thickness (mm)", 0.5, 8.0, 0.1),
        |v| v.cloth_thickness,
    ),
    (
        "sewSpeed",
        field("Sewing Speed (mm/s)", 200.0, 6000.0, 50.0),
        |v| v.sew_speed,
    ),
    ("pdScale", field("Mesh Density Scale", 0.4, 2.5, 0.05), |v| {
        v.pd_scale
    }),
];

/// Simulation presets: key and label.
const SIM_PRESETS: [(&str, &str); 5] = [
    ("draft", "Draft"),
    ("normal", "Normal"),
    ("fine", "Fine"),
    ("layered", "Layered"),
    ("final", "Final"),
];

impl AzamApp {
    /// The properties editor's body.
    pub(crate) fn properties_body(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        let mut tabs = div().class("prop-tabs");
        for tab in PropTab::ALL {
            let class = if tab == self.prop_tab {
                "prop-tab active"
            } else {
                "prop-tab"
            };
            let button = div()
                .class(class)
                .child(self.icons.icon(tab.icon()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.prop_tab = tab;
                    // opening a tab reads the fabric library afresh (another project may have
                    // saved to it)
                    this.fabric_library = None;
                    cx.notify();
                }));
            tabs = tabs.child(Self::with_tooltip(button, tab.title().to_owned(), cx));
        }
        let title = div()
            .class("prop-title")
            .child(self.icons.icon(self.prop_tab.icon()))
            .child(self.prop_tab.title());
        let body = match self.prop_tab {
            PropTab::Project => self.project_panels(cx),
            PropTab::Piece => self.piece_panels(cx),
            PropTab::Part => self.part_panels(cx),
            PropTab::Fabric => self.fabric_panels(cx),
            PropTab::Avatar => self.avatar_panels(cx),
            PropTab::Simulation => self.sim_panels(cx),
        };
        div().class("props").child(tabs).child(
            div()
                .class("prop-body")
                .id(format!("prop-body-{area}"))
                .child(title)
                .children(body),
        )
    }

    /// Points of measure against every size, in cm.
    fn pom_table(&self) -> Div {
        let project = &self.session.project;
        if project.poms.is_empty() {
            return div()
                .class("pnote")
                .child("No points of measure in this project.");
        }
        let mut head = tr().child(th().child("Measure"));
        for size in &project.sizes {
            head = head.child(th().child(size.0.clone()));
        }
        let mut rows = Vec::new();
        for pom in project.poms.values() {
            let mut row = tr().child(td().child(pom.name.clone()));
            for size in &project.sizes {
                let value = azam_doc::pom_value(project, pom, size) / 10.0;
                let cell = td().child(format!("{value:.1}"));
                row = row.child(if *size == project.active_size {
                    cell.font_weight(FontWeight::BOLD)
                } else {
                    cell
                });
            }
            rows.push(row);
        }
        div()
            .class("pom-table")
            .child(table().child(thead().child(head)).child(tbody().children(rows)))
    }

    /// Each fabric's colour in every colorway.
    fn colorway_grid(&self) -> Div {
        let project = &self.session.project;
        let mut colorways: Vec<_> = project.colorways.values().collect();
        colorways.sort_by_key(|c| c.order);
        let mut head = tr().child(th().child("Fabric"));
        for cw in &colorways {
            head = head.child(th().child(cw.name.clone()));
        }
        let mut rows = Vec::new();
        for (fid, fabric) in &project.fabrics {
            let mut row = tr().child(td().child(fabric.name.clone()));
            for cw in &colorways {
                let color = cw
                    .over
                    .get(fid)
                    .and_then(|o| o.get("color"))
                    .and_then(Value::as_str)
                    .unwrap_or(&fabric.color);
                let swatch = div()
                    .class("swatch")
                    .bg(rgb(crate::scene::parse_hex(color).unwrap_or(0x888888)));
                row = row.child(td().child(swatch));
            }
            rows.push(row);
        }
        div()
            .class("pom-table")
            .child(table().child(thead().child(head)).child(tbody().children(rows)))
    }

    pub(crate) fn panel(&mut self, title: &'static str, rows: Vec<Div>) -> Div {
        div()
            .class("panel")
            .child(
                div()
                    .class("panel-head")
                    .child(self.icons.sized("DISCLOSURE_TRI_DOWN", 12.0, None))
                    .child(title),
            )
            .child(div().class("panel-body").children(rows))
    }

    /// A number field that applies `apply` to the project as an undoable edit when it changes.
    pub(crate) fn number(
        spec: NumSpec,
        value: f64,
        cx: &mut Context<Self>,
        apply: impl Fn(&mut Project, f64) + 'static,
    ) -> Div {
        let widget = if spec.slider {
            NumberSlider()
        } else {
            NumberField()
        };
        div().class("prow").child(
            widget
                .attr("label", spec.label)
                .value(format_num(value))
                .min(spec.min)
                .max(spec.max)
                .step(spec.step)
                .class("pfield")
                .on_input(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                    let v = e.number.clamp(spec.min, spec.max);
                    this.session.edit_continuous(spec.label, |p| apply(p, v));
                    cx.notify();
                })),
        )
    }

    /// A labelled dropdown of `items`; `apply` gets the chosen index.
    pub(crate) fn choice(
        label: &'static str,
        items: &[&str],
        selected: usize,
        cx: &mut Context<Self>,
        apply: impl Fn(&mut Project, usize) + 'static,
    ) -> Div {
        let mut select = Dropdown().class("pfield");
        for (i, item) in items.iter().enumerate() {
            select = select.child(option().child((*item).to_owned()).selected(i == selected));
        }
        div()
            .class("prow")
            .child(div().class("plabel").child(label))
            .child(select.on_change(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                let index = e.number.max(0.0) as usize;
                this.session.edit(label, |p| apply(p, index));
                cx.notify();
            })))
    }

    pub(crate) fn text_row(
        label: &'static str,
        value: String,
        cx: &mut Context<Self>,
        apply: impl Fn(&mut Project, &str) + 'static,
    ) -> Div {
        div()
            .class("prow")
            .child(div().class("plabel").child(label))
            .child(TextField().class("pfield").value(value).on_change(cx.listener(
                move |this, e: &ChangeEvent, _, cx| {
                    let text = e.value.clone();
                    this.session.edit(label, |p| apply(p, &text));
                    cx.notify();
                },
            )))
    }

    pub(crate) fn info_row(label: &'static str, value: String) -> Div {
        div()
            .class("prow")
            .child(div().class("plabel").child(label))
            .child(div().class("pvalue").child(value))
    }

    fn active_piece(&self) -> Option<PieceId> {
        self.session
            .selection
            .active
            .clone()
            .filter(|id| self.session.project.pieces.contains_key(id))
    }

    #[allow(clippy::too_many_lines, reason = "the panels of one tab, read top to bottom")]
    fn project_panels(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let project = &self.session.project;
        let name = project.meta.name.clone();
        let sizes: Vec<String> = project.sizes.iter().map(|s| s.0.clone()).collect();
        let active_size = project
            .sizes
            .iter()
            .position(|s| *s == project.active_size)
            .unwrap_or(0);
        let base_size = project
            .sizes
            .iter()
            .position(|s| *s == project.base_size)
            .unwrap_or(0);
        let stats = format!(
            "{} pieces · {} seams · {} fabrics",
            project.pieces.len(),
            project.sewing.len(),
            project.fabrics.len()
        );
        let colorways: Vec<(azam_doc::ColorwayId, String)> = project
            .colorways
            .iter()
            .map(|(id, c)| (id.clone(), c.name.clone()))
            .collect();
        let active_cw = colorways
            .iter()
            .position(|(id, _)| *id == project.active_colorway)
            .unwrap_or(0);
        let size_refs: Vec<&str> = sizes.iter().map(String::as_str).collect();
        let size_row = Self::choice("Active Size", &size_refs, active_size, cx, move |p, i| {
            if let Some(s) = p.sizes.get(i).cloned() {
                p.active_size = s;
            }
        });
        let run_row = Self::text_row("Size Run", sizes.join(", "), cx, |p, t| {
            let names: Vec<azam_geom::SizeName> = t.split(',').map(azam_geom::SizeName::from).collect();
            azam_doc::set_sizes(p, &names);
        });
        let base_row = Self::choice("Base Size", &size_refs, base_size, cx, move |p, i| {
            if let Some(s) = p.sizes.get(i).cloned() {
                azam_doc::set_base_size(p, &s);
            }
        });
        let cw_names: Vec<&str> = colorways.iter().map(|(_, n)| n.as_str()).collect();
        let cw_ids: Vec<azam_doc::ColorwayId> = colorways.iter().map(|(id, _)| id.clone()).collect();
        let cw_row = Self::choice("Colorway", &cw_names, active_cw, cx, move |p, i| {
            if let Some(id) = cw_ids.get(i) {
                p.active_colorway = id.clone();
            }
        });
        let cw_name_row = {
            let active = project.active_colorway.clone();
            let cw_name = colorways
                .get(active_cw)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            Self::text_row("Colorway Name", cw_name, cx, move |p, t| {
                if let Some(cw) = p.colorways.get_mut(&active) {
                    t.trim().clone_into(&mut cw.name);
                }
            })
        };
        let active_colorway = project.active_colorway.clone();
        let name_row = Self::text_row("Name", name, cx, |p, t| t.clone_into(&mut p.meta.name));
        let file = self
            .session
            .path
            .as_ref()
            .map_or_else(|| "Unsaved".to_owned(), |p| p.display().to_string());
        vec![
            self.panel(
                "Project",
                vec![
                    name_row,
                    Self::info_row("File", file),
                    Self::info_row("Contents", stats),
                ],
            ),
            self.panel("Sizes", vec![size_row, run_row, base_row, self.pom_table()]),
            self.panel(
                "Colorways",
                vec![
                    cw_row,
                    cw_name_row,
                    Self::colorway_buttons(&active_colorway, cx),
                    self.colorway_grid(),
                ],
            ),
        ]
    }

    #[allow(clippy::too_many_lines, reason = "the panels of one tab, read top to bottom")]
    fn piece_panels(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(id) = self.active_piece() else {
            return vec![
                div()
                    .class("empty")
                    .child("Select a piece in the pattern editor or the outliner."),
            ];
        };
        let project = &self.session.project;
        let piece = &project.pieces[&id];
        let (name, sa, grain, pd, layer, state) = (
            piece.name.clone(),
            piece.sa,
            piece.grain,
            piece.pd,
            piece.layer,
            piece.state,
        );
        let fabrics: Vec<(azam_doc::FabricId, String)> = project
            .fabrics
            .iter()
            .map(|(fid, f)| (fid.clone(), f.name.clone()))
            .collect();
        let fabric_index = fabrics
            .iter()
            .position(|(fid, _)| *fid == piece.fabric)
            .unwrap_or(0);
        let area = azam_doc::piece_area(project, piece, None, false) / 1.0e4;
        let points = piece.pts.len();
        let name_row = {
            let id = id.clone();
            Self::text_row("Name", name, cx, move |p, t| {
                if let Some(pc) = p.pieces.get_mut(&id) {
                    t.clone_into(&mut pc.name);
                    // a piece still in the default slot follows its name ("Back", "Sleeve R"…)
                    if pc.arr.slot == azam_doc::DEFAULT_SLOT
                        && let Some(slot) = azam_doc::slot_for_name(t)
                    {
                        slot.clone_into(&mut pc.arr.slot);
                        pc.arr.ang = None;
                    }
                }
            })
        };
        let fabric_names: Vec<&str> = fabrics.iter().map(|(_, n)| n.as_str()).collect();
        let fabric_ids: Vec<azam_doc::FabricId> = fabrics.iter().map(|(f, _)| f.clone()).collect();
        let fabric_row = {
            let id = id.clone();
            Self::choice("Fabric", &fabric_names, fabric_index, cx, move |p, i| {
                if let (Some(pc), Some(f)) = (p.pieces.get_mut(&id), fabric_ids.get(i)) {
                    pc.fabric = f.clone();
                }
            })
        };
        let state_index = match state {
            PieceState::Active => 0,
            PieceState::Frozen => 1,
            PieceState::Off => 2,
        };
        let state_row = {
            let id = id.clone();
            Self::choice(
                "Simulation",
                &["Active", "Frozen", "Off"],
                state_index,
                cx,
                move |p, i| {
                    if let Some(pc) = p.pieces.get_mut(&id) {
                        pc.state = [PieceState::Active, PieceState::Frozen, PieceState::Off][i.min(2)];
                    }
                },
            )
        };
        let piece_num = |spec, value, cx: &mut Context<Self>, set: fn(&mut azam_doc::Piece, f64)| {
            let id = id.clone();
            Self::number(spec, value, cx, move |p, v| {
                if let Some(pc) = p.pieces.get_mut(&id) {
                    set(pc, v);
                }
            })
        };
        let sa_row = piece_num(field("Seam Allowance (mm)", 0.0, 100.0, 1.0), sa, cx, |pc, v| {
            pc.sa = v;
        });
        let grain_row = piece_num(field("Grain (°)", -180.0, 180.0, 1.0), grain, cx, |pc, v| {
            pc.grain = v;
        });
        let pd_row = piece_num(
            field("Particle Distance (mm)", 3.0, 80.0, 1.0),
            pd,
            cx,
            |pc, v| pc.pd = v,
        );
        let layer_row = piece_num(field("Layer", -5.0, 5.0, 1.0), f64::from(layer), cx, |pc, v| {
            pc.layer = v.round() as i32;
        });
        vec![
            self.panel(
                "Piece",
                vec![
                    name_row,
                    Self::info_row("Area", format!("{area:.1} dm²")),
                    Self::info_row("Points", points.to_string()),
                ],
            ),
            self.panel("Material", vec![fabric_row, grain_row]),
            self.panel("Seam Allowance", vec![sa_row]),
            self.arrangement_panel(&id, cx),
            self.grading_panel(&id, cx),
            self.panel("Simulation", vec![state_row, pd_row, layer_row]),
        ]
    }

    /// Linear grading of the selected points of piece `id`: how far they move per size away from
    /// the base size, and what that gives in each size.
    fn grading_panel(&mut self, id: &azam_doc::PieceId, cx: &mut Context<Self>) -> Div {
        let project = &self.session.project;
        let piece = &project.pieces[id];
        let points: Vec<azam_geom::PointId> = self
            .session
            .selection
            .points
            .iter()
            .filter_map(|&i| piece.pts.get(i).map(|q| q.id.clone()))
            .collect();
        let Some(first) = points.first() else {
            return self.panel(
                "Grading",
                vec![
                    div()
                        .class("hint")
                        .child("Select points (Edit Points tool) to grade them."),
                ],
            );
        };
        let step = azam_doc::grade_step(project, id, first);
        let per_size: Vec<String> = project
            .sizes
            .iter()
            .enumerate()
            .map(|(i, size)| {
                let base = project
                    .sizes
                    .iter()
                    .position(|s| *s == project.base_size)
                    .unwrap_or(0);
                let k = i as f64 - base as f64;
                format!("{} {:+.0}, {:+.0}", size.0, step.x * k, step.y * k)
            })
            .collect();
        let count = format!("{} point(s), base {}", points.len(), project.base_size.0);
        let step_num = |label, value, cx: &mut Context<Self>, axis_x: bool| {
            let (id, points) = (id.clone(), points.clone());
            Self::number(field(label, -100.0, 100.0, 0.5), value, cx, move |p, v| {
                azam_doc::set_grade_step_axis(p, &id, &points, axis_x, v);
            })
        };
        let x_row = step_num("Step X (mm per size)", step.x, cx, true);
        let y_row = step_num("Step Y (mm per size)", step.y, cx, false);
        self.panel(
            "Grading",
            vec![
                Self::info_row("Selected", count),
                x_row,
                y_row,
                Self::info_row("Per Size", per_size.join(" · ")),
            ],
        )
    }

    /// Where piece `id` starts around the avatar before simulating: its slot (front, back, an
    /// arm…), distance from the body, angle around it and height. A new slot resets the angle to
    /// the slot's own.
    fn arrangement_panel(&mut self, id: &azam_doc::PieceId, cx: &mut Context<Self>) -> Div {
        let arr = self.session.project.pieces[id].arr.clone();
        let slot = azam_avatar::slot_by_id(&arr.slot);
        let labels: Vec<&str> = azam_avatar::SLOTS.iter().map(|s| s.label).collect();
        let index = azam_avatar::SLOTS
            .iter()
            .position(|s| s.id == slot.id)
            .unwrap_or(0);
        let slot_row = {
            let id = id.clone();
            Self::choice("Slot", &labels, index, cx, move |p, i| {
                if let (Some(pc), Some(s)) = (p.pieces.get_mut(&id), azam_avatar::SLOTS.get(i)) {
                    s.id.clone_into(&mut pc.arr.slot);
                    pc.arr.ang = None;
                }
            })
        };
        let arr_num = |spec, value, cx: &mut Context<Self>, set: fn(&mut azam_doc::Arrangement, f64)| {
            let id = id.clone();
            Self::number(spec, value, cx, move |p, v| {
                if let Some(pc) = p.pieces.get_mut(&id) {
                    set(&mut pc.arr, v);
                }
            })
        };
        let off_row = arr_num(
            field("Distance (mm)", 0.0, 400.0, 5.0),
            arr.off.unwrap_or(azam_avatar::DEFAULT_OFFSET_MM),
            cx,
            |a, v| a.off = Some(v),
        );
        let ang_row = arr_num(
            field("Angle (°)", -180.0, 180.0, 5.0),
            arr.ang.unwrap_or(slot.angle_deg),
            cx,
            |a, v| a.ang = Some(v),
        );
        let h_row = arr_num(
            field("Height (mm, 0 = auto)", 0.0, 2200.0, 10.0),
            arr.h.unwrap_or(0.0),
            cx,
            |a, v| a.h = (v > 0.0).then_some(v),
        );
        self.panel("Arrangement", vec![slot_row, off_row, ang_row, h_row])
    }

    #[allow(clippy::too_many_lines, reason = "the panels of one tab, read top to bottom")]
    fn fabric_panels(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        if self.fabric_library.is_none() {
            self.fabric_library = Some(
                crate::fabric_library::fabrics_dir()
                    .map(|d| crate::fabric_library::list(&d))
                    .unwrap_or_default(),
            );
        }
        let project = &self.session.project;
        let fabric_id = self
            .active_piece()
            .and_then(|id| project.pieces.get(&id).map(|p| p.fabric.clone()))
            .or_else(|| project.fabrics.keys().next().cloned());
        let Some(fid) = fabric_id.filter(|f| project.fabrics.contains_key(f)) else {
            return vec![div().class("empty").child("The project has no fabrics.")];
        };
        let fabric = &project.fabrics[&fid];
        let phys = fabric.phys.clone();
        let name = fabric.name.clone();
        let color = project
            .effective_fabric(&fid)
            .map_or_else(|| fabric.color.clone(), |f| f.color);
        let preset_index = FabricPreset::ALL
            .iter()
            .position(|p| p.key() == fabric.preset)
            .unwrap_or(0);
        let presets: Vec<&str> = FabricPreset::ALL.iter().map(|p| p.spec().name).collect();
        let preset_row = {
            let fid = fid.clone();
            Self::choice("Preset", &presets, preset_index, cx, move |p, i| {
                if let (Some(f), Some(preset)) = (p.fabrics.get_mut(&fid), FabricPreset::ALL.get(i)) {
                    preset.key().clone_into(&mut f.preset);
                    f.phys = preset.spec().physics();
                }
            })
        };
        let name_row = {
            let fid = fid.clone();
            Self::text_row("Name", name, cx, move |p, t| {
                if let Some(f) = p.fabrics.get_mut(&fid) {
                    t.clone_into(&mut f.name);
                }
            })
        };
        let color_row = {
            let fid = fid.clone();
            div()
                .class("prow")
                .child(div().class("plabel").child("Color"))
                .child(ColorSwatch().class("pfield").value(color).on_change(cx.listener(
                    move |this, e: &ChangeEvent, _, cx| {
                        let value = e.value.clone();
                        let fid = fid.clone();
                        this.session
                            .edit("Fabric Color", move |p| set_fabric_color(p, &fid, value));
                        cx.notify();
                    },
                )))
        };
        let phys_num = |spec, value, cx: &mut Context<Self>, set: fn(&mut azam_doc::FabricPhysics, f64)| {
            let fid = fid.clone();
            Self::number(spec, value, cx, move |p, v| {
                if let Some(f) = p.fabrics.get_mut(&fid) {
                    set(&mut f.phys, v);
                }
            })
        };
        let stretch = vec![
            phys_num(
                slider("Stretch Weft", 0.0, 100.0),
                phys.stretch_weft,
                cx,
                |f, v| f.stretch_weft = v,
            ),
            phys_num(
                slider("Stretch Warp", 0.0, 100.0),
                phys.stretch_warp,
                cx,
                |f, v| f.stretch_warp = v,
            ),
            phys_num(slider("Shear", 0.0, 100.0), phys.shear, cx, |f, v| f.shear = v),
        ];
        let bending = vec![
            phys_num(slider("Bending Weft", 0.0, 100.0), phys.bend_weft, cx, |f, v| {
                f.bend_weft = v;
            }),
            phys_num(slider("Bending Warp", 0.0, 100.0), phys.bend_warp, cx, |f, v| {
                f.bend_warp = v;
            }),
            phys_num(slider("Buckling Ratio", 0.0, 100.0), phys.buckling, cx, |f, v| {
                f.buckling = v;
            }),
        ];
        let other = vec![
            phys_num(
                field("Density (g/m²)", 10.0, 1500.0, 5.0),
                phys.density,
                cx,
                |f, v| f.density = v,
            ),
            phys_num(
                field("Thickness (mm)", 0.05, 10.0, 0.05),
                phys.thickness,
                cx,
                |f, v| f.thickness = v,
            ),
            phys_num(slider("Friction", 0.0, 100.0), phys.friction, cx, |f, v| {
                f.friction = v;
            }),
            phys_num(
                slider("Internal Damping", 0.0, 100.0),
                phys.damping,
                cx,
                |f, v| f.damping = v,
            ),
        ];
        vec![
            self.panel("Fabric", {
                let mut rows = vec![name_row, preset_row, color_row];
                rows.extend(self.fabric_library_rows(&fid, cx));
                rows
            }),
            self.panel("Stretch", stretch),
            self.panel("Bending", bending),
            self.panel("Physical Properties", other),
        ]
    }

    #[allow(clippy::too_many_lines, reason = "the panels of one tab, read top to bottom")]
    fn avatar_panels(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let body = self.session.project.avatar.body;
        let show = self.session.project.avatar.show;
        let sex_row = Self::choice(
            "Body",
            &["Female", "Male"],
            usize::from(body.sex == Sex::Male),
            cx,
            |p, i| {
                p.avatar.body = azam_doc::Measurements::preset(if i == 1 { Sex::Male } else { Sex::Female });
            },
        );
        let show_row = div()
            .class("prow")
            .child(div().class("plabel").child("Show"))
            .child(
                Checkbox()
                    .checked(show)
                    .on_change(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        let on = e.checked;
                        this.session.edit("Show Avatar", move |p| p.avatar.show = on);
                        cx.notify();
                    })),
            );
        let m = |spec, value, cx: &mut Context<Self>, set: fn(&mut azam_doc::Measurements, f64)| {
            Self::number(spec, value, cx, move |p, v| set(&mut p.avatar.body, v))
        };
        let girths = vec![
            m(field("Height", 1200.0, 2200.0, 5.0), body.height, cx, |b, v| {
                b.height = v;
            }),
            m(
                field("Bust / Chest", 600.0, 1600.0, 5.0),
                body.bust,
                cx,
                |b, v| b.bust = v,
            ),
            m(
                field("Underbust", 550.0, 1500.0, 5.0),
                body.underbust,
                cx,
                |b, v| b.underbust = v,
            ),
            m(field("Waist", 450.0, 1600.0, 5.0), body.waist, cx, |b, v| {
                b.waist = v;
            }),
            m(field("Hip", 650.0, 1700.0, 5.0), body.hip, cx, |b, v| b.hip = v),
            m(field("Neck", 250.0, 550.0, 5.0), body.neck, cx, |b, v| b.neck = v),
            m(field("Thigh", 350.0, 900.0, 5.0), body.thigh, cx, |b, v| {
                b.thigh = v;
            }),
            m(
                field("Upper Arm", 180.0, 500.0, 5.0),
                body.upper_arm,
                cx,
                |b, v| b.upper_arm = v,
            ),
        ];
        let lengths = vec![
            m(
                field("Shoulder Width", 280.0, 560.0, 5.0),
                body.shoulder,
                cx,
                |b, v| b.shoulder = v,
            ),
            m(field("Arm Length", 450.0, 750.0, 5.0), body.arm, cx, |b, v| {
                b.arm = v;
            }),
            m(
                field("Crotch Height", 600.0, 1000.0, 5.0),
                body.crotch,
                cx,
                |b, v| b.crotch = v,
            ),
        ];
        let pose = vec![
            m(
                field("Arm Angle (°)", 10.0, 80.0, 1.0),
                body.arm_angle,
                cx,
                |b, v| b.arm_angle = v,
            ),
            m(
                field("Leg Spread (°)", 0.0, 20.0, 0.5),
                body.leg_spread,
                cx,
                |b, v| b.leg_spread = v,
            ),
        ];
        let mut avatar_rows = vec![sex_row];
        if crate::avatar_motion::makehuman_available() {
            let mannequin = self.scene3d.mannequin;
            let mut select = Dropdown().class("pfield");
            for (i, item) in ["MakeHuman", "Mannequin"].into_iter().enumerate() {
                select = select.child(option().child(item).selected(usize::from(mannequin) == i));
            }
            avatar_rows.push(
                div()
                    .class("prow")
                    .child(div().class("plabel").child("Model"))
                    .child(select.on_change(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        this.scene3d.set_mannequin(e.number >= 1.0);
                        cx.notify();
                    }))),
            );
        }
        let body_buttons = {
            let button = |label: &'static str, op: crate::operators::Operator, cx: &mut Context<Self>| {
                div()
                    .class("pbtn")
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.run(op, window, cx)))
            };
            div()
                .class("pbuttons")
                .child(button(
                    "Custom Body…",
                    crate::operators::Operator::ImportAvatar,
                    cx,
                ))
                .child(button(
                    "Built-in Body",
                    crate::operators::Operator::UseBuiltinBody,
                    cx,
                ))
        };
        avatar_rows.extend([
            show_row,
            body_buttons,
            div().class("pnote").child(self.scene3d.avatar_status()),
            div().class("pnote").child("All measurements in mm."),
        ]);
        vec![
            self.panel("Avatar", avatar_rows),
            self.panel("Girths", girths),
            self.panel("Lengths", lengths),
            self.panel("Pose", pose),
        ]
    }

    #[allow(clippy::too_many_lines, reason = "the panels of one tab, read top to bottom")]
    fn sim_panels(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let sim = self.session.project.sim.clone();
        let preset = sim.preset.as_deref().unwrap_or("normal");
        let preset_index = SIM_PRESETS.iter().position(|(k, _)| *k == preset).unwrap_or(1);
        let labels: Vec<&str> = SIM_PRESETS.iter().map(|(_, l)| *l).collect();
        let preset_row = Self::choice("Preset", &labels, preset_index, cx, |p, i| {
            p.sim.preset = SIM_PRESETS.get(i).map(|(k, _)| (*k).to_owned());
            p.sim.over = None;
        });
        let mut rows = vec![preset_row];
        let resolved = azam_sim::resolve_settings(&sim).values;
        for (key, spec, current) in SIM_FIELDS {
            let value = current(&resolved);
            rows.push(Self::number(spec, value, cx, move |p, v| {
                let over = p.sim.over.get_or_insert_with(serde_json::Map::new);
                over.insert(key.to_owned(), Value::from(v));
            }));
        }
        let self_collision = resolved.self_collision;
        rows.push(
            div()
                .class("prow")
                .child(div().class("plabel").child("Self Collision"))
                .child(Checkbox().checked(self_collision).on_change(cx.listener(
                    |this, e: &ChangeEvent, _, cx| {
                        let on = e.checked;
                        this.session.edit("Self Collision", move |p| {
                            p.sim
                                .over
                                .get_or_insert_with(serde_json::Map::new)
                                .insert("selfCollision".into(), Value::from(on));
                        });
                        cx.notify();
                    },
                ))),
        );
        let status = self.sim_status();
        let gpu_on = !self.scene3d.cpu_only;
        let gpu_row = div()
            .class("prow")
            .child(div().class("plabel").child("GPU Solver"))
            .child(
                Checkbox()
                    .checked(gpu_on)
                    .on_change(cx.listener(|this, e: &ChangeEvent, _, cx| {
                        this.scene3d.set_use_gpu(e.checked);
                        cx.notify();
                    })),
            );
        let mut status_rows = vec![gpu_row, div().class("pnote").child(status)];
        if let Some(error) = &self.scene3d.gpu_error {
            status_rows.push(
                div()
                    .class("pnote status-warn")
                    .child(format!("GPU unavailable, using the CPU: {error}")),
            );
        }
        vec![self.panel("Solver", rows), self.panel("Status", status_rows)]
    }
}

/// Sets fabric `fid`'s colour as the active colorway sees it: the fabric itself in the first
/// colorway, an override in the others.
fn set_fabric_color(p: &mut Project, fid: &azam_doc::FabricId, color: String) {
    let base = p.colorways.values().min_by_key(|c| c.order).map(|c| c.id.clone());
    if base.is_none() || base.as_ref() == Some(&p.active_colorway) {
        if let Some(f) = p.fabrics.get_mut(fid) {
            f.color = color;
        }
    } else if let Some(cw) = p.colorways.get_mut(&p.active_colorway) {
        cw.over
            .entry(fid.clone())
            .or_default()
            .insert("color".into(), Value::from(color));
    }
}

/// A number as a field shows it: integers without a fraction, others to two decimals.
fn format_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}
