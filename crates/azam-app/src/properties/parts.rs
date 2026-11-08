//! The Item tab of the properties editor: the selected seam, segment, notch, topstitch, button,
//! buttonhole, comment, internal line or graphic.

use azam_doc::{Extras, ItemId, ItemKind, PieceId, PointId, Project, SeamId, StitchId};
use blenderui::prelude::*;
use serde_json::Value;

use super::{field, slider};
use crate::app::AzamApp;
use crate::operators::Operator;
use crate::selection::Part;

/// Choices of a button's shape: label and stored value.
const BUTTON_SHAPES: [(&str, &str); 3] = [("Round", "round"), ("Square", "square"), ("Shank", "shank")];
/// Choices of a button's finish: label and stored value.
const BUTTON_FINISHES: [(&str, &str); 2] = [("Matte", "plastic"), ("Metal", "metal")];
/// Choices of a button's number of holes.
const BUTTON_HOLE_COUNTS: [u32; 3] = [0, 2, 4];
/// Fold angle given to a line when it becomes a fold, degrees.
const DEFAULT_FOLD_DEG: f64 = 90.0;

/// A row with a label and a value, both computed.
fn info(label: String, value: String) -> Div {
    div()
        .class("prow")
        .child(div().class("plabel").child(label))
        .child(div().class("pvalue").child(value))
}

impl AzamApp {
    /// The Item tab's panels.
    pub(super) fn part_panels(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let sel = self.session.selection.clone();
        if let Some(seam) = sel.seam {
            return self.seam_panels(&seam, cx);
        }
        match sel.part {
            Some(Part::Notch { piece, index }) => return self.notch_panels(&piece, index, cx),
            Some(Part::Internal { piece, index }) => return self.internal_panels(&piece, index, cx),
            Some(Part::Graphic { piece, index }) => return self.graphic_panels(&piece, index, cx),
            Some(Part::Stitch(id)) => return self.stitch_panels(&id, cx),
            Some(Part::Item(ItemKind::Button, id)) => return self.button_panels(&id, cx),
            Some(Part::Item(ItemKind::Hole, id)) => return self.hole_panels(&id, cx),
            Some(Part::Item(ItemKind::Comment, id)) => return self.comment_panels(&id, cx),
            None => {}
        }
        if let Some((piece, segment)) = sel.segment {
            return self.segment_panels(&piece, &segment, cx);
        }
        vec![
            div()
                .class("empty")
                .child("Select a seam, segment, notch, topstitch, button, buttonhole, comment, internal line or graphic in the pattern editor."),
        ]
    }

    /// A row of buttons running operators.
    fn op_buttons(ops: &[(&'static str, Operator)], cx: &mut Context<Self>) -> Div {
        let mut row = div().class("pbuttons");
        for &(label, op) in ops {
            let class = if op == Operator::Delete {
                "pbtn danger"
            } else {
                "pbtn"
            };
            row = row.child(
                div()
                    .class(class)
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.run(op, window, cx))),
            );
        }
        div().class("prow").child(row)
    }

    /// A colour swatch row that sets a colour with `apply`.
    fn color_row(
        label: &'static str,
        value: String,
        cx: &mut Context<Self>,
        apply: impl Fn(&mut Project, String) + 'static,
    ) -> Div {
        div()
            .class("prow")
            .child(div().class("plabel").child(label))
            .child(ColorSwatch().class("pfield").value(value).on_change(cx.listener(
                move |this, e: &ChangeEvent, _, cx| {
                    let value = e.value.clone();
                    this.session.edit(label, |p| apply(p, value));
                    cx.notify();
                },
            )))
    }

