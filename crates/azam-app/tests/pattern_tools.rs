//! Drives the pattern editor's tools headless: drawing pieces, notches, topstitches, buttons,
//! sewing and editing seams, each followed by undo.
//!
//! Set `AZAM_SHOT_DIR` to a directory to also save a screenshot after each step.

use azam_app::AzamApp;
use azam_app::pattern::PATTERN_ID;
use azam_geom::{DEFAULT_SIDE_STEP_MM, Vec2};
use blenderui::prelude::*;

fn options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1400.0), px(860.0)),
        })),
        ..Default::default()
    }
}

/// Where the status bar is: a click there focuses the window without touching an editor.
const STATUS_BAR: (f32, f32) = (700.0, 850.0);

struct Ui {
    win: WindowHandle<AzamApp>,
}

impl Ui {
    fn open(cx: &mut App) -> Self {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        for _ in 0..3 {
            cx.draw(win.id);
        }
        cx.simulate_click(win.id, STATUS_BAR.0, STATUS_BAR.1);
        cx.draw(win.id);
        Self { win }
    }

    fn app<'a>(&self, cx: &'a App) -> &'a AzamApp {
        self.win.root().read(cx)
    }

    /// Window position of pattern point `q`.
    fn at(&self, cx: &mut App, q: Vec2) -> (f32, f32) {
        let canvas = cx
            .element_bounds(self.win.id, PATTERN_ID)
            .expect("pattern canvas");
        let s = self.app(cx).pattern_view().to_screen(q);
        (canvas.origin.x + s.x as f32, canvas.origin.y + s.y as f32)
    }

    fn click(&self, cx: &mut App, q: Vec2) {
        let (x, y) = self.at(cx, q);
        cx.simulate_click(self.win.id, x, y);
        cx.draw(self.win.id);
    }

    fn shift_click(&self, cx: &mut App, q: Vec2) {
        let (x, y) = self.at(cx, q);
        cx.simulate_click_with(self.win.id, x, y, false, true);
        cx.draw(self.win.id);
    }

    fn key(&self, cx: &mut App, text: &str, keysym: &str) {
        cx.simulate_key(self.win.id, text, keysym);
        cx.draw(self.win.id);
    }

    fn undo(&self, cx: &mut App) {
        cx.simulate_key_with(self.win.id, "z", "z", true, false);
        cx.draw(self.win.id);
    }

    /// Chooses a tool by searching for it (F3) and pressing Enter on the first match.
    fn search(&self, cx: &mut App, query: &str) {
        self.key(cx, "", "F3");
        cx.draw(self.win.id);
        for c in query.chars() {
            let s = c.to_string();
            cx.simulate_key(self.win.id, &s, &s);
        }
        cx.draw(self.win.id);
        self.key(cx, "", "Return");
    }

    /// Saves a screenshot when `AZAM_SHOT_DIR` is set.
    fn shot(&self, cx: &mut App, name: &str) {
        let Some(dir) = std::env::var_os("AZAM_SHOT_DIR") else {
            return;
        };
        // Key presses change state without asking the headless window to paint again.
        self.win.root().update(cx, |_, cx| cx.notify());
        let (w, h, pixels) = cx.draw(self.win.id);
        let rgba: Vec<u8> = pixels
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255])
            .collect();
        let path = std::path::Path::new(&dir).join(format!("{name}.png"));
        let file = std::fs::File::create(&path).expect("screenshot file");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut wr| wr.write_image_data(&rgba))
            .expect("screenshot written");
    }

    /// The piece called `name`.
    fn piece(&self, cx: &App, name: &str) -> azam_doc::PieceId {
        self.app(cx)
            .session()
            .project
            .pieces
            .values()
            .find(|p| p.name == name)
            .map(|p| p.id.clone())
            .expect("piece")
    }

    /// The middle of segment `k` of `piece`, and the segment's id.
    fn segment_middle(&self, cx: &App, piece: &azam_doc::PieceId, k: usize) -> (Vec2, azam_doc::PointId) {
        let ol = self.app(cx).session().project.pieces[piece].outline();
        let seg = &ol.segments[k];
        (seg.point_at(0.5).pos, seg.id.clone())
    }

    /// The centre of `piece`'s bounding box.
    fn centre(&self, cx: &App, piece: &azam_doc::PieceId) -> Vec2 {
        let pts = &self.app(cx).session().project.pieces[piece].outline().poly;
        let b = azam_geom::bbox(pts.iter().copied()).expect("outline");
        b.center()
    }
}

