//! Parts placed on pieces: notches, topstitches, buttons, buttonholes, comments and graphics.
//!
//! Their style fields (stitch length, button size, comment text…) live in the stored `extra` maps
//! under the stored field names; [`Extras`] reads and writes them, and the `*_KEY` constants name
//! them. The `DEFAULT_*` constants are what a newly placed part gets; reading a field that is
//! missing falls back to the `FALLBACK_*` value, which is how older files without the field draw.

use azam_geom::{PointId, Vec2};
use serde_json::{Map, Value};

use crate::ids::{ItemId, PartId, PieceId, StitchId, prefix};
use crate::piece::{Graphic, InternalLine, Notch};
use crate::project::{Item, Project, Seam, Stitch, WHOLE_OUTLINE};

/// Stored field: topstitch distance from the edge, mm.
pub const STITCH_OFFSET_KEY: &str = "off";
/// Stored field: length of one stitch, mm.
pub const STITCH_LENGTH_KEY: &str = "len";
/// Stored field: gap between stitches, mm.
pub const STITCH_GAP_KEY: &str = "gap";
/// Stored field: thread width, mm.
pub const STITCH_WIDTH_KEY: &str = "w";
/// Stored field: number of parallel rows.
pub const STITCH_ROWS_KEY: &str = "rows";
/// Stored field: spacing between rows, mm.
pub const STITCH_ROW_GAP_KEY: &str = "rowGap";
/// Stored field: thread, button, buttonhole or seam colour, `#rrggbb`.
pub const COLOR_KEY: &str = "color";

/// Topstitch distance from the edge of a new topstitch, mm.
pub const DEFAULT_STITCH_OFFSET_MM: f64 = 4.0;
/// Stitch length of a new topstitch, mm.
pub const DEFAULT_STITCH_LENGTH_MM: f64 = 3.0;
/// Gap between stitches of a new topstitch, mm.
pub const DEFAULT_STITCH_GAP_MM: f64 = 1.6;
/// Thread width of a new topstitch, mm.
pub const DEFAULT_STITCH_WIDTH_MM: f64 = 0.9;
/// Thread colour of a new topstitch.
pub const DEFAULT_STITCH_COLOR: &str = "#3b3a36";
/// Rows of a new topstitch.
pub const DEFAULT_STITCH_ROWS: u32 = 1;
/// Row spacing of a new topstitch, mm.
pub const DEFAULT_STITCH_ROW_GAP_MM: f64 = 6.0;
/// Distance from the edge of a topstitch that does not store one, mm.
pub const FALLBACK_STITCH_OFFSET_MM: f64 = 3.0;
/// Gap of a topstitch that does not store one, mm.
pub const FALLBACK_STITCH_GAP_MM: f64 = 1.5;
/// Thread width of a topstitch that does not store one, mm.
pub const FALLBACK_STITCH_WIDTH_MM: f64 = 0.8;
/// Thread colour of a topstitch that does not store one.
pub const FALLBACK_STITCH_COLOR: &str = "#333333";

/// Stored field: button diameter, mm.
pub const BUTTON_SIZE_KEY: &str = "size";
/// Stored field: button shape (`round`, `square`, `shank`).
pub const BUTTON_SHAPE_KEY: &str = "shape";
/// Stored field: number of button holes (0, 2 or 4).
pub const BUTTON_HOLES_KEY: &str = "holes";
/// Stored field: button finish (`plastic`, `metal`).
pub const BUTTON_FINISH_KEY: &str = "finish";
/// Stored field: the buttonhole a button is fastened to.
pub const BUTTON_FASTEN_KEY: &str = "fasten";
/// Diameter of a new button (and of one that stores none), mm.
pub const DEFAULT_BUTTON_SIZE_MM: f64 = 15.0;
/// Colour of a new button.
pub const DEFAULT_BUTTON_COLOR: &str = "#2b2b2b";
/// Shape of a new button.
pub const DEFAULT_BUTTON_SHAPE: &str = "round";
/// Holes of a new button.
pub const DEFAULT_BUTTON_HOLES: u32 = 4;
/// Finish of a new button.
pub const DEFAULT_BUTTON_FINISH: &str = "plastic";