    /// A checkbox row that sets a flag with `apply`.
    fn check_row(
        label: &'static str,
        on: bool,
        cx: &mut Context<Self>,
        apply: impl Fn(&mut Project, bool) + 'static,
    ) -> Div {
        div()
            .class("prow")
            .child(div().class("plabel").child(label))
            .child(
                Checkbox()
                    .checked(on)
                    .on_change(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                        let checked = e.checked;
                        this.session.edit(label, |p| apply(p, checked));
                        cx.notify();
                    })),
            )
    }

    fn piece_name(&self, piece: &PieceId) -> String {
        self.session
            .project
            .pieces
            .get(piece)
            .map_or_else(|| "—".to_owned(), |p| p.name.clone())
    }

    fn seam_panels(&mut self, seam: &SeamId, cx: &mut Context<Self>) -> Vec<Div> {
        let project = &self.session.project;
        let Some(sw) = project.sewing.get(seam) else {
            return Vec::new();
        };
        let number = azam_doc::seam_numbers(project).get(seam).copied().unwrap_or(0);
        let lengths = azam_doc::sew_lengths(project).get(seam).copied();
        let sa = azam_doc::seam_sa(project, sw);
        let color = sw.color.clone();
        let ratio = format!("{}:{}", sw.a.len(), sw.b.len());
        let mut sides = Vec::new();
        for (group, list) in [("A", &sw.a), ("B", &sw.b)] {
            for (i, side) in list.iter().enumerate() {
                let length = project
                    .pieces
                    .get(&side.piece)
                    .and_then(|pc| {
                        side.side
                            .range(&pc.graded_outline(&project.active_size, &project.base_size))
                    })
                    .map_or_else(
                        || "–".to_owned(),
                        |r| crate::units::text(self.session.project.units, r.length).to_string(),
                    );
                sides.push(info(
                    format!("{group}{} {}", i + 1, self.piece_name(&side.piece)),
                    length,
                ));
            }
        }
        let sa_row = {
            let seam = seam.clone();
            Self::number(
                field("Seam Allowance (mm)", 0.0, 80.0, 1.0),
                sa,
                cx,
                move |p, v| {
                    azam_doc::set_seam_sa(p, &seam, v);
                },
            )
        };
        let color_row = {
            let seam = seam.clone();
            Self::color_row("Seam Color", color, cx, move |p, v| {
                if let Some(sw) = p.sewing.get_mut(&seam) {
                    sw.color = v;
                }
            })
        };
        let mut rows = vec![info("Seam".into(), format!("#{number} · {ratio}")), sa_row];
        rows.extend(sides);
        if let Some(l) = lengths {
            rows.push(info(
                "Total A / B".into(),
                format!(
                    "{} / {}",
                    crate::units::number(self.session.project.units, l.a),
                    crate::units::text(self.session.project.units, l.b)
                ),
            ));
            // Adding zero turns a rounded -0.0 into 0.0.
            let diff = (((l.a - l.b) * 10.0).round() / 10.0) + 0.0;
            rows.push(info("Difference".into(), format!("{diff:+.1} mm")));
        }
        rows.push(color_row);
        rows.push(Self::op_buttons(
            &[
                ("Flip Side B", Operator::FlipSeam),
                ("Swap Sides", Operator::SwapSeamSides),
                ("Delete", Operator::Delete),
            ],
            cx,
        ));
        vec![self.panel("Seam", rows)]
    }

    fn segment_panels(&mut self, piece: &PieceId, segment: &PointId, cx: &mut Context<Self>) -> Vec<Div> {
        let project = &self.session.project;
        let Some(pc) = project.pieces.get(piece) else {
            return Vec::new();
        };
        let ol = pc.graded_outline(&project.active_size, &project.base_size);
        let Some(seg) = ol.segment_index(segment).map(|k| &ol.segments[k]) else {
            return Vec::new();
        };
        let length = seg.len;
        let sa = pc
            .sa_seg
            .as_ref()
            .and_then(|m| m.get(segment))
            .copied()
            .unwrap_or(pc.sa);
        let hem = pc
            .hem
            .as_ref()
            .and_then(|m| m.get(segment))
            .copied()
            .unwrap_or(0.0);
        let name = pc.name.clone();
        let sa_row = {
            let (piece, segment) = (piece.clone(), segment.clone());
            Self::number(
                field("Segment Allowance (mm)", 0.0, 100.0, 1.0),
                sa,
                cx,
                move |p, v| {
                    azam_doc::set_segment_sa(p, &piece, &segment, v);
                },
            )
        };
        let hem_row = {
            let (piece, segment) = (piece.clone(), segment.clone());
            Self::number(field("Hem (mm)", 0.0, 100.0, 1.0), hem, cx, move |p, v| {
                azam_doc::set_hem(p, &piece, &segment, v);
            })
        };
        vec![self.panel(
            "Segment",
            vec![
                info("Piece".into(), name),
                info(
                    "Length".into(),
                    crate::units::text(self.session.project.units, length).to_string(),
                ),
                sa_row,
                hem_row,
            ],
        )]
    }

    fn notch_panels(&mut self, piece: &PieceId, index: usize, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(n) = self
            .session
            .project
            .pieces
            .get(piece)
            .and_then(|pc| pc.notches.get(index))
        else {
            return Vec::new();
        };
        let t = n.t.unwrap_or(0.0);
        let position = {
            let piece = piece.clone();
            Self::number(
                field("Position Along Segment", 0.0, 1.0, 0.01),
                t,
                cx,
                move |p, v| {
                    if let Some(n) = p.pieces.get_mut(&piece).and_then(|pc| pc.notches.get_mut(index)) {
                        n.t = Some(v);
                    }
                },
            )
        };
        let name = self.piece_name(piece);
        let delete = Self::op_buttons(&[("Delete", Operator::Delete)], cx);
        vec![self.panel("Notch", vec![info("Piece".into(), name), position, delete])]
    }

    fn stitch_panels(&mut self, id: &StitchId, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(st) = self.session.project.stitches.get(id) else {
            return Vec::new();
        };
        let style = azam_doc::StitchStyle::of(st);
        let whole = st.seg.0 == azam_doc::WHOLE_OUTLINE;
        let piece = st.p.clone();
        let name = piece
            .as_ref()
            .map_or_else(|| "—".to_owned(), |p| self.piece_name(p));
        let applies = {
            let id = id.clone();
            Self::choice(
                "Applies To",
                &["Segment", "Whole Outline"],
                usize::from(whole),
                cx,
                move |p, i| {
                    let first = p
                        .stitches
                        .get(&id)
                        .and_then(|st| st.p.as_ref())
                        .and_then(|pc| p.pieces.get(pc))
                        .and_then(|pc| pc.pts.first())
                        .map(|pt| pt.id.clone());
                    if let Some(st) = p.stitches.get_mut(&id) {
                        if i == 1 {
                            st.seg = PointId::new(azam_doc::WHOLE_OUTLINE);
                        } else if st.seg.0 == azam_doc::WHOLE_OUTLINE
                            && let Some(first) = first
                        {
                            st.seg = first;
                        }
                    }
                },
            )
        };
        let num = |spec, value, key: &'static str, cx: &mut Context<Self>| {
            let id = id.clone();
            Self::number(spec, value, cx, move |p, v| {
                if let Some(st) = p.stitches.get_mut(&id) {
                    st.set(key, v);
                }
            })
        };
        let mut rows = vec![
            info("Piece".into(), name),
            applies,
            num(
                field("Distance From Edge (mm)", 0.5, 60.0, 0.5),
                style.offset,
                azam_doc::STITCH_OFFSET_KEY,
                cx,
            ),
            num(
                field("Rows", 1.0, 4.0, 1.0),
                f64::from(style.rows),
                azam_doc::STITCH_ROWS_KEY,
                cx,
            ),
        ];
        if style.rows > 1 {
            rows.push(num(
                field("Row Spacing (mm)", 1.0, 30.0, 0.5),
                style.row_gap,
                azam_doc::STITCH_ROW_GAP_KEY,
                cx,
            ));
        }
        rows.extend([
            num(
                field("Stitch Length (mm)", 0.5, 15.0, 0.1),
                style.length,
                azam_doc::STITCH_LENGTH_KEY,
                cx,
            ),
            num(
                field("Gap (mm)", 0.2, 10.0, 0.1),
                style.gap,
                azam_doc::STITCH_GAP_KEY,
                cx,
            ),
            num(
                field("Thread Width (mm)", 0.2, 4.0, 0.1),
                style.width,
                azam_doc::STITCH_WIDTH_KEY,
                cx,
            ),
        ]);
        let id2 = id.clone();
        rows.push(Self::color_row("Thread Color", style.color, cx, move |p, v| {
            if let Some(st) = p.stitches.get_mut(&id2) {
                st.set(azam_doc::COLOR_KEY, v);
            }
        }));
        rows.push(Self::op_buttons(&[("Delete", Operator::Delete)], cx));
        vec![self.panel("Topstitch", rows)]
    }

    fn button_panels(&mut self, id: &ItemId, cx: &mut Context<Self>) -> Vec<Div> {
        let project = &self.session.project;
        let Some(b) = project.buttons.get(id) else {
            return Vec::new();
        };
        let set_text = |key: &'static str, values: &'static [(&'static str, &'static str)]| {
            let id = id.clone();
            move |p: &mut Project, i: usize| {
                if let (Some(b), Some((_, v))) = (p.buttons.get_mut(&id), values.get(i)) {
                    b.set(key, *v);
                }
            }
        };
        let shape = b
            .text(azam_doc::BUTTON_SHAPE_KEY)
            .unwrap_or(azam_doc::DEFAULT_BUTTON_SHAPE);
        let shape_index = BUTTON_SHAPES.iter().position(|(_, v)| *v == shape).unwrap_or(0);
        let finish = b
            .text(azam_doc::BUTTON_FINISH_KEY)
            .unwrap_or(azam_doc::DEFAULT_BUTTON_FINISH);
        let finish_index = BUTTON_FINISHES
            .iter()
            .position(|(_, v)| *v == finish)
            .unwrap_or(0);
        let holes = b.num(
            azam_doc::BUTTON_HOLES_KEY,
            f64::from(azam_doc::DEFAULT_BUTTON_HOLES),
        );
        let holes_index = BUTTON_HOLE_COUNTS
            .iter()
            .position(|&h| (f64::from(h) - holes).abs() < 0.5)
            .unwrap_or(2);
        let size = b.num(azam_doc::BUTTON_SIZE_KEY, azam_doc::DEFAULT_BUTTON_SIZE_MM);
        let color = b
            .text(azam_doc::COLOR_KEY)
            .unwrap_or(azam_doc::DEFAULT_BUTTON_COLOR)
            .to_owned();
        let shape_labels: Vec<&str> = BUTTON_SHAPES.iter().map(|(l, _)| *l).collect();
        let finish_labels: Vec<&str> = BUTTON_FINISHES.iter().map(|(l, _)| *l).collect();
        let shape_row = Self::choice(
            "Shape",
            &shape_labels,
            shape_index,
            cx,
            set_text(azam_doc::BUTTON_SHAPE_KEY, &BUTTON_SHAPES),
        );
        let finish_row = Self::choice(
            "Finish",
            &finish_labels,
            finish_index,
            cx,
            set_text(azam_doc::BUTTON_FINISH_KEY, &BUTTON_FINISHES),
        );
        let holes_row = {
            let id = id.clone();
            Self::choice("Holes", &["0", "2", "4"], holes_index, cx, move |p, i| {
                if let (Some(b), Some(&h)) = (p.buttons.get_mut(&id), BUTTON_HOLE_COUNTS.get(i)) {
                    b.set(azam_doc::BUTTON_HOLES_KEY, h);
                }
            })
        };
        let size_row = {
            let id = id.clone();
            Self::number(field("Size (mm)", 5.0, 60.0, 1.0), size, cx, move |p, v| {
                if let Some(b) = p.buttons.get_mut(&id) {
                    b.set(azam_doc::BUTTON_SIZE_KEY, v);
                }
            })
        };
        let color_row = {
            let id = id.clone();
            Self::color_row("Button Color", color, cx, move |p, v| {
                if let Some(b) = p.buttons.get_mut(&id) {
                    b.set(azam_doc::COLOR_KEY, v);
                }
            })
        };
        let fasten_row = self.fasten_row(id, cx);
        let delete = Self::op_buttons(&[("Delete", Operator::Delete)], cx);
        vec![self.panel(
            "Button",
            vec![
                shape_row, size_row, holes_row, color_row, finish_row, fasten_row, delete,
            ],
        )]
    }

    /// The buttonhole button `id` is fastened to, as a choice of every buttonhole.
    fn fasten_row(&self, id: &ItemId, cx: &mut Context<Self>) -> Div {
        let project = &self.session.project;
        let fastened = project
            .buttons
            .get(id)
            .and_then(|b| b.text(azam_doc::BUTTON_FASTEN_KEY))
            .map(str::to_owned);
        let holes: Vec<(ItemId, String)> = project
            .holes
            .values()
            .map(|h| {
                let piece = h.p.as_ref().map_or_else(String::new, |p| self.piece_name(p));
                let len = h.num(azam_doc::HOLE_LENGTH_KEY, azam_doc::DEFAULT_HOLE_LENGTH_MM);
                (h.id.clone(), format!("{piece} · {len:.0} mm"))
            })
            .collect();
        let index = fastened
            .as_deref()
            .and_then(|f| holes.iter().position(|(h, _)| h.as_str() == f))
            .map_or(0, |i| i + 1);
        let mut labels = vec!["— none —".to_owned()];
        labels.extend(holes.iter().map(|(_, l)| l.clone()));
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let hole_ids: Vec<ItemId> = holes.into_iter().map(|(h, _)| h).collect();
        let id = id.clone();
        Self::choice("Fastened To", &refs, index, cx, move |p, i| {
            let hole = i.checked_sub(1).and_then(|k| hole_ids.get(k));
            azam_doc::fasten_button(p, &id, hole);
        })
    }

    fn hole_panels(&mut self, id: &ItemId, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(h) = self.session.project.holes.get(id) else {
            return Vec::new();
        };
        let length = h.num(azam_doc::HOLE_LENGTH_KEY, azam_doc::DEFAULT_HOLE_LENGTH_MM);
        let angle = h.num(azam_doc::HOLE_ANGLE_KEY, 0.0);
        let color = h
            .text(azam_doc::COLOR_KEY)
            .unwrap_or(azam_doc::DEFAULT_HOLE_COLOR)
            .to_owned();
        let num = |spec, value, key: &'static str, cx: &mut Context<Self>| {
            let id = id.clone();
            Self::number(spec, value, cx, move |p, v| {
                if let Some(h) = p.holes.get_mut(&id) {
                    h.set(key, v);
                }
            })
        };
        let rows = vec![
            num(
                field("Length (mm)", 5.0, 60.0, 1.0),
                length,
                azam_doc::HOLE_LENGTH_KEY,
                cx,
            ),
            num(
                slider("Angle (°)", -90.0, 90.0),
                angle,
                azam_doc::HOLE_ANGLE_KEY,
                cx,
            ),
            {
                let id = id.clone();
                Self::color_row("Thread Color", color, cx, move |p, v| {
                    if let Some(h) = p.holes.get_mut(&id) {
                        h.set(azam_doc::COLOR_KEY, v);
                    }
                })
            },
            Self::op_buttons(&[("Delete", Operator::Delete)], cx),
        ];
        vec![self.panel("Buttonhole", rows)]
    }

    /// A comment's replies and the field to post another.
    fn comment_thread(&self, id: &ItemId, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(c) = self.session.project.comments.get(id) else {
            return Vec::new();
        };
        let replies: Vec<Div> = c
            .extra
            .get(COMMENT_REPLIES_KEY)
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|r| {
                        let by = r
                            .get("by")
                            .and_then(Value::as_str)
                            .unwrap_or("Someone")
                            .to_owned();
                        let at = r
                            .get("at")
                            .and_then(Value::as_f64)
                            .map(format_time)
                            .unwrap_or_default();
                        let text = r.get("text").and_then(Value::as_str).unwrap_or("").to_owned();
                        div()
                            .class("reply")
                            .child(div().class("reply-head").child(format!("{by} · {at}")))
                            .child(div().class("reply-text").child(text))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let reply_row = {
            let id = id.clone();
            div()
                .class("prow")
                .child(div().class("plabel").child("Reply"))
                .child(
                    TextField()
                        .class("pfield")
                        .placeholder("Write a reply, Enter to post")
                        .value(String::new())
                        .on_change(cx.listener(move |this, e: &ChangeEvent, _, cx| {
                            let text = e.value.trim().to_owned();
                            if text.is_empty() {
                                return;
                            }
                            let id = id.clone();
                            this.session.edit("Reply", move |p| {
                            if let Some(c) = p.comments.get_mut(&id) {
                                let reply =
                                    serde_json::json!({ "by": user_name(), "at": now_ms(), "text": text });
                                match c.extra.get_mut(COMMENT_REPLIES_KEY).and_then(Value::as_array_mut) {
                                    Some(list) => list.push(reply),
                                    None => {
                                        c.extra
                                            .insert(COMMENT_REPLIES_KEY.into(), Value::Array(vec![reply]));
                                    }
                                }
                            }
                        });
                            cx.notify();
                        })),
                )
        };
        let mut thread = vec![div().class("pnote").child(if replies.is_empty() {
            "No replies yet.".to_owned()
        } else {
            format!(
                "{} repl{}",
                replies.len(),
                if replies.len() == 1 { "y" } else { "ies" }
            )
        })];
        thread.extend(replies);
        thread.push(reply_row);
        thread
    }

    fn comment_panels(&mut self, id: &ItemId, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(c) = self.session.project.comments.get(id) else {
            return Vec::new();
        };
        let number = c.num(azam_doc::COMMENT_NUMBER_KEY, 0.0);
        let text = c.text(azam_doc::COMMENT_TEXT_KEY).unwrap_or("").to_owned();
        let resolved = c.flag(azam_doc::COMMENT_RESOLVED_KEY);
        let piece =
            c.p.as_ref()
                .map_or_else(|| "—".to_owned(), |p| self.piece_name(p));
        let text_row = {
            let id = id.clone();
            Self::text_row("Text", text, cx, move |p, t| {
                if let Some(c) = p.comments.get_mut(&id) {
                    c.set(azam_doc::COMMENT_TEXT_KEY, t);
                }
            })
        };
        let resolved_row = {
            let id = id.clone();
            Self::check_row("Resolved", resolved, cx, move |p, on| {
                if let Some(c) = p.comments.get_mut(&id) {
                    c.set(azam_doc::COMMENT_RESOLVED_KEY, on);
                }
            })
        };
        let delete = Self::op_buttons(&[("Delete", Operator::Delete)], cx);
        let thread = self.comment_thread(id, cx);
        vec![
            self.panel(
                "Comment",
                vec![
                    info("Comment".into(), format!("#{number:.0} on {piece}")),
                    text_row,
                    resolved_row,
                    delete,
                ],
            ),
            self.panel("Thread", thread),
        ]
    }

    fn internal_panels(&mut self, piece: &PieceId, index: usize, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(il) = self
            .session
            .project
            .pieces
            .get(piece)
            .and_then(|pc| pc.internals.get(index))
        else {
            return Vec::new();
        };
        let fold = azam_doc::internal_fold(il);
        let closed = il.extra.get(azam_doc::LINE_CLOSED_KEY).and_then(Value::as_bool) == Some(true);
        let points = il.pts.len();
        let fold_row = {
            let piece = piece.clone();
            Self::check_row("Fold Line", fold.is_some(), cx, move |p, on| {
                azam_doc::set_internal_fold(p, &piece, index, on.then_some(DEFAULT_FOLD_DEG));
            })
        };
        let mut rows = vec![info("Piece".into(), self.piece_name(piece)), fold_row];
        if let Some(angle) = fold {
            let piece = piece.clone();
            rows.push(Self::number(
                slider("Fold Angle (°)", 0.0, 360.0),
                angle,
                cx,
                move |p, v| {
                    azam_doc::set_internal_fold(p, &piece, index, Some(v));
                },
            ));
        }
        let closed_row = {
            let piece = piece.clone();
            Self::check_row("Closed", closed, cx, move |p, on| {
                azam_doc::set_internal_closed(p, &piece, index, on);
            })
        };
        rows.push(closed_row);
        rows.push(info("Points".into(), points.to_string()));
        rows.push(Self::op_buttons(&[("Delete", Operator::Delete)], cx));
        vec![self.panel("Internal Line", rows)]
    }

    fn graphic_panels(&mut self, piece: &PieceId, index: usize, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(g) = self
            .session
            .project
            .pieces
            .get(piece)
            .and_then(|pc| pc.graphics.get(index))
        else {
            return Vec::new();
        };
        let size = azam_doc::graphic_size(g);
        let rotation = g.num(azam_doc::GRAPHIC_ROTATION_KEY, 0.0);
        let opacity = g.num(azam_doc::GRAPHIC_OPACITY_KEY, 1.0);
        let path = g.text(azam_doc::GRAPHIC_PATH_KEY).unwrap_or("").to_owned();
        let num = |spec, value, key: &'static str, cx: &mut Context<Self>| {
            let piece = piece.clone();
            Self::number(spec, value, cx, move |p, v| {
                if let Some(g) = p.pieces.get_mut(&piece).and_then(|pc| pc.graphics.get_mut(index)) {
                    g.set(key, v);
                }
            })
        };
        let path_row = {
            let piece = piece.clone();
            Self::text_row("Image File", path, cx, move |p, t| {
                if let Some(g) = p.pieces.get_mut(&piece).and_then(|pc| pc.graphics.get_mut(index)) {
                    g.set(azam_doc::GRAPHIC_PATH_KEY, t.trim());
                }
            })
        };
        let browse = {
            let piece = piece.clone();
            div()
                .class("pbuttons")
                .child(div().class("pbtn").child("Browse…").on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        let Some(path) = pick_image() else { return };
                        let aspect = blenderui::Image::open(&path.to_string_lossy())
                            .filter(|img| img.width > 0)
                            .map(|img| f64::from(img.height) / f64::from(img.width));
                        let piece = piece.clone();
                        this.session.edit("Graphic Image", move |p| {
                            if let Some(g) =
                                p.pieces.get_mut(&piece).and_then(|pc| pc.graphics.get_mut(index))
                            {
                                g.set(azam_doc::GRAPHIC_PATH_KEY, path.to_string_lossy().into_owned());
                                if let Some(aspect) = aspect {
                                    let width = azam_doc::graphic_size(g).x;
                                    g.set(azam_doc::GRAPHIC_HEIGHT_KEY, width * aspect);
                                }
                            }
                        });
                        cx.notify();
                    },
                )))
        };
        let rows = vec![
            path_row,
            browse,
            num(
                field("Width (mm)", 5.0, 2000.0, 1.0),
                size.x,
                azam_doc::GRAPHIC_WIDTH_KEY,
                cx,
            ),
            num(
                field("Height (mm)", 5.0, 2000.0, 1.0),
                size.y,
                azam_doc::GRAPHIC_HEIGHT_KEY,
                cx,
            ),
            num(
                slider("Rotation (°)", -180.0, 180.0),
                rotation,
                azam_doc::GRAPHIC_ROTATION_KEY,
                cx,
            ),
            num(
                field("Opacity", 0.0, 1.0, 0.05),
                opacity,
                azam_doc::GRAPHIC_OPACITY_KEY,
                cx,
            ),
            div()
                .class("pnote")
                .child("Graphics without an image file show as a frame."),
            Self::op_buttons(&[("Delete", Operator::Delete)], cx),
        ];
        vec![self.panel("Graphic", rows)]
    }
}

/// Key of a comment's reply thread: a list of `{by, at, text}`.
const COMMENT_REPLIES_KEY: &str = "replies";

/// The name replies are signed with: the operating system's user name.
fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "Me".to_owned())
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// `ms` since the Unix epoch as `YYYY-MM-DD HH:MM` (UTC).
fn format_time(ms: f64) -> String {
    let secs = (ms / 1000.0).max(0.0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

/// Asks for an image with Blender's File Browser.
fn pick_image() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match blenderui::file_dialog(
        "Choose Image",
        blenderui::FileDialogMode::Open,
        dir.as_deref(),
        None,
        &["Images\t*.png *.jpg *.jpeg *.webp *.gif *.bmp *.tga *.svg"],
        false,
    ) {
        blenderui::FileDialogResult::Chosen(paths) => paths.into_iter().next(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::format_time;

    #[test]
    fn times_format_as_utc_dates() {
        assert_eq!(format_time(0.0), "1970-01-01 00:00");
        assert_eq!(format_time(1_794_000_000_000.0), "2026-11-06 21:20");
    }
}