#[test]
fn rectangle_and_polygon_draw_pieces() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let before = ui.app(cx).session().project.pieces.len();
        ui.key(cx, "s", "s");
        let canvas = cx.element_bounds(ui.win.id, PATTERN_ID).expect("pattern canvas");
        let right = canvas.origin.x + canvas.size.width;
        let top = canvas.origin.y;
        cx.simulate_drag(
            ui.win.id,
            (right - 160.0, top + 60.0),
            (right - 70.0, top + 150.0),
        );
        cx.draw(ui.win.id);
        let app = ui.app(cx);
        assert_eq!(
            app.session().project.pieces.len(),
            before + 1,
            "a rectangle piece"
        );
        let made = app.session().project.pieces.values().last().expect("new piece");
        assert!(made.name.starts_with("Rectangle"), "{}", made.name);
        // Adjust Last Operation: 200 mm wide, still one new piece
        assert!(matches!(
            app.last_op(),
            Some(azam_app::redo_panel::LastKind::Shape(_))
        ));
        ui.win.root().update(cx, |app, cx| {
            app.adjust_last(0, 200.0);
            cx.notify();
        });
        cx.draw(ui.win.id);
        let app = ui.app(cx);
        assert_eq!(
            app.session().project.pieces.len(),
            before + 1,
            "redone, not added"
        );
        let made = app.session().project.pieces.values().last().expect("new piece");
        let b = azam_geom::bbox(made.pts.iter().map(|q| q.pos)).expect("outline");
        assert!((b.max.x - b.min.x - 200.0).abs() < 1e-6, "200 mm wide");
        assert_eq!(made.pts.len(), 4);
        ui.shot(cx, "rectangle");
        ui.undo(cx);
        assert_eq!(
            ui.app(cx).session().project.pieces.len(),
            before,
            "undo removes it"
        );

        ui.key(cx, "h", "h");
        let corner = ui
            .app(cx)
            .pattern_view()
            .to_pattern(Vec2::new(f64::from(canvas.size.width) - 220.0, 90.0));
        for d in [(0.0, 0.0), (250.0, 0.0), (120.0, 220.0)] {
            ui.click(cx, corner + Vec2::new(d.0, d.1));
        }
        ui.shot(cx, "polygon-draft");
        ui.key(cx, "", "Return");
        let app = ui.app(cx);
        assert_eq!(
            app.session().project.pieces.len(),
            before + 1,
            "Enter finishes the polygon"
        );
        let made = app.session().project.pieces.values().last().expect("new piece");
        assert_eq!(made.pts.len(), 3);
        ui.undo(cx);
        assert_eq!(ui.app(cx).session().project.pieces.len(), before);
    });
}

#[test]
fn notches_topstitches_and_buttons_go_on_pieces() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let (mid, _) = ui.segment_middle(cx, &back, 0);

        ui.search(cx, "notch");
        ui.click(cx, mid);
        assert_eq!(
            ui.app(cx).session().project.pieces[&back].notches.len(),
            1,
            "a notch"
        );
        ui.undo(cx);
        assert_eq!(ui.app(cx).session().project.pieces[&back].notches.len(), 0);

        let stitches_before = ui.app(cx).session().project.stitches.len();
        ui.key(cx, "k", "k");
        ui.click(cx, mid);
        let stitches = &ui.app(cx).session().project.stitches;
        assert_eq!(stitches.len(), stitches_before + 1, "a topstitch");
        assert_eq!(
            stitches.values().last().and_then(|s| s.p.clone()),
            Some(back.clone())
        );
        ui.shot(cx, "topstitch");
        // Delete removes the selected topstitch, not its piece.
        ui.key(cx, "", "Delete");
        let app = ui.app(cx);
        assert_eq!(app.session().project.stitches.len(), stitches_before);
        assert!(app.session().project.pieces.contains_key(&back));
        ui.undo(cx);
        assert_eq!(
            ui.app(cx).session().project.stitches.len(),
            stitches_before + 1,
            "undo brings it back"
        );
        ui.undo(cx);
        assert_eq!(
            ui.app(cx).session().project.stitches.len(),
            stitches_before,
            "undo removes it"
        );

        let buttons_before = ui.app(cx).session().project.buttons.len();
        ui.search(cx, "button");
        let centre = ui.centre(cx, &back);
        ui.click(cx, centre);
        assert_eq!(
            ui.app(cx).session().project.buttons.len(),
            buttons_before + 1,
            "a button"
        );
        ui.shot(cx, "button");
        ui.undo(cx);
        assert_eq!(ui.app(cx).session().project.buttons.len(), buttons_before);
    });
}

