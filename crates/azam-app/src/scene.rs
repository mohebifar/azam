//! Builds what the editors draw from the project.

use azam_doc::{Extras, ItemKind, Piece, PieceId, PieceState, Project, StitchStyle};
use azam_geom::{DEFAULT_SIDE_STEP_MM, OutlinePoint, Vec2, cut_outline, flatten_open_line};

use crate::pattern::{
    ButtonDraw, CommentDraw, GraphicDraw, HoleDraw, InternalDraw, NotchDraw, PatternScene, PieceDraw,
    SeamDraw, StitchDraw,
};
use crate::selection::{Part, Selection};

/// Fallback fabric colour.
const DEFAULT_FABRIC: u32 = 0x8f9bb3;

/// `#rrggbb` (or `#rgb`) to `0xRRGGBB`.
#[must_use]
pub fn parse_hex(color: &str) -> Option<u32> {
    let hex = color.trim().strip_prefix('#')?;
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok(),
        3 => {
            let v = u32::from_str_radix(hex, 16).ok()?;
            let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
            Some((r * 17) << 16 | (g * 17) << 8 | (b * 17))
        }
        _ => None,
    }
}

/// The colour a piece shows in: its fabric's colour.
#[must_use]
pub fn piece_color(project: &Project, piece: &PieceId) -> u32 {
    project
        .pieces
        .get(piece)
        .and_then(|p| project.effective_fabric(&p.fabric))
        .and_then(|f| parse_hex(&f.color))
        .unwrap_or(DEFAULT_FABRIC)
}

/// The pattern editor's scene for `project` with `selection`.
#[must_use]
pub fn pattern_scene(project: &Project, selection: &Selection, nested_sizes: bool) -> PatternScene {
    pattern_scene_with(project, selection, nested_sizes, &[])
}

/// [`pattern_scene`], with the pieces collaborators have selected outlined in their colours
/// (`(piece, 0xRRGGBB)` pairs).
#[must_use]
pub fn pattern_scene_with(
    project: &Project,
    selection: &Selection,
    nested_sizes: bool,
    collaborators: &[(PieceId, u32)],
) -> PatternScene {
    let mut pieces: Vec<_> = project.pieces.values().filter(|p| p.pts.len() >= 3).collect();
    pieces.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    let mut scene = PatternScene::default();
    for piece in &pieces {
        let mut draw = piece_draw(project, piece, selection, nested_sizes);
        draw.collaborators = collaborators
            .iter()
            .filter(|(id, _)| *id == piece.id)
            .map(|&(_, color)| color)
            .collect();
        scene.pieces.push(draw);
    }
    let numbers = azam_doc::seam_numbers(project);
    let lengths = azam_doc::sew_lengths(project);
    for seam in project.sewing.values() {
        let side_lines = |sides: &[azam_doc::SeamSide]| -> Vec<Vec<Vec2>> {
            sides
                .iter()
                .filter_map(|s| {
                    let piece = project.pieces.get(&s.piece)?;
                    let outline = piece.graded_outline(&project.active_size, &project.base_size);
                    Some(
                        s.side
                            .polyline(&outline, DEFAULT_SIDE_STEP_MM)
                            .iter()
                            .map(|p| p.pos)
                            .collect(),
                    )
                })
                .collect()
        };
        scene.seams.push(SeamDraw {
            a: side_lines(&seam.a),
            b: side_lines(&seam.b),
            color: parse_hex(&seam.color).unwrap_or(0xffcc33),
            number: numbers.get(&seam.id).copied().unwrap_or(0),
            selected: selection.seam.as_ref() == Some(&seam.id),
            diff_mm: lengths.get(&seam.id).map_or(0.0, |l| l.a - l.b),
        });
    }
    scene.stitches = stitch_draws(project, selection);
    add_items(&mut scene, project, selection);
    scene
}

