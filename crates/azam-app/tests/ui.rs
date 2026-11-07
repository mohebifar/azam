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