#[test]
fn segment_sewing_and_flipping_a_seam() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let seams_before = ui.app(cx).session().project.sewing.len();
        let n = ui.app(cx).session().project.pieces[&back]
            .outline()
            .segments
            .len();
        // Sew the first segment to the last one (near their starts).
        let (a, _) = ui.segment_middle(cx, &back, 0);
        let (b, _) = ui.segment_middle(cx, &back, n - 1);
        ui.key(cx, "n", "n");
        ui.click(cx, a);
        ui.click(cx, b);
        let app = ui.app(cx);
        assert_eq!(app.session().project.sewing.len(), seams_before + 1, "a new seam");
        let seam = app
            .session()
            .selection
            .seam
            .clone()
            .expect("the new seam is selected");
        let side = app.session().project.sewing[&seam].b[0].clone();
        ui.shot(cx, "sewn");

        // Select it with Edit Sewing, then flip side B.
        let app = ui.app(cx);
        let ol = app.session().project.pieces[&side.piece].outline();
        let pl = side.side.polyline(&ol, DEFAULT_SIDE_STEP_MM);
        let on_side = pl[pl.len() / 2].pos;
        let direction = side.side.direction;
        ui.key(cx, "b", "b");
        ui.click(cx, on_side);
        assert!(
            ui.app(cx).session().selection.seam.is_some(),
            "a seam is selected"
        );
        let picked = ui.app(cx).session().selection.seam.clone().expect("seam");
        let picked_dir = ui.app(cx).session().project.sewing[&picked].b[0].side.direction;
        ui.search(cx, "flip seam");
        let flipped = ui.app(cx).session().project.sewing[&picked].b[0].side.direction;
        assert_ne!(flipped, picked_dir, "side B runs the other way");
        ui.undo(cx);
        assert_eq!(
            ui.app(cx).session().project.sewing[&picked].b[0].side.direction,
            picked_dir
        );
        assert_eq!(
            ui.app(cx).session().project.sewing[&seam].b[0].side.direction,
            direction
        );

        ui.undo(cx);
        assert_eq!(
            ui.app(cx).session().project.sewing.len(),
            seams_before,
            "undo removes the seam"
        );
    });
}

#[test]
fn shift_click_sews_several_segments_per_side() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let front = ui.piece(cx, "Front");
        let before = ui.app(cx).session().project.sewing.len();
        let (a0, _) = ui.segment_middle(cx, &back, 0);
        let (a1, _) = ui.segment_middle(cx, &back, 1);
        let (b0, _) = ui.segment_middle(cx, &front, 0);
        ui.key(cx, "n", "n");
        ui.shift_click(cx, a0);
        ui.shift_click(cx, a1);
        ui.key(cx, "", "Return");
        ui.click(cx, b0);
        let app = ui.app(cx);
        assert_eq!(app.session().project.sewing.len(), before + 1);
        let seam = app.session().project.sewing.values().last().expect("seam");
        assert_eq!((seam.a.len(), seam.b.len()), (2, 1), "a 2:1 seam");
    });
}

