//! The Segment panel's length links: which segments the selected one is kept equal to (with its
//! ease), and the dropdown that links it to another.

use azam_doc::{PieceId, PointId, Project, SegmentRef};
use blenderui::prelude::*;

use super::field;
use crate::app::AzamApp;

/// Segments offered in the "Match Length Of" dropdown, closest in length first.
const MAX_CANDIDATES: usize = 24;

/// `r` as the panel names it: its piece and its number along the outline, and its length.
fn segment_label(project: &Project, r: &SegmentRef) -> String {
    let Some(pc) = project.pieces.get(&r.piece) else {
        return "—".into();
    };
    let ol = pc.outline();
    let Some(k) = ol.segment_index(&r.seg) else {
        return pc.name.clone();
    };
    format!(
        "{} · {} ({})",
        pc.name,
        k + 1,
        crate::units::text(project.units, ol.segments[k].len)
    )
}

impl AzamApp {
    /// Rows for the length links of segment `seg` of `piece`.
    pub(super) fn length_link_rows(
        &mut self,
        piece: &PieceId,
        seg: &PointId,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let project = &self.session.project;
        let me = SegmentRef::new(piece, seg);
        let mut rows = Vec::new();
        let linked: Vec<(usize, SegmentRef, f64)> = project
            .links
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                if l.a == me {
                    Some((i, l.b.clone(), l.ease))
                } else if l.b == me {
                    Some((i, l.a.clone(), -l.ease))
                } else {
                    None
                }
            })
            .collect();
        for (index, other, ease) in linked {
            rows.push(
                div()
                    .class("prow")
                    .id(format!("link-{index}"))
                    .child(div().class("plabel").child("Linked To"))
                    .child(div().class("pvalue").child(segment_label(project, &other))),
            );
            // the ease as this segment sees it: how much longer the other one is
            let ease_row = Self::number(
                field("Other Is Longer By (mm)", -200.0, 200.0, 0.5),
                ease,
                cx,
                move |p, v| {
                    if let Some(l) = p.links.get_mut(index) {
                        // stored as b − a
                        l.ease = if l.a.seg == other.seg && l.a.piece == other.piece {
                            -v
                        } else {
                            v
                        };
                    }
                },
            );
            rows.push(ease_row);
            rows.push(
                div().class("pbuttons").child(
                    div()
                        .class("pbtn")
                        .id(format!("unlink-{index}"))
                        .child("Unlink")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.session.edit("Unlink Lengths", |p| {
                                azam_doc::remove_length_link(p, index);
                            });
                            this.status = "Segments unlinked".into();
                            cx.notify();
                        })),
                ),
            );
        }
        let candidates = link_candidates(project, &me);
        if !candidates.is_empty() {
            let labels: Vec<String> = std::iter::once("Choose a segment…".to_owned())
                .chain(candidates.iter().map(|r| segment_label(project, r)))
                .collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            rows.push(Self::choice("Match Length Of", &refs, 0, cx, move |p, i| {
                if let Some(other) = i.checked_sub(1).and_then(|i| candidates.get(i)) {
                    // the chosen segment leads; this one takes its length
                    azam_doc::add_length_link(p, other, &me, 0.0);
                }
            }));
        }
        rows
    }
}

/// Segments `me` could be linked to: every other segment not linked to it yet, closest in length
/// first.
fn link_candidates(project: &Project, me: &SegmentRef) -> Vec<SegmentRef> {
    let Some(mine) = azam_doc::segment_length(project, me) else {
        return Vec::new();
    };
    let mut all: Vec<(f64, SegmentRef)> = project
        .pieces
        .values()
        .flat_map(|pc| {
            let ol = pc.outline();
            ol.segments
                .iter()
                .map(|sg| (sg.len, SegmentRef::new(&pc.id, &sg.id)))
                .collect::<Vec<_>>()
        })
        .filter(|(_, r)| r != me)
        .filter(|(_, r)| {
            !project
                .links
                .iter()
                .any(|l| (l.a == *me && l.b == *r) || (l.b == *me && l.a == *r))
        })
        .collect();
    all.sort_by(|a, b| (a.0 - mine).abs().total_cmp(&(b.0 - mine).abs()));
    all.into_iter().take(MAX_CANDIDATES).map(|(_, r)| r).collect()
}