/// Stored field: buttonhole length, mm.
pub const HOLE_LENGTH_KEY: &str = "len";
/// Stored field: buttonhole angle, degrees.
pub const HOLE_ANGLE_KEY: &str = "ang";
/// Length of a new buttonhole (and of one that stores none), mm.
pub const DEFAULT_HOLE_LENGTH_MM: f64 = 18.0;
/// Thread colour of a new buttonhole.
pub const DEFAULT_HOLE_COLOR: &str = "#2a2a2a";

/// Stored field: comment number shown on its pin.
pub const COMMENT_NUMBER_KEY: &str = "n";
/// Stored field: comment text.
pub const COMMENT_TEXT_KEY: &str = "text";
/// Stored field: whether a comment is resolved.
pub const COMMENT_RESOLVED_KEY: &str = "resolved";
/// Stored field: when a comment was written, ms since the Unix epoch.
pub const COMMENT_AT_KEY: &str = "at";

/// Stored field: graphic width, mm.
pub const GRAPHIC_WIDTH_KEY: &str = "w";
/// Stored field: graphic height, mm.
pub const GRAPHIC_HEIGHT_KEY: &str = "h";
/// Stored field: graphic rotation, degrees.
pub const GRAPHIC_ROTATION_KEY: &str = "rot";
/// Stored field: graphic opacity, 0–1.
pub const GRAPHIC_OPACITY_KEY: &str = "opacity";
/// Stored field: image file of a graphic.
pub const GRAPHIC_PATH_KEY: &str = "path";
/// Stored field: display name of a graphic.
pub const GRAPHIC_NAME_KEY: &str = "name";
/// Width of a newly placed graphic, mm.
pub const DEFAULT_GRAPHIC_WIDTH_MM: f64 = 120.0;

/// Stored field: whether an internal line closes on itself.
pub const LINE_CLOSED_KEY: &str = "closed";
/// Stored field: fold angle of an internal line, degrees (absent or null: not a fold).
pub const LINE_FOLD_KEY: &str = "fold";

/// Typed access to the stored fields a part keeps in its `extra` map.
pub trait Extras {
    /// The fields.
    fn extra(&self) -> &Map<String, Value>;
    /// The fields, to change.
    fn extra_mut(&mut self) -> &mut Map<String, Value>;

    /// Number field `key`, or `fallback` when it is missing or not a number.
    fn num(&self, key: &str, fallback: f64) -> f64 {
        self.extra().get(key).and_then(Value::as_f64).unwrap_or(fallback)
    }

    /// Text field `key`, if it is a string.
    fn text(&self, key: &str) -> Option<&str> {
        self.extra().get(key).and_then(Value::as_str)
    }

    /// Whether flag `key` is set.
    fn flag(&self, key: &str) -> bool {
        self.extra().get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    /// Sets field `key`.
    fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.extra_mut().insert(key.to_owned(), value.into());
    }

    /// Removes field `key`.
    fn unset(&mut self, key: &str) {
        self.extra_mut().remove(key);
    }
}

macro_rules! impl_extras {
    ($($t:ty),*) => {$(
        impl Extras for $t {
            fn extra(&self) -> &Map<String, Value> {
                &self.extra
            }
            fn extra_mut(&mut self) -> &mut Map<String, Value> {
                &mut self.extra
            }
        }
    )*};
}

impl_extras!(Stitch, Item, Graphic, InternalLine, Notch, Seam);

/// How a topstitch is drawn, read with the fallbacks of a stitch that stores nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct StitchStyle {
    /// Distance of the first row from the edge, mm.
    pub offset: f64,
    /// Length of one stitch, mm.
    pub length: f64,
    /// Gap between stitches, mm.
    pub gap: f64,
    /// Thread width, mm.
    pub width: f64,
    /// Number of rows (at least 1).
    pub rows: u32,
    /// Spacing between rows, mm.
    pub row_gap: f64,
    /// Thread colour, `#rrggbb`.
    pub color: String,
}