/// Builds a pattern with every kind of part, zoomed in, and saves screenshots of the tools'
/// feedback. Runs only when `AZAM_SHOT_DIR` is set.
#[test]
fn screenshots_of_the_tools() {
    if std::env::var_os("AZAM_SHOT_DIR").is_none() {
        return;
    }
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let centre = ui.centre(cx, &back);
        let (cx_, cy_) = ui.at(cx, centre);
        for _ in 0..6 {
            cx.simulate_scroll(ui.win.id, cx_, cy_, 1);
        }
        cx.draw(ui.win.id);
        let n = ui.app(cx).session().project.pieces[&back]
            .outline()
            .segments
            .len();
        let (side, _) = ui.segment_middle(cx, &back, n / 2);
        let (top, _) = ui.segment_middle(cx, &back, 0);

        ui.search(cx, "notch");
        ui.click(cx, side);
        ui.key(cx, "k", "k");
        ui.shift_click(cx, top);
        ui.search(cx, "button");
        ui.click(cx, centre + Vec2::new(0.0, -120.0));
        ui.search(cx, "buttonhole");
        ui.click(cx, centre + Vec2::new(0.0, 120.0));
        ui.search(cx, "fasten");
        ui.click(cx, centre + Vec2::new(0.0, -120.0));
        ui.click(cx, centre + Vec2::new(0.0, 120.0));
        ui.search(cx, "comment");
        ui.click(cx, centre + Vec2::new(-120.0, 0.0));
        ui.key(cx, "r", "r");
        let (a, b) = (
            ui.at(cx, centre + Vec2::new(60.0, -60.0)),
            ui.at(cx, centre + Vec2::new(160.0, 20.0)),
        );
        cx.simulate_drag(ui.win.id, a, b);
        cx.draw(ui.win.id);
        ui.key(cx, "t", "t");
        ui.click(cx, centre + Vec2::new(-80.0, 200.0));
        ui.shot(cx, "parts");
        ui.key(cx, "e", "e");
        let (x, y) = ui.at(cx, side);
        cx.simulate_mouse_move(ui.win.id, x + 2.0, y);
        cx.draw(ui.win.id);

        ui.shot(cx, "edit-hover");

        ui.key(cx, "n", "n");
        ui.click(cx, top);
        let (x, y) = ui.at(cx, side);
        cx.simulate_mouse_move(ui.win.id, x, y);
        cx.draw(ui.win.id);
        ui.shot(cx, "sew-picking");
        ui.key(cx, "", "Escape");

        ui.key(cx, "w", "w");
        ui.click(cx, centre + Vec2::new(-150.0, 120.0));
        ui.shot(cx, "transform");

        ui.key(cx, "h", "h");
        let corner = centre + Vec2::new(400.0, -250.0);
        ui.click(cx, corner);
        ui.click(cx, corner + Vec2::new(200.0, 30.0));
        let (x, y) = ui.at(cx, corner + Vec2::new(150.0, 200.0));
        cx.simulate_mouse_move(ui.win.id, x, y);
        cx.draw(ui.win.id);
        ui.shot(cx, "polygon");
        ui.key(cx, "", "Escape");

        ui.key(cx, "b", "b");
        let side_b = {
            let app = ui.app(cx);
            let sd = app
                .session()
                .project
                .sewing
                .values()
                .flat_map(|sw| sw.a.iter().chain(&sw.b))
                .find(|sd| sd.piece == back)
                .expect("a seam on the back");
            let ol = app.session().project.pieces[&sd.piece].outline();
            let pl = sd.side.polyline(&ol, DEFAULT_SIDE_STEP_MM);
            pl[pl.len() / 2].pos
        };
        ui.click(cx, side_b);
        ui.shot(cx, "edit-sewing");

        let (x, y) = ui.at(cx, centre);
        for _ in 0..8 {
            cx.simulate_scroll(ui.win.id, x, y, 1);
        }
        ui.shot(cx, "parts-zoomed");
    });
}

#[test]
fn free_sewing_joins_picked_stretches() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let front = ui.piece(cx, "Front");
        let before = ui.app(cx).session().project.sewing.len();
        let (b0, b0_id) = ui.segment_middle(cx, &back, 0);
        let (b1, b1_id) = ui.segment_middle(cx, &back, 1);
        let (f0, _) = ui.segment_middle(cx, &front, 0);
        let (f1, _) = ui.segment_middle(cx, &front, 1);
        // clicks land on whole pixels: one pixel along segment 0, as a fraction of it
        let pixel_t = {
            let app = ui.app(cx);
            let seg = &app.session().project.pieces[&back].outline().segments[0];
            1.0 / (app.pattern_view().zoom * seg.point_at(0.0).pos.distance(seg.point_at(1.0).pos))
        };
        cx.simulate_key_with(ui.win.id, "N", "n", false, true);
        cx.draw(ui.win.id);
        assert_eq!(ui.app(cx).current_tool(), azam_app::operators::Tool::FreeSew);
        for q in [b0, b1, f0, f1] {
            ui.click(cx, q);
        }
        let app = ui.app(cx);
        assert_eq!(app.session().project.sewing.len(), before + 1, "a free seam");
        let seam = app.session().project.sewing.values().last().expect("seam");
        let side = &seam.a[0].side;
        assert_eq!((&side.a.segment, &side.b.segment), (&b0_id, &b1_id));
        assert!(
            (side.a.t - 0.5).abs() < 0.02 + pixel_t,
            "starts where it was clicked (t = {}, one pixel = {pixel_t})",
            side.a.t
        );
        ui.undo(cx);
        assert_eq!(ui.app(cx).session().project.sewing.len(), before);
    });
}

