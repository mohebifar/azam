//! Drives the window headless: the pattern editor, menus, operator search and playback.

use azam_app::AzamApp;
use azam_app::pattern::PATTERN_ID;
use azam_app::viewport::VIEWPORT_ID;
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

#[test]
fn select_move_and_undo_a_piece() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        for _ in 0..3 {
            cx.draw(win.id);
        }
        let canvas = cx.element_bounds(win.id, PATTERN_ID).expect("pattern canvas");
        let (view, back_centre, back_id) = {
            let app = win.root().read(cx);
            let scene = app.pattern_scene();
            let back = scene
                .pieces
                .iter()
                .position(|p| p.name == "Back")
                .expect("a Back piece");
            let pts = &scene.pieces[back].sewing;
            let centre = pts.iter().fold(azam_geom::Vec2::ZERO, |a, p| a + *p) / pts.len() as f64;
            let mut ids: Vec<_> = app.session().project.pieces.values().collect();
            ids.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
            (app.pattern_view(), centre, ids[back].id.clone())
        };
        let s = view.to_screen(back_centre);
        let (x, y) = (canvas.origin.x + s.x as f32, canvas.origin.y + s.y as f32);

        cx.simulate_click(win.id, x, y);
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).session().selection.active.as_ref(),
            Some(&back_id),
            "click selects Back"
        );

        let before = win.root().read(cx).session().project.pieces[&back_id].pts[0].pos;
        cx.simulate_key_with(win.id, "", "Delete", false, false);
        cx.draw(win.id);
        assert!(
            !win.root()
                .read(cx)
                .session()
                .project
                .pieces
                .contains_key(&back_id),
            "Delete removes it"
        );

        cx.simulate_key_with(win.id, "z", "z", true, false);
        cx.draw(win.id);
        let app = win.root().read(cx);
        assert_eq!(
            app.session().project.pieces[&back_id].pts[0].pos,
            before,
            "Ctrl+Z brings it back"
        );
    });
}

#[test]
fn operator_search_runs_an_operator() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.simulate_click(win.id, 700.0, 850.0);
        cx.draw(win.id);
        cx.simulate_key(win.id, "", "F3");
        cx.draw(win.id);
        cx.draw(win.id);
        cx.simulate_key(win.id, "s", "s");
        cx.simulate_key(win.id, "k", "k");
        cx.simulate_key(win.id, "i", "i");
        cx.simulate_key(win.id, "r", "r");
        cx.simulate_key(win.id, "t", "t");
        cx.draw(win.id);
        cx.simulate_key(win.id, "", "Return");
        cx.draw(win.id);
        let app = win.root().read(cx);
        assert_eq!(
            app.session().project.meta.name,
            "A-line Skirt",
            "searching 'skirt' and Enter opens the skirt template"
        );
    });
}

#[test]
fn space_plays_the_simulation() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        // The avatar builds on a worker thread; render until the simulation exists.
        for _ in 0..200 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
            if win.root().read(cx).sim_time().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            win.root().read(cx).sim_time(),
            Some(0.0),
            "a fresh simulation at t = 0"
        );
        cx.simulate_click(win.id, 700.0, 850.0);
        cx.simulate_key(win.id, " ", "space");
        for _ in 0..60 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        let app = win.root().read(cx);
        let t = app.sim_time().expect("simulation");
        assert!(t > 0.5, "60 frames advance the simulation (t = {t})");
        assert!(app.sim_is_finite(), "no particle went NaN");
    });
}

#[test]
fn moving_over_another_menu_title_switches_menus() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        let centre = |cx: &mut App, id: &str| {
            let b = cx.element_bounds(win.id, id).expect("menu title");
            (b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0)
        };
        let (x, y) = centre(cx, "menu-0");
        cx.simulate_click(win.id, x, y);
        cx.draw(win.id);
        cx.draw(win.id);
        assert_eq!(win.root().read(cx).open_menu_title(), Some("File"));

        // the open menu's scrim covers the top bar; moving over another title still switches
        let (x, y) = centre(cx, "menu-1");
        cx.simulate_mouse_move(win.id, x, y);
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).open_menu_title(),
            Some("Edit"),
            "hovering Edit opens it"
        );

        let (fx, fy) = centre(cx, "menu-0");
        cx.simulate_drag(win.id, (x, y), (fx, fy));
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).open_menu_title(),
            Some("File"),
            "dragging back onto File keeps it open"
        );

        cx.simulate_click(win.id, fx, fy);
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).open_menu_title(),
            None,
            "clicking the open title closes it"
        );
    });
}