impl StitchStyle {
    /// The style of `stitch`.
    #[must_use]
    pub fn of(stitch: &Stitch) -> Self {
        Self {
            offset: stitch.num(STITCH_OFFSET_KEY, FALLBACK_STITCH_OFFSET_MM),
            length: stitch.num(STITCH_LENGTH_KEY, DEFAULT_STITCH_LENGTH_MM),
            gap: stitch.num(STITCH_GAP_KEY, FALLBACK_STITCH_GAP_MM),
            width: stitch.num(STITCH_WIDTH_KEY, FALLBACK_STITCH_WIDTH_MM),
            rows: stitch.num(STITCH_ROWS_KEY, 1.0).round().max(1.0) as u32,
            row_gap: stitch.num(STITCH_ROW_GAP_KEY, DEFAULT_STITCH_ROW_GAP_MM),
            color: stitch.text(COLOR_KEY).unwrap_or(FALLBACK_STITCH_COLOR).to_owned(),
        }
    }

    /// Distance of row `row` (0-based) from the edge, mm.
    #[must_use]
    pub fn row_offset(&self, row: u32) -> f64 {
        self.offset + f64::from(row) * self.row_gap
    }
}

/// The three kinds of [`Item`] a project keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// [`Project::buttons`].
    Button,
    /// [`Project::holes`].
    Hole,
    /// [`Project::comments`].
    Comment,
}

impl ItemKind {
    /// Id prefix of new items of this kind.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Button => prefix::BUTTON,
            Self::Hole => prefix::HOLE,
            Self::Comment => prefix::COMMENT,
        }
    }
}

impl Project {
    /// The items of `kind`.
    #[must_use]
    pub fn items(&self, kind: ItemKind) -> &indexmap::IndexMap<ItemId, Item> {
        match kind {
            ItemKind::Button => &self.buttons,
            ItemKind::Hole => &self.holes,
            ItemKind::Comment => &self.comments,
        }
    }

    /// The items of `kind`, to change.
    pub fn items_mut(&mut self, kind: ItemKind) -> &mut indexmap::IndexMap<ItemId, Item> {
        match kind {
            ItemKind::Button => &mut self.buttons,
            ItemKind::Hole => &mut self.holes,
            ItemKind::Comment => &mut self.comments,
        }
    }
}

/// Adds a notch at fraction `t` along segment `segment` of `piece`. Returns its id, or `None` if
/// there is no such piece or segment.
pub fn add_notch(project: &mut Project, piece: &PieceId, segment: &PointId, t: f64) -> Option<PartId> {
    let pc = project.pieces.get(piece)?;
    pc.outline().segment_index(segment)?;
    let id = PartId::new(project.fresh_id(prefix::NOTCH));
    project.pieces.get_mut(piece)?.notches.push(Notch {
        id: Some(id.clone()),
        s: segment.clone(),
        t: Some(t.clamp(0.0, 1.0)),
        extra: Map::new(),
    });
    Some(id)
}

/// Adds a topstitch along segment `segment` of `piece`, or around its whole outline when `segment`
/// is `None`, with the default style. Returns its id, or `None` if there is no such piece or
/// segment.
pub fn add_stitch(project: &mut Project, piece: &PieceId, segment: Option<&PointId>) -> Option<StitchId> {
    let pc = project.pieces.get(piece)?;
    let seg = match segment {
        Some(s) => {
            pc.outline().segment_index(s)?;
            s.clone()
        }
        None => PointId::new(WHOLE_OUTLINE),
    };
    let id = StitchId::new(project.fresh_id(prefix::STITCH));
    let mut stitch = Stitch {
        id: id.clone(),
        p: Some(piece.clone()),
        seg,
        extra: Map::new(),
    };
    stitch.set(STITCH_OFFSET_KEY, DEFAULT_STITCH_OFFSET_MM);
    stitch.set(STITCH_LENGTH_KEY, DEFAULT_STITCH_LENGTH_MM);
    stitch.set(STITCH_GAP_KEY, DEFAULT_STITCH_GAP_MM);
    stitch.set(STITCH_WIDTH_KEY, DEFAULT_STITCH_WIDTH_MM);
    stitch.set(COLOR_KEY, DEFAULT_STITCH_COLOR);
    stitch.set(STITCH_ROWS_KEY, DEFAULT_STITCH_ROWS);
    stitch.set(STITCH_ROW_GAP_KEY, DEFAULT_STITCH_ROW_GAP_MM);
    project.stitches.insert(id.clone(), stitch);
    Some(id)
}