#[test]
fn curvature_bends_a_segment() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let before = ui.app(cx).session().project.pieces[&back].pts.clone();
        let ol = ui.app(cx).session().project.pieces[&back].outline();
        let k = ol
            .segments
            .iter()
            .position(|s| s.ctrl.len() == 2 && s.len > 200.0)
            .expect("a long straight segment");
        let mid = ol.segments[k].point_at(0.5);
        let out = mid.pos + azam_geom::Vec2::new(mid.tangent.y, -mid.tangent.x) * 40.0;
        ui.key(cx, "c", "c");
        let (from, to) = (ui.at(cx, mid.pos), ui.at(cx, out));
        cx.simulate_drag(ui.win.id, from, to);
        cx.draw(ui.win.id);
        let after = &ui.app(cx).session().project.pieces[&back].pts;
        assert_eq!(after.len(), before.len() + 1, "one control point bends it");
        assert_eq!(
            after.iter().filter(|p| p.curve).count(),
            before.iter().filter(|p| p.curve).count() + 1
        );
        ui.undo(cx);
        assert_eq!(ui.app(cx).session().project.pieces[&back].pts, before);
    });
}

#[test]
fn dart_tool_cuts_a_sewn_dart() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let (middle, seams_before) = {
            let app = ui.app(cx);
            let ol = app.session().project.pieces[&back].outline();
            let longest = ol
                .segments
                .iter()
                .max_by(|a, b| a.len.total_cmp(&b.len))
                .expect("segments");
            (longest.point_at(0.5).pos, app.session().project.sewing.len())
        };
        ui.key(cx, "d", "d");
        assert_eq!(ui.app(cx).current_tool(), azam_app::operators::Tool::Dart);
        ui.click(cx, middle);
        let app = ui.app(cx);
        assert_eq!(
            app.session().project.sewing.len(),
            seams_before + 1,
            "the dart's legs are sewn"
        );
        let seam = app.session().project.sewing.values().last().expect("seam");
        assert!(
            seam.a[0].piece == back && seam.b[0].piece == back,
            "both legs are on the back"
        );
        // Adjust Last Operation: a wider dart replaces it, still one dart
        let points_after_cut = ui.app(cx).session().project.pieces[&back].pts.len();
        assert!(ui.app(cx).last_dart().is_some(), "the panel offers the last dart");
        ui.win.root().update(cx, |app, cx| {
            app.redo_dart(Some(40.0), None);
            cx.notify();
        });
        cx.draw(ui.win.id);
        let app = ui.app(cx);
        assert_eq!(
            app.session().project.sewing.len(),
            seams_before + 1,
            "still one dart seam"
        );
        assert_eq!(app.session().project.pieces[&back].pts.len(), points_after_cut);
        assert!((app.last_dart().expect("still adjustable").width - 40.0).abs() < 1e-9);
        ui.undo(cx);
        assert_eq!(ui.app(cx).session().project.sewing.len(), seams_before);
    });
}