#[test]
fn view_menu_opens_a_pie_in_the_middle_of_the_3d_view() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        let centre = |cx: &mut App, id: &str| {
            let b = cx.element_bounds(win.id, id).unwrap_or_else(|| panic!("{id}"));
            (b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0)
        };
        let view_menu = format!("menu-{}", win.root().read(cx).top_menu_count());
        let (x, y) = centre(cx, &view_menu);
        cx.simulate_click(win.id, x, y);
        cx.draw(win.id);
        cx.draw(win.id);
        let (x, y) = centre(cx, "submenu-Pie Menus");
        cx.simulate_click(win.id, x, y);
        cx.draw(win.id);
        cx.draw(win.id);
        let (x, y) = centre(cx, "op-Shading Pie");
        cx.simulate_click(win.id, x, y);
        cx.draw(win.id);

        let viewport = cx.element_bounds(win.id, VIEWPORT_ID).expect("3D view");
        let (px, py) = win.root().read(cx).pie_centre().expect("the shading pie opens");
        let (vx, vy) = (
            viewport.origin.x + viewport.size.width / 2.0,
            viewport.origin.y + viewport.size.height / 2.0,
        );
        assert!(
            (px - vx).abs() < 1.0 && (py - vy).abs() < 1.0,
            "pie at ({px}, {py}), view centre ({vx}, {vy})"
        );
    });
}

#[test]
fn operator_search_switches_the_fit_map() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.simulate_click(win.id, 700.0, 850.0);
        cx.draw(win.id);
        cx.simulate_key(win.id, "", "F3");
        cx.draw(win.id);
        cx.draw(win.id);
        for c in ["s", "t", "r", "a", "i", "n"] {
            cx.simulate_key(win.id, c, c);
        }
        cx.draw(win.id);
        cx.simulate_key(win.id, "", "Return");
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).fit_mode(),
            azam_sim::FitMode::Strain,
            "searching 'strain' and Enter shows the strain map"
        );
    });
}

#[test]
fn pinning_cloth_is_saved_on_the_piece() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        for _ in 0..200 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
            if win.root().read(cx).sim_time().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let pins = |cx: &App| -> usize {
            win.root()
                .read(cx)
                .session()
                .project
                .pieces
                .values()
                .map(|p| p.pins.len())
                .sum()
        };
        assert_eq!(pins(cx), 0);
        let vp = cx.element_bounds(win.id, VIEWPORT_ID).expect("3D view");
        // the chest of the avatar, where the tee's front hangs
        let (x, y) = (
            vp.origin.x + vp.size.width / 2.0,
            vp.origin.y + vp.size.height * 0.42,
        );
        cx.simulate_mouse_move(win.id, x, y);
        cx.simulate_key(win.id, "i", "i");
        cx.draw(win.id);
        cx.simulate_click(win.id, x, y);
        cx.draw(win.id);
        assert_eq!(pins(cx), 1, "a click with the Pin tool stores a pin on the piece");
        cx.simulate_click(win.id, x, y);
        cx.draw(win.id);
        assert_eq!(pins(cx), 0, "a second click on it removes the pin");
    });
}

#[test]
fn dragging_an_area_border_resizes_the_side_column() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.draw(win.id);
        let before = win.root().read(cx).area_layout().side_px;
        let border = cx
            .element_bounds(win.id, "border-Side")
            .expect("the side column's border");
        let x = border.origin.x + border.size.width / 2.0;
        let y = border.origin.y + border.size.height / 2.0;
        cx.simulate_drag(win.id, (x, y), (x - 100.0, y));
        cx.draw(win.id);
        let after = win.root().read(cx).area_layout().side_px;
        assert!(
            (after - (before + 100.0)).abs() < 3.0,
            "side column {before} → {after}"
        );
    });
}