/// One piece as drawn, with what is placed on it.
fn piece_draw(project: &Project, piece: &Piece, selection: &Selection, nested_sizes: bool) -> PieceDraw {
    let (size, base) = (&project.active_size, &project.base_size);
    let outline = piece.graded_outline(size, base);
    let graded_pts: Vec<(Vec2, bool)> = match &piece.grade {
        Some(rules) => azam_geom::graded_points(&piece.pts, rules, size, base)
            .iter()
            .map(|p| (p.pos, p.curve))
            .collect(),
        None => piece.pts.iter().map(|p| (p.pos, p.curve)).collect(),
    };
    let sizes = if nested_sizes && piece.grade.is_some() {
        project
            .sizes
            .iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    piece.graded_outline(s, base).poly,
                    size_color(i, project.sizes.len()),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let allowances = piece.allowances();
    let cut = cut_outline(&outline, |id| allowances.finish(id));
    let controls = outline
        .segments
        .iter()
        .filter(|s| s.ctrl.len() > 2)
        .map(|s| s.ctrl.clone())
        .collect();
    let selected_segment = selection
        .segment
        .as_ref()
        .filter(|(p, _)| *p == piece.id)
        .and_then(|(_, s)| outline.segment_index(s))
        .map(|k| outline.segments[k].poly.clone());
    let (internals, notches, graphics) = placed_parts(piece, &outline, selection);
    PieceDraw {
        name: piece.name.clone(),
        sewing: outline.poly.clone(),
        cut,
        internals,
        notches,
        graphics,
        controls,
        selected_segment,
        points: graded_pts,
        sizes,
        color: piece_color(project, &piece.id),
        selected: selection.pieces.contains(&piece.id),
        active: selection.active.as_ref() == Some(&piece.id),
        selected_points: if selection.active.as_ref() == Some(&piece.id) {
            selection.points.clone()
        } else {
            Vec::new()
        },
        grain_deg: piece.grain,
        disabled: piece.state == PieceState::Off,
        collaborators: Vec::new(),
    }
}

/// The internal lines, notches and graphics of `piece` on its shown `outline`.
fn placed_parts(
    piece: &Piece,
    outline: &azam_geom::Outline,
    selection: &Selection,
) -> (Vec<InternalDraw>, Vec<NotchDraw>, Vec<GraphicDraw>) {
    let selected_part = |part: &Part| selection.part.as_ref() == Some(part);
    let internals = piece
        .internals
        .iter()
        .enumerate()
        .map(|(index, il)| InternalDraw {
            line: internal_polyline(il),
            fold: azam_doc::internal_fold(il).is_some(),
            selected: selected_part(&Part::Internal {
                piece: piece.id.clone(),
                index,
            }),
        })
        .collect();
    let inward = inward_sign(&outline.poly);
    let notches = piece
        .notches
        .iter()
        .enumerate()
        .filter_map(|(index, n)| {
            let seg = &outline.segments[outline.segment_index(&n.s)?];
            let at = seg.point_at(n.t.unwrap_or(0.0).clamp(0.0, 1.0));
            Some(NotchDraw {
                at: at.pos,
                outward: -left_normal(at.tangent) * inward,
                selected: selected_part(&Part::Notch {
                    piece: piece.id.clone(),
                    index,
                }),
            })
        })
        .collect();
    let graphics = piece
        .graphics
        .iter()
        .enumerate()
        .map(|(index, g)| GraphicDraw {
            centre: Vec2::new(g.x, g.y),
            size: azam_doc::graphic_size(g),
            rotation: g.num(azam_doc::GRAPHIC_ROTATION_KEY, 0.0),
            image: g
                .text(azam_doc::GRAPHIC_PATH_KEY)
                .filter(|p| !p.is_empty() && std::path::Path::new(p).is_file())
                .map(str::to_owned),
            selected: selected_part(&Part::Graphic {
                piece: piece.id.clone(),
                index,
            }),
        })
        .collect();
    (internals, notches, graphics)
}

/// An internal line flattened, back to its start when it is closed.
#[must_use]
pub fn internal_polyline(il: &azam_doc::InternalLine) -> Vec<Vec2> {
    let mut pts: Vec<OutlinePoint> = il
        .pts
        .iter()
        .map(|p| OutlinePoint {
            id: p.id.clone().unwrap_or_else(|| "".into()),
            pos: p.pos,
            curve: p.curve,
        })
        .collect();
    if il
        .extra
        .get(azam_doc::LINE_CLOSED_KEY)
        .and_then(serde_json::Value::as_bool)
        == Some(true)
        && let Some(first) = pts.first().cloned()
    {
        pts.push(OutlinePoint {
            curve: false,
            ..first
        });
    }
    flatten_open_line(&pts)
}

/// `1` when the left normal of the outline's direction points into the piece, else `-1`.
#[must_use]
pub fn inward_sign(poly: &[Vec2]) -> f64 {
    if azam_geom::signed_area(poly) > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// The normal to the left of `tangent` (on the y-down page).
#[must_use]
pub fn left_normal(tangent: Vec2) -> Vec2 {
    Vec2::new(-tangent.y, tangent.x)
}

/// The polyline `poly` moved `offset` mm along its left normal.
#[must_use]
pub fn offset_polyline(poly: &[Vec2], offset: f64) -> Vec<Vec2> {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let a = poly[i.saturating_sub(1)];
            let b = poly[(i + 1).min(n - 1)];
            let t = (b - a).normalize_or_zero();
            poly[i] + left_normal(t) * offset
        })
        .collect()
}