#[test]
fn g_and_r_move_and_turn_pieces_modally() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let centre = ui.centre(cx, &back);
        ui.click(cx, centre);
        assert!(ui.app(cx).session().selection.pieces.contains(&back));
        let first = |cx: &App| ui.app(cx).session().project.pieces[&back].pts[0].pos;
        let start = first(cx);
        let zoom = ui.app(cx).pattern_view().zoom;
        let (x, y) = ui.at(cx, centre);

        // G, move 40 px, click: the piece follows and stays
        cx.simulate_mouse_move(ui.win.id, x, y);
        ui.key(cx, "g", "g");
        assert_eq!(ui.app(cx).modal_kind(), Some(azam_app::modal::ModalKind::Move));
        cx.simulate_mouse_move(ui.win.id, x + 40.0, y);
        cx.draw(ui.win.id);
        cx.simulate_click(ui.win.id, x + 40.0, y);
        cx.draw(ui.win.id);
        assert_eq!(ui.app(cx).modal_kind(), None);
        let moved = first(cx);
        assert!(
            (moved.x - start.x - 40.0 / zoom).abs() < 1.0 / zoom,
            "moved {} mm",
            moved.x - start.x
        );

        // G, move, Esc: everything goes back
        cx.simulate_mouse_move(ui.win.id, x + 40.0, y);
        ui.key(cx, "g", "g");
        cx.simulate_mouse_move(ui.win.id, x + 120.0, y + 30.0);
        cx.draw(ui.win.id);
        ui.key(cx, "", "Escape");
        assert!(first(cx).distance(moved) < 1e-9, "Esc puts the piece back");
        cx.simulate_key_with(ui.win.id, "Z", "z", true, true);
        cx.draw(ui.win.id);
        assert!(
            first(cx).distance(moved) < 1e-9,
            "and redo does not bring the cancelled move back"
        );

        // R, sweep the pointer round the middle, Enter: the piece turns
        let (cx0, cy0) = ui.at(cx, ui.centre(cx, &back));
        cx.simulate_mouse_move(ui.win.id, cx0 + 60.0, cy0);
        ui.key(cx, "r", "r");
        assert_eq!(ui.app(cx).modal_kind(), Some(azam_app::modal::ModalKind::Rotate));
        cx.simulate_mouse_move(ui.win.id, cx0, cy0 + 60.0);
        cx.draw(ui.win.id);
        ui.key(cx, "", "Return");
        assert_eq!(ui.app(cx).modal_kind(), None);
        assert!(first(cx).distance(moved) > 1.0, "R turned the piece");

        // G then X: the move keeps to the horizontal
        let before = first(cx);
        let (cx1, cy1) = ui.at(cx, ui.centre(cx, &back));
        cx.simulate_mouse_move(ui.win.id, cx1, cy1);
        ui.key(cx, "g", "g");
        ui.key(cx, "x", "x");
        assert_eq!(ui.app(cx).modal_axis(), Some(azam_app::modal::Axis::X));
        cx.simulate_mouse_move(ui.win.id, cx1 + 30.0, cy1 + 40.0);
        cx.draw(ui.win.id);
        ui.key(cx, "", "Return");
        let after = first(cx);
        assert!(
            (after.y - before.y).abs() < 1e-9 && (after.x - before.x).abs() > 1.0,
            "only x moved"
        );

        // S: twice as far from the middle, twice the size
        let width = |cx: &App| {
            let pts = &ui.app(cx).session().project.pieces[&back].pts;
            let b = azam_geom::bbox(pts.iter().map(|q| q.pos)).expect("outline");
            b.max.x - b.min.x
        };
        let w0 = width(cx);
        let (mx, my) = ui.at(cx, ui.centre(cx, &back));
        cx.simulate_mouse_move(ui.win.id, mx + 40.0, my);
        ui.key(cx, "s", "s");
        assert_eq!(ui.app(cx).modal_kind(), Some(azam_app::modal::ModalKind::Scale));
        cx.simulate_mouse_move(ui.win.id, mx + 80.0, my);
        cx.draw(ui.win.id);
        ui.key(cx, "", "Return");
        let w1 = width(cx);
        assert!((w1 / w0 - 2.0).abs() < 0.1, "scaled {w0} → {w1}");
        ui.undo(cx);
        assert!(
            (width(cx) - w0).abs() < 1e-6,
            "one undo takes the whole scale back"
        );
    });
}

#[test]
fn the_point_menu_toggles_a_corner_into_a_curve_point() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let (index, pos) = {
            let pc = &ui.app(cx).session().project.pieces[&back];
            let i = (1..pc.pts.len()).find(|&i| !pc.pts[i].curve).expect("a corner");
            (i, pc.pts[i].pos)
        };
        ui.key(cx, "e", "e");
        ui.click(cx, pos);
        assert_eq!(
            ui.app(cx).session().selection.points,
            vec![index],
            "the corner is selected"
        );
        let (x, y) = ui.at(cx, pos);
        ui.win.root().update(cx, |app, cx| {
            app.open_context_menu(azam_app::operators::ContextMenu::Point, point(px(x), px(y)));
            cx.notify();
        });
        cx.draw(ui.win.id);
        cx.draw(ui.win.id);
        let item = cx
            .element_bounds(ui.win.id, "op-Toggle Corner / Curve")
            .expect("in the Point menu");
        cx.simulate_click(
            ui.win.id,
            item.origin.x + 20.0,
            item.origin.y + item.size.height / 2.0,
        );
        cx.draw(ui.win.id);
        assert!(
            ui.app(cx).session().project.pieces[&back].pts[index].curve,
            "now a curve point"
        );
    });
}

