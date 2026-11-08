//! The outliner: the project as a tree of pieces, seams, trims, fabrics and the avatar, with
//! Blender's visibility and freeze toggles.

use azam_doc::{Extras, ItemKind, PieceId, PieceState, SeamId};
use blenderui::prelude::*;

use crate::app::AzamApp;
use crate::properties::PropTab;
use crate::scene::{parse_hex, piece_color};
use crate::selection::Part;

/// Indent per tree level, in logical pixels.
const INDENT_PX: f32 = 14.0;

impl AzamApp {
    /// The outliner's body.
    pub(crate) fn outliner_body(&mut self, area: crate::areas::AreaId, cx: &mut Context<Self>) -> Div {
        let project = &self.session.project;
        let mut pieces: Vec<(PieceId, String, PieceState, u32)> = project
            .pieces
            .values()
            .map(|p| (p.id.clone(), p.name.clone(), p.state, piece_color(project, &p.id)))
            .collect();
        pieces.sort_by_key(|(id, ..)| project.pieces[id].order);
        let numbers = azam_doc::seam_numbers(project);
        // In seam-number order (the numbers the pattern editor shows).
        let seams: Vec<(SeamId, String, u32)> = numbers
            .iter()
            .filter_map(|(id, &number)| project.sewing.get(id).map(|s| (s, number)))
            .map(|(s, number)| {
                let piece_name = |sides: &[azam_doc::SeamSide]| {
                    sides
                        .first()
                        .and_then(|side| project.pieces.get(&side.piece))
                        .map_or("?", |p| p.name.as_str())
                        .to_owned()
                };
                let label = format!("Seam {number} · {} ↔ {}", piece_name(&s.a), piece_name(&s.b));
                (s.id.clone(), label, parse_hex(&s.color).unwrap_or(0xffcc33))
            })
            .collect();
        let fabrics: Vec<(String, u32)> = project
            .fabrics
            .values()
            .map(|f| (f.name.clone(), parse_hex(&f.color).unwrap_or(0x888888)))
            .collect();
        let project_name = if project.meta.name.is_empty() {
            "Untitled".to_owned()
        } else {
            project.meta.name.clone()
        };

        let mut rows = vec![self.ol_group(0, "OUTLINER_COLLECTION", project_name)];
        rows.push(self.ol_group(1, "OUTLINER_COLLECTION", format!("Pieces ({})", pieces.len())));
        for (id, name, state, color) in pieces {
            rows.push(self.ol_piece(&id, name, state, color, cx));
        }
        rows.push(self.ol_group(1, "OUTLINER_COLLECTION", format!("Seams ({})", seams.len())));
        for (id, label, color) in seams {
            let selected = self.session.selection.seam.as_ref() == Some(&id);
            rows.push(
                Self::ol_row(2, selected, false)
                    .child(div().class("ol-swatch").bg(rgb(color)).rounded_full())
                    .child(div().class("ol-name").child(label))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.show_seam(id.clone());
                        cx.notify();
                    })),
            );
        }
        rows.extend(self.ol_trims(cx));
        rows.push(self.ol_group(1, "OUTLINER_COLLECTION", format!("Fabrics ({})", fabrics.len())));
        for (name, color) in fabrics {
            rows.push(
                Self::ol_row(2, false, false)
                    .child(self.icons.icon("MATERIAL"))
                    .child(div().class("ol-swatch").bg(rgb(color)))
                    .child(div().class("ol-name").child(name))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.prop_tab = PropTab::Fabric;
                        cx.notify();
                    })),
            );
        }
        let show = self.session.project.avatar.show;
        let eye = if show { "HIDE_OFF" } else { "HIDE_ON" };
        rows.push(
            Self::ol_row(1, false, false)
                .child(self.icons.icon("ARMATURE_DATA"))
                .child(div().class("ol-name").child("Avatar"))
                .child(
                    self.icons
                        .icon(eye)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.session
                                .edit("Show Avatar", |p| p.avatar.show = !p.avatar.show);
                            cx.notify();
                        })),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.prop_tab = PropTab::Avatar;
                    cx.notify();
                })),
        );
        div()
            .class("outliner")
            .id(format!("outliner-{area}"))
            .children(rows)
    }

    /// Topstitches, buttons, buttonholes and comments, each selectable.
    fn ol_trims(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let project = &self.session.project;
        let piece_name = |p: Option<&PieceId>| {
            p.and_then(|p| project.pieces.get(p))
                .map_or_else(|| "?".to_owned(), |p| p.name.clone())
        };
        let mut items: Vec<(Part, &'static str, String)> = project
            .stitches
            .values()
            .map(|st| {
                (
                    Part::Stitch(st.id.clone()),
                    "MOD_DASH",
                    format!("Topstitch · {}", piece_name(st.p.as_ref())),
                )
            })
            .collect();
        for (kind, icon, noun) in [
            (ItemKind::Button, "RADIOBUT_ON", "Button"),
            (ItemKind::Hole, "REMOVE", "Buttonhole"),
            (ItemKind::Comment, "TEXT", "Comment"),
        ] {
            items.extend(project.items(kind).values().map(|it| {
                let label = match it
                    .text(azam_doc::COMMENT_TEXT_KEY)
                    .filter(|_| kind == ItemKind::Comment)
                {
                    Some(text) => format!("{noun} · {text}"),
                    None => format!("{noun} · {}", piece_name(it.p.as_ref())),
                };
                (Part::Item(kind, it.id.clone()), icon, label)
            }));
        }
        if items.is_empty() {
            return Vec::new();
        }
        let mut rows = vec![self.ol_group(1, "OUTLINER_COLLECTION", format!("Trims ({})", items.len()))];
        for (part, icon, label) in items {
            let selected = self.session.selection.part.as_ref() == Some(&part);
            rows.push(
                Self::ol_row(2, selected, false)
                    .child(self.icons.icon(icon))
                    .child(div().class("ol-name").child(label))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.show_part(part.clone());
                        cx.notify();
                    })),
            );
        }
        rows
    }

    fn ol_row(depth: usize, selected: bool, active: bool) -> Div {
        let class = if active {
            "ol-row active"
        } else if selected {
            "ol-row selected"
        } else {
            "ol-row"
        };
        div().class(class).pl(px(6.0 + depth as f32 * INDENT_PX))
    }

    fn ol_group(&mut self, depth: usize, icon: &'static str, label: String) -> Div {
        Self::ol_row(depth, false, false)
            .child(self.icons.sized("DISCLOSURE_TRI_DOWN", 12.0, None))
            .child(self.icons.icon(icon))
            .child(div().class("ol-name").child(label))
    }

    fn ol_piece(
        &mut self,
        id: &PieceId,
        name: String,
        state: PieceState,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let selection = &self.session.selection;
        let selected = selection.pieces.contains(id);
        let active = selection.active.as_ref() == Some(id);
        let (eye, freeze) = (
            if state == PieceState::Off {
                "HIDE_ON"
            } else {
                "HIDE_OFF"
            },
            if state == PieceState::Frozen {
                "FREEZE"
            } else {
                "UNLOCKED"
            },
        );
        let (toggle_off, toggle_freeze, select) = (id.clone(), id.clone(), id.clone());
        Self::ol_row(2, selected, active)
            .child(self.icons.icon("MESH_DATA"))
            .child(div().class("ol-swatch").bg(rgb(color)))
            .child(div().class("ol-name").child(name))
            .child(
                self.icons
                    .icon(freeze)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        let id = toggle_freeze.clone();
                        this.session.edit("Freeze Piece", move |p| {
                            if let Some(pc) = p.pieces.get_mut(&id) {
                                pc.state = if pc.state == PieceState::Frozen {
                                    PieceState::Active
                                } else {
                                    PieceState::Frozen
                                };
                            }
                        });
                        cx.notify();
                    })),
            )
            .child(
                self.icons
                    .icon(eye)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        let id = toggle_off.clone();
                        this.session.edit("Toggle Piece", move |p| {
                            if let Some(pc) = p.pieces.get_mut(&id) {
                                pc.state = if pc.state == PieceState::Off {
                                    PieceState::Active
                                } else {
                                    PieceState::Off
                                };
                            }
                        });
                        cx.notify();
                    })),
            )
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                if e.down.modifiers.shift || e.down.modifiers.control {
                    this.session.selection.toggle(select.clone());
                } else {
                    this.session.selection.select_only(select.clone());
                }
                this.prop_tab = PropTab::Piece;
                cx.notify();
            }))
    }
}