/// A new item of `kind` on `piece` at `pos`, with a fresh id and no fields.
fn new_item(project: &mut Project, kind: ItemKind, piece: &PieceId, pos: Vec2) -> Option<Item> {
    if !project.pieces.contains_key(piece) {
        return None;
    }
    Some(Item {
        id: ItemId::new(project.fresh_id(kind.prefix())),
        p: Some(piece.clone()),
        x: Some(pos.x),
        y: Some(pos.y),
        extra: Map::new(),
    })
}

/// Adds a button with the default style on `piece` at `pos`. Returns its id, or `None` if there is
/// no such piece.
pub fn add_button(project: &mut Project, piece: &PieceId, pos: Vec2) -> Option<ItemId> {
    let mut item = new_item(project, ItemKind::Button, piece, pos)?;
    item.set(BUTTON_SIZE_KEY, DEFAULT_BUTTON_SIZE_MM);
    item.set(COLOR_KEY, DEFAULT_BUTTON_COLOR);
    item.set(BUTTON_SHAPE_KEY, DEFAULT_BUTTON_SHAPE);
    item.set(BUTTON_HOLES_KEY, DEFAULT_BUTTON_HOLES);
    item.set(BUTTON_FINISH_KEY, DEFAULT_BUTTON_FINISH);
    let id = item.id.clone();
    project.buttons.insert(id.clone(), item);
    Some(id)
}

/// Adds a horizontal buttonhole with the default style on `piece` at `pos`. Returns its id, or
/// `None` if there is no such piece.
pub fn add_hole(project: &mut Project, piece: &PieceId, pos: Vec2) -> Option<ItemId> {
    let mut item = new_item(project, ItemKind::Hole, piece, pos)?;
    item.set(HOLE_LENGTH_KEY, DEFAULT_HOLE_LENGTH_MM);
    item.set(HOLE_ANGLE_KEY, 0.0);
    item.set(COLOR_KEY, DEFAULT_HOLE_COLOR);
    let id = item.id.clone();
    project.holes.insert(id.clone(), item);
    Some(id)
}

/// Fastens button `button` to buttonhole `hole`, or unfastens it when `hole` is `None`. Returns
/// `false` if the button or the buttonhole does not exist.
pub fn fasten_button(project: &mut Project, button: &ItemId, hole: Option<&ItemId>) -> bool {
    if hole.is_some_and(|h| !project.holes.contains_key(h)) {
        return false;
    }
    let Some(b) = project.buttons.get_mut(button) else {
        return false;
    };
    match hole {
        Some(h) => b.set(BUTTON_FASTEN_KEY, h.as_str()),
        None => b.unset(BUTTON_FASTEN_KEY),
    }
    true
}

/// Pins a comment saying `text` on `piece` at `pos`, numbered after the highest comment number.
/// `now_ms` is stored as when it was written. Returns its id, or `None` if there is no such piece.
pub fn add_comment(
    project: &mut Project,
    piece: &PieceId,
    pos: Vec2,
    text: &str,
    now_ms: f64,
) -> Option<ItemId> {
    let number = project
        .comments
        .values()
        .map(|c| c.num(COMMENT_NUMBER_KEY, 0.0))
        .fold(0.0, f64::max)
        + 1.0;
    let mut item = new_item(project, ItemKind::Comment, piece, pos)?;
    item.set(COMMENT_NUMBER_KEY, number);
    item.set(COMMENT_TEXT_KEY, text);
    item.set(COMMENT_AT_KEY, now_ms);
    item.set(COMMENT_RESOLVED_KEY, false);
    let id = item.id.clone();
    project.comments.insert(id.clone(), item);
    Some(id)
}