#[test]
fn typed_numbers_set_modal_transforms_exactly() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let centre = ui.centre(cx, &back);
        ui.click(cx, centre);
        let first = |cx: &App| ui.app(cx).session().project.pieces[&back].pts[0].pos;
        let start = first(cx);
        let (x, y) = ui.at(cx, centre);

        // G, Y, 2, 5, Backspace, 0, Enter: exactly 20 mm down, whatever the pointer does
        cx.simulate_mouse_move(ui.win.id, x, y);
        ui.key(cx, "g", "g");
        ui.key(cx, "y", "y");
        for (text, sym) in [("2", "2"), ("5", "5"), ("", "BackSpace"), ("0", "0")] {
            ui.key(cx, text, sym);
        }
        assert_eq!(ui.app(cx).modal_typed(), Some("20"));
        cx.simulate_mouse_move(ui.win.id, x + 80.0, y - 50.0);
        cx.draw(ui.win.id);
        ui.key(cx, "", "Return");
        let moved = first(cx);
        assert!(
            (moved - start - azam_geom::Vec2::new(0.0, 20.0)).length() < 1e-6,
            "moved {:?}",
            moved - start
        );

        // R, 9, 0, Enter: a quarter turn about the middle keeps the distance to it
        let mid = ui.centre(cx, &back);
        let (x, y) = ui.at(cx, mid);
        cx.simulate_mouse_move(ui.win.id, x, y);
        ui.key(cx, "r", "r");
        ui.key(cx, "9", "9");
        ui.key(cx, "0", "0");
        ui.key(cx, "", "Return");
        let turned = first(cx);
        let (a, b) = (moved - mid, turned - mid);
        assert!(
            (a.length() - b.length()).abs() < 1e-6 && a.dot(b).abs() < 1e-6 * a.length_squared(),
            "a quarter turn"
        );

        // S, 2, Esc: cancelled, nothing changes
        cx.simulate_mouse_move(ui.win.id, x, y);
        ui.key(cx, "s", "s");
        ui.key(cx, "2", "2");
        ui.key(cx, "", "Escape");
        assert!(first(cx).distance(turned) < 1e-9, "Esc puts the typed scale back");
    });
}

#[test]
fn the_cut_tool_splits_a_piece_and_the_fillet_tool_rounds_a_corner() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let count = |cx: &App| ui.app(cx).session().project.pieces.len();
        let before = count(cx);
        // U, then drag a horizontal line across the middle of the back
        ui.key(cx, "u", "u");
        assert_eq!(ui.app(cx).current_tool(), azam_app::operators::Tool::Cut);
        let c = ui.centre(cx, &back);
        let pts = &ui.app(cx).session().project.pieces[&back].pts;
        let (lo, hi) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.pos.x), hi.max(p.pos.x))
        });
        let (x0, y) = ui.at(cx, Vec2::new(lo - 40.0, c.y));
        let (x1, _) = ui.at(cx, Vec2::new(hi + 40.0, c.y));
        cx.simulate_drag(ui.win.id, (x0, y), (x1, y));
        cx.draw(ui.win.id);
        assert_eq!(count(cx), before + 1, "{}", ui.app(cx).status());
        assert!(
            ui.app(cx)
                .session()
                .project
                .pieces
                .values()
                .any(|pc| pc.name == "Back 2")
        );
        ui.undo(cx);
        assert_eq!(count(cx), before, "one undo puts the piece back whole");

        // F, then click the back's first corner: it becomes a corner, a curve point and a corner
        ui.key(cx, "f", "f");
        let corners = ui.app(cx).session().project.pieces[&back].pts.len();
        let first = ui.app(cx).session().project.pieces[&back].pts[0].pos;
        ui.click(cx, first);
        assert_eq!(
            ui.app(cx).session().project.pieces[&back].pts.len(),
            corners + 2,
            "{}",
            ui.app(cx).status()
        );
    });
}

#[test]
fn pleat_and_spread_tools_add_fullness() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let area = |cx: &App| {
            azam_geom::signed_area(&ui.app(cx).session().project.pieces[&back].outline().poly).abs()
        };
        let span = |cx: &App| {
            let pts = &ui.app(cx).session().project.pieces[&back].pts;
            pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                (lo.min(p.pos.y), hi.max(p.pos.y))
            })
        };
        let drag_across = |cx: &mut App, x_mm: f64| {
            let (lo, hi) = span(cx);
            let (x, y0) = ui.at(cx, Vec2::new(x_mm, hi + 40.0));
            let (_, y1) = ui.at(cx, Vec2::new(x_mm, lo - 40.0));
            cx.simulate_drag(ui.win.id, (x, y0), (x, y1));
            cx.draw(ui.win.id);
        };
        let before = area(cx);
        let internals = ui.app(cx).session().project.pieces[&back].internals.len();
        ui.search(cx, "pleat");
        assert_eq!(ui.app(cx).current_tool(), azam_app::operators::Tool::Pleat);
        drag_across(cx, ui.centre(cx, &back).x);
        assert!(
            area(cx) > before + 1000.0,
            "{}: {before} → {}",
            ui.app(cx).status(),
            area(cx)
        );
        assert_eq!(
            ui.app(cx).session().project.pieces[&back].internals.len(),
            internals + 2,
            "the pleat's two fold lines"
        );
        let pleated = area(cx);
        ui.search(cx, "spread");
        assert_eq!(ui.app(cx).current_tool(), azam_app::operators::Tool::Spread);
        drag_across(cx, ui.centre(cx, &back).x - 60.0);
        assert!(
            area(cx) > pleated + 500.0,
            "{}: {pleated} → {}",
            ui.app(cx).status(),
            area(cx)
        );
    });
}