fn wait_for_garment(cx: &mut App, win: &WindowHandle<AzamApp>) {
    for _ in 0..300 {
        win.root().update(cx, |_, cx| cx.notify());
        cx.draw(win.id);
        if win.root().read(cx).sim_time().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    cx.draw(win.id);
}

fn front_id(cx: &App, win: &WindowHandle<AzamApp>) -> azam_doc::PieceId {
    let app = win.root().read(cx);
    app.session()
        .project
        .pieces
        .values()
        .find(|p| p.name == "Front")
        .expect("a front")
        .id
        .clone()
}

#[test]
fn dragging_a_piece_in_3d_moves_it() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        let front = front_id(cx, &win);
        let before = win.root().read(cx).piece_position(&front).expect("front cloth");
        let vp = cx.element_bounds(win.id, VIEWPORT_ID).expect("3D view");
        let (x, y) = (
            vp.origin.x + vp.size.width / 2.0,
            vp.origin.y + vp.size.height * 0.42,
        );
        cx.simulate_drag(win.id, (x, y), (x + 60.0, y));
        cx.draw(win.id);
        let app = win.root().read(cx);
        assert_eq!(
            app.session().selection.active.as_ref(),
            Some(&front),
            "the press selects the front"
        );
        let after = app.piece_position(&front).expect("front cloth");
        let moved = ((after[0] - before[0]).powi(2) + (after[2] - before[2]).powi(2)).sqrt();
        assert!(moved > 20.0, "the front moved {moved} mm sideways");
        assert!(
            app.session().project.pieces[&front].arr.moved.is_some(),
            "the move is saved on the piece"
        );

        // undo puts it back; the drape moves back with it
        cx.simulate_click(win.id, 700.0, 850.0);
        cx.simulate_key_with(win.id, "z", "z", true, false);
        for _ in 0..3 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        let app = win.root().read(cx);
        assert!(
            app.session().project.pieces[&front].arr.moved.is_none(),
            "undo forgets the move"
        );
        let back = app.piece_position(&front).expect("front cloth");
        let off = ((back[0] - before[0]).powi(2) + (back[2] - before[2]).powi(2)).sqrt();
        // (the rebuild may push a few particles out of the body, so not exactly)
        assert!(
            off < moved * 0.25,
            "undo puts the cloth back ({off} mm away after a {moved} mm move)"
        );
    });
}

#[test]
fn clicking_an_arrangement_point_reslots_the_selected_piece() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        let front = front_id(cx, &win);
        win.root().update(cx, |app, cx| {
            app.select_piece(front.clone());
            app.set_show_arrangement(true);
            cx.notify();
        });
        cx.draw(win.id);
        cx.draw(win.id);
        let dot = cx
            .element_bounds(win.id, "arr-left-Waist")
            .expect("a left-side waist point");
        cx.simulate_click(
            win.id,
            dot.origin.x + dot.size.width / 2.0,
            dot.origin.y + dot.size.height / 2.0,
        );
        cx.draw(win.id);
        let app = win.root().read(cx);
        assert_eq!(app.session().project.pieces[&front].arr.slot, "left");
    });
}

#[test]
fn the_piece_context_menu_runs_its_commands() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        let front = front_id(cx, &win);
        let before = win.root().read(cx).session().project.pieces.len();
        win.root().update(cx, |app, cx| {
            app.select_piece(front.clone());
            app.open_context_menu(
                azam_app::operators::ContextMenu::Piece,
                point(px(800.0), px(400.0)),
            );
            cx.notify();
        });
        cx.draw(win.id);
        cx.draw(win.id);
        assert_eq!(win.root().read(cx).open_context_menu_title(), Some("Piece"));
        let item = cx
            .element_bounds(win.id, "op-Duplicate Pieces")
            .expect("Duplicate in the menu");
        assert!(
            item.origin.x >= 799.0 && item.origin.y >= 399.0,
            "the menu opens at the pointer"
        );
        cx.simulate_click(
            win.id,
            item.origin.x + 20.0,
            item.origin.y + item.size.height / 2.0,
        );
        cx.draw(win.id);
        let app = win.root().read(cx);
        assert_eq!(app.session().project.pieces.len(), before + 1, "Duplicate ran");
        assert_eq!(app.open_context_menu_title(), None, "and the menu closed");
    });
}

#[test]
fn the_editor_type_menu_switches_an_area() {
    use azam_app::editors::{Editor, Slot};
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.draw(win.id);
        assert_eq!(win.root().read(cx).editor_at(Slot::Main), Some(Editor::View3d));
        let button = cx
            .element_bounds(win.id, "editor-type-0")
            .expect("the main area's editor type");
        cx.simulate_click(
            win.id,
            button.origin.x + 8.0,
            button.origin.y + button.size.height / 2.0,
        );
        cx.draw(win.id);
        cx.draw(win.id);
        let item = cx
            .element_bounds(win.id, "op-Pattern Editor")
            .expect("Pattern Editor in the menu");
        cx.simulate_click(
            win.id,
            item.origin.x + 20.0,
            item.origin.y + item.size.height / 2.0,
        );
        for _ in 0..3 {
            cx.draw(win.id);
        }
        let app = win.root().read(cx);
        assert_eq!(app.editor_at(Slot::Main), Some(Editor::Pattern));
        assert_eq!(
            app.editor_at(Slot::Second),
            Some(Editor::View3d),
            "the two areas swap"
        );
        assert!(
            cx.element_bounds(win.id, PATTERN_ID).is_some()
                && cx.element_bounds(win.id, VIEWPORT_ID).is_some()
        );
    });
}