/// Moves item `id` of `kind` by `delta`. Returns `false` if there is no such item or it has no
/// position.
pub fn move_item(project: &mut Project, kind: ItemKind, id: &ItemId, delta: Vec2) -> bool {
    match project.items_mut(kind).get_mut(id) {
        Some(Item {
            x: Some(x),
            y: Some(y),
            ..
        }) => {
            *x += delta.x;
            *y += delta.y;
            true
        }
        _ => false,
    }
}

/// Deletes item `id` of `kind`; deleting a buttonhole unfastens the buttons fastened to it.
/// Returns `false` if there was no such item.
pub fn delete_item(project: &mut Project, kind: ItemKind, id: &ItemId) -> bool {
    if project.items_mut(kind).shift_remove(id).is_none() {
        return false;
    }
    if kind == ItemKind::Hole {
        for b in project.buttons.values_mut() {
            if b.text(BUTTON_FASTEN_KEY) == Some(id.as_str()) {
                b.unset(BUTTON_FASTEN_KEY);
            }
        }
    }
    true
}

/// Places a graphic of `size` mm centred at `pos` on `piece`, showing image file `path` if given.
/// Returns its id, or `None` if there is no such piece.
pub fn add_graphic(
    project: &mut Project,
    piece: &PieceId,
    pos: Vec2,
    size: Vec2,
    path: Option<&str>,
) -> Option<PartId> {
    if !project.pieces.contains_key(piece) {
        return None;
    }
    let id = PartId::new(project.fresh_id(prefix::GRAPHIC));
    let mut g = Graphic {
        id: Some(id.clone()),
        x: pos.x,
        y: pos.y,
        extra: Map::new(),
    };
    if let Some(path) = path {
        g.set(GRAPHIC_PATH_KEY, path);
    }
    g.set(GRAPHIC_WIDTH_KEY, size.x);
    g.set(GRAPHIC_HEIGHT_KEY, size.y);
    g.set(GRAPHIC_ROTATION_KEY, 0.0);
    g.set(GRAPHIC_OPACITY_KEY, 1.0);
    project.pieces.get_mut(piece)?.graphics.push(g);
    Some(id)
}