#[test]
fn the_grading_panel_shows_a_rule_table_for_the_selected_point() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        ui.key(cx, "e", "e");
        let corner = ui.app(cx).session().project.pieces[&back].pts[0].clone();
        ui.click(cx, corner.pos);
        for _ in 0..3 {
            cx.draw(ui.win.id);
        }
        let sizes = ui.app(cx).session().project.sizes.clone();
        let base = ui.app(cx).session().project.base_size.clone();
        assert!(
            !ui.app(cx).session().selection.points.is_empty(),
            "{}",
            ui.app(cx).status()
        );
        for size in sizes.iter().filter(|s| **s != base) {
            let id = format!("grade-{}-{}-x", corner.id.0, size.0);
            assert!(
                cx.element_bounds(ui.win.id, &id).is_some(),
                "{id} is in the grading table"
            );
        }
    });
}

#[test]
fn adjust_last_operation_redoes_pleats_and_fillets() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let area = |cx: &App| {
            azam_geom::signed_area(&ui.app(cx).session().project.pieces[&back].outline().poly).abs()
        };
        // a fillet first: the panel offers its radius, and a new radius rounds it again
        ui.key(cx, "f", "f");
        let first = ui.app(cx).session().project.pieces[&back].pts[0].pos;
        ui.click(cx, first);
        assert!(matches!(
            ui.app(cx).last_op(),
            Some(azam_app::redo_panel::LastKind::Fillet(..))
        ));
        let points = ui.app(cx).session().project.pieces[&back].pts.len();
        win_update(cx, &ui, |app| app.adjust_last(0, 35.0));
        assert_eq!(
            ui.app(cx).session().project.pieces[&back].pts.len(),
            points,
            "redone, not added"
        );
        let pts = &ui.app(cx).session().project.pieces[&back].pts;
        let (lo, hi) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.pos.y), hi.max(p.pos.y))
        });
        let x = ui.centre(cx, &back).x;
        ui.search(cx, "pleat");
        let (sx, sy0) = ui.at(cx, Vec2::new(x, hi + 40.0));
        let (_, sy1) = ui.at(cx, Vec2::new(x, lo - 40.0));
        cx.simulate_drag(ui.win.id, (sx, sy0), (sx, sy1));
        cx.draw(ui.win.id);
        let pleated = area(cx);
        assert!(matches!(
            ui.app(cx).last_op(),
            Some(azam_app::redo_panel::LastKind::Spread(..))
        ));
        win_update(cx, &ui, |app| app.adjust_last(0, 40.0));
        assert!(
            area(cx) > pleated + 500.0,
            "a deeper pleat: {pleated} → {}",
            area(cx)
        );
    });
}

fn win_update(cx: &mut App, ui: &Ui, f: impl FnOnce(&mut AzamApp)) {
    ui.win.root().update(cx, |app, cx| {
        f(app);
        cx.notify();
    });
    cx.draw(ui.win.id);
}

#[test]
fn a_modal_move_shows_how_far_it_has_gone() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        let back = ui.piece(cx, "Back");
        let centre = ui.centre(cx, &back);
        ui.click(cx, centre);
        let (x, y) = ui.at(cx, centre);
        cx.simulate_mouse_move(ui.win.id, x, y);
        ui.key(cx, "g", "g");
        cx.simulate_mouse_move(ui.win.id, x + 40.0, y);
        cx.draw(ui.win.id);
        let status = ui.app(cx).status().to_owned();
        assert!(status.starts_with("Move") && status.contains("ΔX"), "{status}");
        ui.key(cx, "", "Escape");
    });
}

#[test]
fn lengths_follow_the_project_units() {
    Application::headless().run(|cx| {
        let ui = Ui::open(cx);
        ui.win.root().update(cx, |app, cx| {
            app.edit_project("Units", |p| p.units = azam_doc::Units::In);
            cx.notify();
        });
        cx.draw(ui.win.id);
        assert_eq!(ui.app(cx).pattern_scene().units, azam_doc::Units::In);
        ui.key(cx, "m", "m");
        let (x0, y) = ui.at(cx, Vec2::new(0.0, -200.0));
        let (x1, _) = ui.at(cx, Vec2::new(254.0, -200.0));
        cx.simulate_drag(ui.win.id, (x0, y), (x1, y));
        cx.draw(ui.win.id);
        let status = ui.app(cx).status().to_owned();
        assert!(
            status.starts_with("Distance") && status.ends_with(" in"),
            "{status}"
        );
    });
}