/// Topstitches, each row offset into its piece.
fn stitch_draws(project: &Project, selection: &Selection) -> Vec<StitchDraw> {
    project
        .stitches
        .values()
        .filter_map(|st| {
            let piece = project.pieces.get(st.p.as_ref()?)?;
            let outline = piece.graded_outline(&project.active_size, &project.base_size);
            let style = StitchStyle::of(st);
            let inward = inward_sign(&outline.poly);
            let segments: Vec<&azam_geom::Segment> = if st.seg.0 == azam_doc::WHOLE_OUTLINE {
                outline.segments.iter().collect()
            } else {
                vec![&outline.segments[outline.segment_index(&st.seg)?]]
            };
            let rows = (0..style.rows)
                .flat_map(|r| {
                    let off = style.row_offset(r) * inward;
                    segments.iter().map(move |s| offset_polyline(&s.poly, off))
                })
                .collect();
            Some(StitchDraw {
                rows,
                length: style.length,
                gap: style.gap,
                width: style.width,
                color: parse_hex(&style.color).unwrap_or(0x333333),
                selected: selection.part.as_ref() == Some(&Part::Stitch(st.id.clone())),
            })
        })
        .collect()
}

/// Buttons, buttonholes and comments.
fn add_items(scene: &mut PatternScene, project: &Project, selection: &Selection) {
    let at = |it: &azam_doc::Item| Some(Vec2::new(it.x?, it.y?));
    let selected =
        |kind, id: &azam_doc::ItemId| selection.part.as_ref() == Some(&Part::Item(kind, id.clone()));
    for b in project.buttons.values() {
        let Some(pos) = at(b) else { continue };
        scene.buttons.push(ButtonDraw {
            at: pos,
            diameter: b.num(azam_doc::BUTTON_SIZE_KEY, azam_doc::DEFAULT_BUTTON_SIZE_MM),
            color: b
                .text(azam_doc::COLOR_KEY)
                .and_then(parse_hex)
                .unwrap_or(0x333333),
            selected: selected(ItemKind::Button, &b.id),
            fastened_to: b
                .text(azam_doc::BUTTON_FASTEN_KEY)
                .and_then(|h| project.holes.get(&azam_doc::ItemId::new(h)))
                .and_then(at),
        });
    }
    for h in project.holes.values() {
        let Some(pos) = at(h) else { continue };
        scene.holes.push(HoleDraw {
            at: pos,
            length: h.num(azam_doc::HOLE_LENGTH_KEY, azam_doc::DEFAULT_HOLE_LENGTH_MM),
            angle: h.num(azam_doc::HOLE_ANGLE_KEY, 0.0),
            selected: selected(ItemKind::Hole, &h.id),
        });
    }
    for c in project.comments.values() {
        let Some(pos) = at(c) else { continue };
        let n = c.num(azam_doc::COMMENT_NUMBER_KEY, 0.0);
        scene.comments.push(CommentDraw {
            at: pos,
            number: if n > 0.0 { format!("{n:.0}") } else { "•".into() },
            resolved: c.flag(azam_doc::COMMENT_RESOLVED_KEY),
            selected: selected(ItemKind::Comment, &c.id),
        });
    }
}

/// Colour of size `i` of `n` in the nested grading view: blue for the smallest through red for
/// the largest.
fn size_color(i: usize, n: usize) -> u32 {
    const RAMP: [u32; 5] = [0x4f8ff7, 0x3fc8c8, 0x7ed957, 0xf2c14e, 0xef6a4c];
    let t = if n <= 1 { 0.0 } else { i as f64 / (n - 1) as f64 };
    RAMP[((t * (RAMP.len() - 1) as f64).round() as usize).min(RAMP.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_parse() {
        assert_eq!(parse_hex("#4772b3"), Some(0x4772b3));
        assert_eq!(parse_hex("#fff"), Some(0xffffff));
        assert_eq!(parse_hex("blue"), None);
    }
}