/// Width and height of graphic `g`, mm (a graphic that stores no size is the default width,
/// square).
#[must_use]
pub fn graphic_size(g: &Graphic) -> Vec2 {
    let w = g.num(GRAPHIC_WIDTH_KEY, DEFAULT_GRAPHIC_WIDTH_MM);
    Vec2::new(w, g.num(GRAPHIC_HEIGHT_KEY, w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;

    fn project_with_square() -> (Project, PieceId) {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pts =
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = crate::create_piece(&mut p, &pts).expect("piece");
        (p, id)
    }

    fn first_segment(p: &Project, id: &PieceId) -> PointId {
        p.pieces[id].pts[0].id.clone()
    }

    #[test]
    fn notches_land_on_existing_segments_only() {
        let (mut p, id) = project_with_square();
        let seg = first_segment(&p, &id);
        let n = add_notch(&mut p, &id, &seg, 1.5).expect("notch");
        let notch = &p.pieces[&id].notches[0];
        assert_eq!(notch.id.as_ref(), Some(&n));
        assert_eq!(notch.t, Some(1.0), "t is clamped to the segment");
        assert!(add_notch(&mut p, &id, &PointId::new("nope"), 0.5).is_none());
    }

    #[test]
    fn a_new_topstitch_has_the_default_style() {
        let (mut p, id) = project_with_square();
        let seg = first_segment(&p, &id);
        let st = add_stitch(&mut p, &id, Some(&seg)).expect("stitch");
        let style = StitchStyle::of(&p.stitches[&st]);
        assert!((style.offset - DEFAULT_STITCH_OFFSET_MM).abs() < 1e-12);
        assert_eq!(style.rows, 1);
        assert_eq!(style.color, DEFAULT_STITCH_COLOR);
        let all = add_stitch(&mut p, &id, None).expect("whole outline");
        assert_eq!(p.stitches[&all].seg.0, WHOLE_OUTLINE);
    }

    #[test]
    fn a_stored_topstitch_without_fields_draws_with_fallbacks() {
        let st = Stitch {
            id: StitchId::new("st"),
            p: None,
            seg: PointId::new("a"),
            extra: Map::new(),
        };
        let style = StitchStyle::of(&st);
        assert!((style.offset - FALLBACK_STITCH_OFFSET_MM).abs() < 1e-12);
        assert!(
            (style.row_offset(2) - (FALLBACK_STITCH_OFFSET_MM + 2.0 * DEFAULT_STITCH_ROW_GAP_MM)).abs()
                < 1e-12
        );
    }

    #[test]
    fn deleting_a_buttonhole_unfastens_its_buttons() {
        let (mut p, id) = project_with_square();
        let b = add_button(&mut p, &id, Vec2::new(50.0, 20.0)).expect("button");
        let h = add_hole(&mut p, &id, Vec2::new(50.0, 80.0)).expect("hole");
        assert!(fasten_button(&mut p, &b, Some(&h)));
        assert_eq!(p.buttons[&b].text(BUTTON_FASTEN_KEY), Some(h.as_str()));
        assert!(!fasten_button(&mut p, &b, Some(&ItemId::new("missing"))));
        assert!(delete_item(&mut p, ItemKind::Hole, &h));
        assert_eq!(p.buttons[&b].text(BUTTON_FASTEN_KEY), None);
        assert!(!delete_item(&mut p, ItemKind::Hole, &h));
    }

    #[test]
    fn items_move_and_ids_use_their_kind_prefix() {
        let (mut p, id) = project_with_square();
        let b = add_button(&mut p, &id, Vec2::new(10.0, 10.0)).expect("button");
        assert!(b.as_str().starts_with("bt_"));
        assert!(move_item(&mut p, ItemKind::Button, &b, Vec2::new(5.0, -2.0)));
        assert_eq!((p.buttons[&b].x, p.buttons[&b].y), (Some(15.0), Some(8.0)));
        assert!(add_button(&mut p, &PieceId::new("none"), Vec2::ZERO).is_none());
    }

    #[test]
    fn comments_are_numbered_in_turn() {
        let (mut p, id) = project_with_square();
        let a = add_comment(&mut p, &id, Vec2::new(1.0, 1.0), "first", 5.0).expect("comment");
        let b = add_comment(&mut p, &id, Vec2::new(2.0, 2.0), "second", 6.0).expect("comment");
        assert!((p.comments[&a].num(COMMENT_NUMBER_KEY, 0.0) - 1.0).abs() < 1e-12);
        assert!((p.comments[&b].num(COMMENT_NUMBER_KEY, 0.0) - 2.0).abs() < 1e-12);
        assert_eq!(p.comments[&b].text(COMMENT_TEXT_KEY), Some("second"));
        assert!(!p.comments[&b].flag(COMMENT_RESOLVED_KEY));
    }

    #[test]
    fn graphics_follow_their_piece() {
        let (mut p, id) = project_with_square();
        let g =
            add_graphic(&mut p, &id, Vec2::new(50.0, 50.0), Vec2::new(40.0, 20.0), None).expect("graphic");
        assert_eq!(p.pieces[&id].graphics[0].id.as_ref(), Some(&g));
        assert_eq!(graphic_size(&p.pieces[&id].graphics[0]), Vec2::new(40.0, 20.0));
        crate::transform_piece(&mut p, &id, |v| v + Vec2::new(10.0, 0.0));
        assert!((p.pieces[&id].graphics[0].x - 60.0).abs() < 1e-12);
        let bare = Graphic {
            id: None,
            x: 0.0,
            y: 0.0,
            extra: Map::new(),
        };
        assert_eq!(graphic_size(&bare), Vec2::splat(DEFAULT_GRAPHIC_WIDTH_MM));
    }
}
