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
    use azam_app::editors::Editor;
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.draw(win.id);
        let (root, outliner) = {
            let app = win.root().read(cx);
            (
                app.layout().id(),
                app.area_with(Editor::Outliner).expect("an outliner"),
            )
        };
        let side_x = |cx: &mut App| {
            cx.element_bounds(win.id, &format!("editor-type-{outliner}"))
                .expect("the outliner's header")
                .origin
                .x
        };
        let before = side_x(cx);
        let border = cx
            .element_bounds(win.id, &format!("border-{root}"))
            .expect("the side column's border");
        let x = border.origin.x + border.size.width / 2.0;
        let y = border.origin.y + border.size.height / 2.0;
        cx.simulate_drag(win.id, (x, y), (x - 100.0, y));
        cx.draw(win.id);
        cx.draw(win.id);
        let after = side_x(cx);
        assert!(
            (after - (before - 100.0)).abs() < 3.0,
            "side column at {before} → {after}"
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

/// Clicks the element `id` near its left end.
fn click_element(cx: &mut App, win: &WindowHandle<AzamApp>, id: &str) {
    let b = cx
        .element_bounds(win.id, id)
        .unwrap_or_else(|| panic!("{id} is laid out"));
    cx.simulate_click(win.id, b.origin.x + 8.0, b.origin.y + b.size.height / 2.0);
    for _ in 0..3 {
        cx.draw(win.id);
    }
}

#[test]
fn the_editor_type_menu_switches_an_area() {
    use azam_app::editors::Editor;
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.draw(win.id);
        let (view, pattern) = {
            let app = win.root().read(cx);
            (
                app.area_with(Editor::View3d).expect("3D view"),
                app.area_with(Editor::Pattern).expect("pattern"),
            )
        };
        click_element(cx, &win, &format!("editor-type-{view}"));
        click_element(cx, &win, "op-Pattern Editor");
        let app = win.root().read(cx);
        assert_eq!(app.editor_of(view), Some(Editor::Pattern));
        assert_eq!(app.editor_of(pattern), Some(Editor::View3d), "the two areas swap");
        assert!(
            cx.element_bounds(win.id, PATTERN_ID).is_some()
                && cx.element_bounds(win.id, VIEWPORT_ID).is_some()
        );
    });
}

#[test]
fn areas_split_freely_and_join() {
    use azam_app::areas::Dir;
    use azam_app::editors::Editor;
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        win.root().update(cx, |app, cx| {
            app.set_workspace(azam_app::app::Workspace::Review);
            cx.notify();
        });
        cx.draw(win.id);
        let view = {
            let app = win.root().read(cx);
            assert_eq!(app.area_with(Editor::Pattern), None, "Review has one large area");
            app.area_with(Editor::View3d).expect("3D view")
        };
        let pattern = win.root().update(cx, |app, cx| {
            cx.notify();
            app.split_area(view, Dir::Row).expect("the 3D view splits")
        });
        cx.draw(win.id);
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).editor_of(pattern),
            Some(Editor::Pattern),
            "the new area shows the pattern"
        );
        assert!(
            cx.element_bounds(win.id, PATTERN_ID).is_some(),
            "and it is laid out"
        );
        // the new area splits again, one over the other, from its editor-type menu
        click_element(cx, &win, &format!("editor-type-{pattern}"));
        click_element(cx, &win, "op-Horizontal Split");
        let shown = win.root().read(cx).editors_shown();
        assert_eq!(
            shown,
            vec![
                Editor::View3d,
                Editor::Pattern,
                Editor::Properties,
                Editor::Outliner,
                Editor::Properties
            ],
            "no second pattern editor; the properties show twice"
        );
        let props: Vec<_> = win
            .root()
            .read(cx)
            .layout()
            .leaves()
            .into_iter()
            .filter(|&(_, e)| e == Editor::Properties)
            .map(|(id, _)| id)
            .collect();
        for id in &props {
            assert!(
                cx.element_bounds(win.id, &format!("prop-body-{id}")).is_some(),
                "both properties editors are laid out"
            );
        }
        win.root().update(cx, |app, cx| {
            assert!(app.join_area(pattern), "the pattern editor takes its room back");
            assert!(app.join_area(view));
            cx.notify();
        });
        cx.draw(win.id);
        let app = win.root().read(cx);
        assert_eq!(
            app.editors_shown(),
            vec![Editor::View3d, Editor::Outliner, Editor::Properties]
        );
    });
}

#[test]
fn n_over_the_3d_view_shows_the_sidebar_and_its_fields_place_the_piece() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        let vp = cx.element_bounds(win.id, VIEWPORT_ID).expect("3D view");
        cx.simulate_mouse_move(win.id, vp.origin.x + 100.0, vp.origin.y + 300.0);
        cx.simulate_key(win.id, "n", "n");
        cx.draw(win.id);
        assert!(
            win.root().read(cx).sidebar_shown(),
            "N over the 3D view opens the sidebar"
        );
        assert_ne!(
            win.root().read(cx).current_tool(),
            azam_app::operators::Tool::Sew,
            "and does not pick the Sew tool"
        );
        // over the pattern editor N is still the Sew tool
        let pat = cx.element_bounds(win.id, PATTERN_ID).expect("pattern editor");
        cx.simulate_mouse_move(win.id, pat.origin.x + 300.0, pat.origin.y + 500.0);
        cx.simulate_key(win.id, "n", "n");
        cx.draw(win.id);
        assert_eq!(win.root().read(cx).current_tool(), azam_app::operators::Tool::Sew);
    });
}

#[test]
fn an_obj_body_becomes_the_avatar() {
    use std::fmt::Write as _;
    // a body mesh in metres, as another program might export it
    let body = azam_avatar::Avatar::parametric(&azam_avatar::AvatarParams::preset(azam_avatar::Sex::Female));
    let mut obj = String::new();
    for v in body.mesh.positions.chunks_exact(3) {
        writeln!(obj, "v {} {} {}", v[0] / 1000.0, v[1] / 1000.0, v[2] / 1000.0).expect("write");
    }
    for t in body.mesh.indices.chunks_exact(3) {
        writeln!(obj, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1).expect("write");
    }
    let path = std::env::temp_dir().join(format!("azam-body-{}.obj", std::process::id()));
    std::fs::write(&path, obj).expect("write the body");
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        win.root().update(cx, |app, cx| {
            app.set_custom_body(Some(&path.to_string_lossy()));
            cx.notify();
        });
        for _ in 0..400 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
            if win.root().read(cx).avatar_kind() == Some(azam_avatar::AvatarKind::Custom) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let app = win.root().read(cx);
        assert_eq!(app.avatar_kind(), Some(azam_avatar::AvatarKind::Custom));
        assert!(
            app.sim_time().is_some(),
            "the garment simulates on the custom body"
        );
    });
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_draped_garment_exports_as_gltf() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        let path = std::env::temp_dir().join(format!("azam-export-{}.glb", std::process::id()));
        win.root()
            .read(cx)
            .export_to(azam_app::export::ExportKind::Gltf, &path)
            .expect("exported");
        let glb = std::fs::read(&path).expect("written");
        let _ = std::fs::remove_file(&path);
        assert_eq!(&glb[0..4], b"glTF");
        let json_len = u32::from_le_bytes(glb[12..16].try_into().expect("4 bytes")) as usize;
        let doc: serde_json::Value = serde_json::from_slice(&glb[20..20 + json_len]).expect("JSON chunk");
        let names: Vec<&str> = doc["meshes"]
            .as_array()
            .expect("meshes")
            .iter()
            .filter_map(|m| m["name"].as_str())
            .collect();
        assert!(names.contains(&"Front") && names.contains(&"Avatar"), "{names:?}");
    });
}

#[test]
fn the_astm_export_writes_the_dxf_and_its_rule_table() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        let path = std::env::temp_dir().join(format!("azam-astm-{}.dxf", std::process::id()));
        win.root()
            .read(cx)
            .export_to(azam_app::export::ExportKind::Astm, &path)
            .expect("exported");
        let dxf = std::fs::read_to_string(&path).expect("DXF written");
        let rul = std::fs::read_to_string(path.with_extension("rul")).expect("rule table written");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("rul"));
        assert!(rul.starts_with("ASTM/D13Proposal") && rul.contains("RULE: DELTA 1"));
        assert!(dxf.contains("Rule Table:") && dxf.contains("# 1"));
        let pieces = azam_io::import_dxf(&dxf).expect("azam reads its own ASTM DXF");
        assert!(pieces.len() >= 2, "{} pieces", pieces.len());
    });
}

#[test]
fn an_astm_export_imports_back_as_the_same_graded_pattern() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        let path = std::env::temp_dir().join(format!("azam-astm-rt-{}.dxf", std::process::id()));
        win.root()
            .read(cx)
            .export_to(azam_app::export::ExportKind::Astm, &path)
            .expect("exported");
        let original = win.root().read(cx).session().project.clone();
        let before = original.pieces.len();
        let note = win
            .root()
            .update(cx, |app, _| {
                app.import_path(azam_app::import::ImportFormat::Dxf, &path)
            })
            .expect("imported");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("rul"));
        assert!(note.contains("graded"), "{note}");
        let project = win.root().read(cx).session().project.clone();
        assert_eq!(project.sizes, original.sizes);
        assert_eq!(project.base_size, original.base_size);
        // each imported piece's outline in each size is the original's cut line in that size
        let cut_area = |pc: &azam_doc::Piece, size: &azam_geom::SizeName| {
            let ol = pc.graded_outline(size, &project.base_size);
            let cut = azam_geom::cut_outline(&ol, |id| pc.allowances().finish(id));
            azam_geom::signed_area(&cut).abs()
        };
        let mut imported: Vec<_> = project.pieces.values().collect();
        imported.sort_by_key(|pc| pc.id.0.clone());
        let mut originals: Vec<_> = original.pieces.values().collect();
        originals.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        let new_pieces: Vec<_> = imported
            .iter()
            .filter(|pc| !original.pieces.contains_key(&pc.id))
            .collect();
        assert_eq!(new_pieces.len(), before);
        for size in &original.sizes {
            let total = |pcs: &[&azam_doc::Piece]| pcs.iter().map(|pc| cut_area(pc, size)).sum::<f64>();
            let new: Vec<&azam_doc::Piece> = new_pieces.iter().map(|pc| **pc).collect();
            let (a, b) = (total(&originals), total(&new));
            assert!((a - b).abs() / a < 0.005, "size {}: {a:.0} vs {b:.0} mm²", size.0);
        }
    });
}

#[test]
fn a_u3m_archive_imports_as_a_fabric_with_its_images_unpacked() {
    // a 4 × 2 PNG, as both the colour and the normal map
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&vec![128; (w * h * 4) as usize]))
            .expect("png");
        out
    }
    let image = png(4, 2);
    let u3m = r#"{"schema": "1.0", "material": {"name": "Scanned Denim",
        "front": {"shader": "principled",
          "basecolor": {"constant": {"r": 0.05, "g": 0.08, "b": 0.2},
            "texture": {"mode": "multiply", "factor": {"r": 1, "g": 1, "b": 1},
              "image": {"path": "maps/color.png", "width": 0, "height": 0, "dpi": 254, "repeat": null}}},
          "normal": {"constant": 0, "texture": {"factor": 1, "offset": 0,
              "image": {"path": "maps/normal.png", "width": 0, "height": 0, "dpi": 254, "repeat": null}}},
          "roughness": {"constant": 0.9, "texture": null}},
        "back": null}, "custom": null}"#;
    let zip = azam_io::write_zip(&[
        ("denim.u3m", u3m.as_bytes()),
        ("maps/color.png", &image),
        ("maps/normal.png", &image),
        ("../escape.png", &image),
    ]);
    let dir = std::env::temp_dir().join(format!("azam-u3m-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("denim.u3ma");
    std::fs::write(&path, zip).expect("written");
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        let note = win
            .root()
            .update(cx, |app, _| app.import_u3m_into(&path, &dir.join("lib")))
            .expect("imported");
        assert!(note.contains("Scanned Denim"), "{note}");
        let project = win.root().read(cx).session().project.clone();
        let fabric = project
            .fabrics
            .values()
            .find(|f| f.name == "Scanned Denim")
            .expect("the new fabric");
        let (texture, tile) = fabric.texture().expect("colour image");
        assert!(std::path::Path::new(texture).exists(), "unpacked: {texture}");
        // 4 px at 254 dpi is 0.4 mm
        assert!((tile - 0.4).abs() < 1e-9, "tile {tile}");
        let maps = fabric.surface_maps().expect("normal map");
        assert!(std::path::Path::new(maps.normal.expect("normal")).exists());
        assert!((fabric.rough - 0.9).abs() < 1e-12);
        assert!(
            !dir.join("escape.png").exists(),
            "nothing written outside the folder"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_shortcut_set_in_preferences_runs_its_command() {
    use azam_app::operators::{Operator, Tool};
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.simulate_mouse_move(win.id, 700.0, 400.0);
        cx.draw(win.id);
        cx.simulate_key_with(win.id, ",", "comma", true, false);
        cx.draw(win.id);
        cx.draw(win.id);
        let click = |cx: &mut App, id: &str| {
            let b = cx
                .element_bounds(win.id, id)
                .unwrap_or_else(|| panic!("{id} is laid out"));
            cx.simulate_click(
                win.id,
                b.origin.x + b.size.width / 2.0,
                b.origin.y + b.size.height / 2.0,
            );
            cx.draw(win.id);
            cx.draw(win.id);
        };
        assert!(
            win.root().read(cx).prefs_open(),
            "Ctrl Comma opens the preferences"
        );
        click(cx, "prefs-tab-Keymap");
        click(cx, "keymap-filter");
        for c in "measure".chars() {
            let s = c.to_string();
            cx.simulate_key(win.id, &s, &s);
        }
        cx.draw(win.id);
        cx.draw(win.id);
        assert!(
            win.root().read(cx).prefs_open(),
            "typing into the filter keeps the window open"
        );
        click(cx, "bind-Tool: Measure");
        cx.simulate_key(win.id, "q", "q");
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).prefs().operator_for("q"),
            Some(Operator::SetTool(Tool::Measure)),
            "Q is bound to the Measure tool"
        );
        assert_eq!(
            win.root()
                .read(cx)
                .shortcut_label(Operator::SetTool(Tool::Measure))
                .as_deref(),
            Some("Q"),
            "menus show the new shortcut"
        );
        // Esc closes the window; then Q picks the tool
        cx.simulate_key(win.id, "", "Escape");
        cx.draw(win.id);
        let pat = cx.element_bounds(win.id, PATTERN_ID).expect("pattern editor");
        cx.simulate_mouse_move(win.id, pat.origin.x + 300.0, pat.origin.y + 300.0);
        cx.simulate_key(win.id, "q", "q");
        cx.draw(win.id);
        assert_eq!(win.root().read(cx).current_tool(), Tool::Measure);
    });
}

#[test]
fn g_and_r_move_and_turn_a_piece_in_3d() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        let front = front_id(cx, &win);
        let pos = |cx: &App| win.root().read(cx).piece_position(&front).expect("front cloth");
        let start = pos(cx);
        let vp = cx.element_bounds(win.id, VIEWPORT_ID).expect("3D view");
        let (x, y) = (
            vp.origin.x + vp.size.width / 2.0,
            vp.origin.y + vp.size.height * 0.42,
        );
        cx.simulate_mouse_move(win.id, x, y);
        // G, Z, 5, 0, Enter: exactly 50 mm forwards
        for (text, sym) in [("g", "g"), ("z", "z"), ("5", "5"), ("0", "0")] {
            cx.simulate_key(win.id, text, sym);
            cx.draw(win.id);
        }
        assert!(win.root().read(cx).piece_modal(), "G started a 3D move");
        cx.simulate_key(win.id, "", "Return");
        cx.draw(win.id);
        let moved = pos(cx);
        assert!(!win.root().read(cx).piece_modal());
        assert!(
            (moved[2] - start[2] - 50.0).abs() < 0.5 && (moved[0] - start[0]).abs() < 0.5,
            "moved {:?}",
            [moved[0] - start[0], moved[1] - start[1], moved[2] - start[2]]
        );
        assert!(
            win.root().read(cx).session().project.pieces[&front]
                .arr
                .moved
                .is_some(),
            "saved"
        );

        // R, sweep, Esc: the turn is undone
        cx.simulate_mouse_move(win.id, x, y);
        cx.simulate_key(win.id, "r", "r");
        cx.draw(win.id);
        cx.simulate_mouse_move(win.id, x + 80.0, y);
        cx.draw(win.id);
        let turned = pos(cx);
        let swing = (0..3).map(|k| (turned[k] - moved[k]).powi(2)).sum::<f32>().sqrt();
        assert!(swing > 5.0, "R turned the piece ({swing} mm)");
        cx.simulate_key(win.id, "", "Escape");
        cx.draw(win.id);
        let back = pos(cx);
        let off = (0..3).map(|k| (back[k] - moved[k]).powi(2)).sum::<f32>().sqrt();
        assert!(off < 0.5, "Esc puts it back ({off} mm off)");
    });
}

#[test]
fn rendered_shading_path_traces_the_viewport() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        win.root().update(cx, |app, cx| {
            app.set_shading(azam_app::operators::Shading::Rendered);
            cx.notify();
        });
        let mut samples = 0;
        for _ in 0..400 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
            samples = win.root().read(cx).live_render_samples().unwrap_or(0);
            if samples >= 4 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            samples >= 4,
            "the viewport refines a path trace ({samples} samples)"
        );
        // switching back drops it
        win.root().update(cx, |app, cx| {
            app.set_shading(azam_app::operators::Shading::Solid);
            cx.notify();
        });
        cx.draw(win.id);
        assert_eq!(win.root().read(cx).live_render_samples(), None);
    });
}

#[test]
fn a_draped_piece_flattens_into_a_new_piece() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        win.root().update(cx, |app, _| app.set_playing(true));
        for _ in 0..90 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        win.root().update(cx, |app, _| app.set_playing(false));
        let front = front_id(cx, &win);
        let before = win.root().read(cx).session().project.pieces.len();
        win.root().update(cx, |app, cx| {
            app.select_piece(front.clone());
            app.flatten_active_piece();
            cx.notify();
        });
        let app = win.root().read(cx);
        assert_eq!(app.session().project.pieces.len(), before + 1, "{}", app.status());
        let flat = app
            .session()
            .project
            .pieces
            .values()
            .find(|p| p.name == "Front (flattened)")
            .expect("the flattened front");
        let area = |pc: &azam_doc::Piece| azam_geom::signed_area(&pc.outline().poly).abs();
        let ratio = area(flat) / area(&app.session().project.pieces[&front]);
        assert!((0.8..1.25).contains(&ratio), "close to the draft's area: {ratio}");
    });
}

#[test]
fn a_recorded_simulation_exports_as_an_animated_gltf() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        win.root().update(cx, |app, _| {
            app.toggle_recording();
            app.set_playing(true);
        });
        for _ in 0..30 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        win.root().update(cx, |app, _| {
            app.set_playing(false);
            app.toggle_recording();
        });
        let frames = win.root().read(cx).recorded_frames().expect("a recording");
        assert!(frames >= 20, "{frames} frames recorded");
        // stopped: playing on adds nothing
        win.root().update(cx, |app, _| app.set_playing(true));
        for _ in 0..5 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        win.root().update(cx, |app, _| app.set_playing(false));
        assert_eq!(win.root().read(cx).recorded_frames(), Some(frames));
        let path = std::env::temp_dir().join(format!("azam-anim-{}.glb", std::process::id()));
        win.root()
            .read(cx)
            .export_to(azam_app::export::ExportKind::Animation, &path)
            .expect("exported");
        let glb = std::fs::read(&path).expect("written");
        let _ = std::fs::remove_file(&path);
        let json_len = u32::from_le_bytes(glb[12..16].try_into().expect("4 bytes")) as usize;
        let doc: serde_json::Value = serde_json::from_slice(&glb[20..20 + json_len]).expect("JSON chunk");
        assert_eq!(
            doc["meshes"][0]["primitives"][0]["targets"]
                .as_array()
                .map(Vec::len),
            Some(frames.div_ceil(2) - 1),
            "a morph target per later frame, sampled at 30 fps"
        );
        assert!(
            !doc["animations"][0]["channels"]
                .as_array()
                .expect("channels")
                .is_empty()
        );
    });
}

#[test]
fn an_auto_save_recovers_the_project() {
    let dir = std::env::temp_dir().join(format!("azam-autosave-{}", std::process::id()));
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        win.root().update(cx, |app, _| {
            app.use_prefs_file(dir.join("prefs.json"));
            app.edit_project("Rename", |p| "Saved Shirt".clone_into(&mut p.meta.name));
        });
        let path = win.root().read(cx).autosave_path().expect("an auto save place");
        win.root().update(cx, |app, _| {
            assert!(app.write_autosave_now(), "written");
            app.edit_project("Rename", |p| "Lost Changes".clone_into(&mut p.meta.name));
            app.recover_autosave();
        });
        let app = win.root().read(cx);
        assert_eq!(app.session().project.meta.name, "Saved Shirt", "{}", app.status());
        assert!(app.session().path.is_none(), "recovered as an unsaved project");
        assert!(path.exists());
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ctrl_space_maximizes_the_area_under_the_pointer() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.draw(win.id);
        let pat = cx.element_bounds(win.id, PATTERN_ID).expect("pattern editor");
        cx.simulate_mouse_move(win.id, pat.origin.x + 200.0, pat.origin.y + 200.0);
        cx.simulate_key_with(win.id, " ", "space", true, false);
        cx.draw(win.id);
        cx.draw(win.id);
        assert!(
            win.root().read(cx).maximized_area().is_some(),
            "{}",
            win.root().read(cx).status()
        );
        assert!(
            cx.element_bounds(win.id, VIEWPORT_ID).is_none(),
            "the 3D view is hidden"
        );
        let wide = cx.element_bounds(win.id, PATTERN_ID).expect("pattern editor");
        assert!(
            wide.size.width > pat.size.width * 1.5,
            "the pattern editor fills the window"
        );
        cx.simulate_key_with(win.id, " ", "space", true, false);
        cx.draw(win.id);
        cx.draw(win.id);
        assert_eq!(win.root().read(cx).maximized_area(), None);
        assert!(
            cx.element_bounds(win.id, VIEWPORT_ID).is_some(),
            "the areas are back"
        );
    });
}

#[test]
fn the_search_says_when_nothing_matches_and_lists_recent_commands_first() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        cx.simulate_mouse_move(win.id, 700.0, 400.0);
        cx.draw(win.id);
        // a command run from the keyboard becomes recent
        cx.simulate_key(win.id, "", "Home");
        cx.draw(win.id);
        cx.simulate_key(win.id, "", "F3");
        cx.draw(win.id);
        cx.draw(win.id);
        let first = cx.element_bounds(win.id, "op-Frame All").map(|b| b.origin.y);
        let other = cx.element_bounds(win.id, "op-Open…").map(|b| b.origin.y);
        assert!(first.is_some(), "Frame All is listed");
        assert!(
            other.is_none_or(|o| first.is_some_and(|f| f < o)),
            "the recent command comes first"
        );
        for c in "zzqx".chars() {
            let s = c.to_string();
            cx.simulate_key(win.id, &s, &s);
        }
        cx.draw(win.id);
        cx.draw(win.id);
        assert!(
            cx.element_bounds(win.id, "op-Frame All").is_none(),
            "no results for nonsense"
        );
    });
}

#[test]
fn layouts_are_saved_and_come_back() {
    use azam_app::areas::Dir;
    use azam_app::editors::Editor;
    let dir = std::env::temp_dir().join(format!("azam-layouts-{}", std::process::id()));
    let prefs = dir.join("prefs.json");
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        let before = win.root().read(cx).layout().leaves().len();
        win.root().update(cx, |app, cx| {
            app.use_prefs_file(prefs.clone());
            let props = app.area_with(Editor::Properties).expect("properties");
            app.split_area(props, Dir::Column).expect("split");
            cx.notify();
        });
        assert_eq!(win.root().read(cx).layout().leaves().len(), before + 1);
        // a new window on the same preferences opens as it was left
        let again = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(again.id);
        again
            .root()
            .update(cx, |app, _| app.use_prefs_file(prefs.clone()));
        assert_eq!(
            again.root().read(cx).layout().leaves().len(),
            before + 1,
            "the split came back"
        );
        again.root().update(cx, |app, _| app.reset_layout());
        assert_eq!(
            again.root().read(cx).layout().leaves().len(),
            before,
            "Reset Layout"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn workspaces_can_be_added_duplicated_renamed_moved_and_deleted() {
    use azam_app::app::Workspace;
    use azam_app::areas::Dir;
    use azam_app::editors::Editor;
    let dir = std::env::temp_dir().join(format!("azam-workspaces-{}", std::process::id()));
    let prefs = dir.join("prefs.json");
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        let names =
            |app: &AzamApp| -> Vec<String> { app.workspace_tabs().iter().map(|t| t.name.clone()).collect() };
        win.root().update(cx, |app, cx| {
            app.use_prefs_file(prefs.clone());
            assert_eq!(
                names(app),
                ["Design", "Grading", "Colorway", "Review", "Tech Pack"]
            );
            // split Design's properties, then duplicate it: the copy has the split
            let props = app.area_with(Editor::Properties).expect("properties");
            app.split_area(props, Dir::Column).expect("split");
            let areas = app.layout().leaves().len();
            app.duplicate_workspace();
            assert_eq!(app.active_tab(), 1);
            assert_eq!(names(app)[1], "Design.001");
            assert_eq!(app.layout().leaves().len(), areas, "the copy keeps the layout");
            assert_eq!(app.workspace(), Workspace::Design);
            // rename it; a taken name is refused
            assert!(!app.rename_workspace("Review"));
            assert!(app.rename_workspace("Fitting"));
            assert_eq!(names(app)[1], "Fitting");
            assert_eq!(app.layout().leaves().len(), areas, "the layout follows the name");
            // move it to the end, add a Review tab after it, delete that
            app.move_workspace(10);
            assert_eq!(names(app).last().map(String::as_str), Some("Fitting"));
            app.add_workspace(Workspace::Review);
            assert_eq!(names(app).last().map(String::as_str), Some("Review.001"));
            assert_eq!(app.workspace(), Workspace::Review);
            assert!(app.delete_workspace());
            assert_eq!(names(app).last().map(String::as_str), Some("Fitting"));
            assert_eq!(
                app.active_tab(),
                names(app).len() - 1,
                "the tab before it is active"
            );
            cx.notify();
        });
        // a new window on the same preferences has the same tabs and layouts
        let again = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(again.id);
        again.root().update(cx, |app, _| {
            app.use_prefs_file(prefs.clone());
            assert_eq!(
                names(app),
                ["Design", "Grading", "Colorway", "Review", "Tech Pack", "Fitting"]
            );
            let fitting = app.workspace_tabs().len() - 1;
            app.activate_tab(fitting);
            let design_areas = {
                let n = app.layout().leaves().len();
                app.activate_tab(0);
                assert_eq!(app.layout().leaves().len(), n, "Design kept its split too");
                n
            };
            app.activate_tab(fitting);
            assert_eq!(app.layout().leaves().len(), design_areas);
            // the last tab cannot go
            while app.workspace_tabs().len() > 1 {
                assert!(app.delete_workspace());
            }
            assert!(!app.delete_workspace());
        });
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_marker_follows_its_settings_and_exports() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        cx.draw(win.id);
        win.root().update(cx, |app, _| {
            let one = app.marker();
            let pieces = app.session().project.pieces.len();
            assert!(one.placements.len() >= pieces, "every piece of the size shown");
            assert!(one.efficiency() > 0.5);
            let sizes = app.session().project.sizes.len();
            app.edit_project("Marker Sizes", |p| {
                p.extra.insert(
                    "marker".into(),
                    serde_json::json!({"allSizes": true, "width": 1200.0}),
                );
            });
            let all = app.marker();
            assert_eq!(
                all.placements.len(),
                one.placements.len() * sizes,
                "one garment of each size"
            );
            assert!((all.width_mm - 1200.0).abs() < 1e-9);
            assert!(all.length_mm > one.length_mm * 2.0);
            let path = std::env::temp_dir().join(format!("azam-marker-{}.svg", std::process::id()));
            app.export_to(azam_app::export::ExportKind::MarkerSvg, &path)
                .expect("exported");
            let svg = std::fs::read_to_string(&path).expect("written");
            let _ = std::fs::remove_file(&path);
            assert_eq!(svg.matches("<polygon").count(), all.placements.len());
        });
    });
}

#[test]
fn a_marker_piece_can_be_dragged_and_clashes_show() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        win.root()
            .update(cx, |app, _| app.set_workspace(azam_app::app::Workspace::TechPack));
        cx.draw(win.id);
        win.root().update(cx, |app, _| {
            let m = app.marker();
            let centre = |pts: &[azam_geom::Vec2]| {
                pts.iter().fold(azam_geom::Vec2::ZERO, |s, &q| s + q) * (1.0 / pts.len() as f64)
            };
            let (a, b) = (centre(&m.placements[0].outline), centre(&m.placements[1].outline));
            let from = app.marker_to_window(b).expect("the editor drew the marker");
            let to = app.marker_to_window(a).expect("drawn");
            // drag the second piece onto the first
            app.marker_press(from.0, from.1);
            assert!(app.marker_drag_to(to.0, to.1), "dragging");
            app.marker_release();
            let moved = app.marker();
            let clashes = moved.clashes();
            assert!(clashes[0] && clashes[1], "the two overlap now");
            let saved = azam_app::marker_view::MarkerSettings::of(&app.session().project).moves;
            assert_eq!(saved.len(), 1, "the move is kept with the project");
            // Re-nest by clearing the moves brings the clean nest back
            app.edit_project("Re-nest Marker", |p| {
                p.extra.insert("marker".into(), serde_json::json!({"moves": {}}));
            });
            assert!(app.marker().clashes().iter().all(|c| !c));
        });
    });
}

#[test]
fn new_colorway_copies_the_active_one_and_makes_it_active() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        win.root().update(cx, |app, _| {
            let before = app.session().project.colorways.len();
            let old = app.session().project.active_colorway.clone();
            app.new_colorway();
            let p = &app.session().project;
            assert_eq!(p.colorways.len(), before + 1);
            assert_ne!(p.active_colorway, old, "the copy is active");
        });
    });
}

#[test]
fn the_colorways_editor_shows_every_colorway_and_picks_one() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        wait_for_garment(cx, &win);
        win.root().update(cx, |app, cx| {
            app.open_template(azam_app::operators::Template::Labadeh);
            app.set_workspace(azam_app::app::Workspace::Colorway);
            cx.notify();
        });
        wait_for_garment(cx, &win);
        for _ in 0..3 {
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        let ids: Vec<String> = win
            .root()
            .read(cx)
            .session()
            .project
            .colorways
            .keys()
            .map(|k| k.as_str().to_owned())
            .collect();
        assert!(ids.len() > 1, "the labadeh has several colorways");
        for id in &ids {
            assert!(
                cx.element_bounds(win.id, &format!("cw-{id}")).is_some(),
                "a card for {id}"
            );
        }
        let other = ids
            .iter()
            .find(|i| **i != win.root().read(cx).session().project.active_colorway.as_str())
            .expect("another")
            .clone();
        let b = cx.element_bounds(win.id, &format!("cw-{other}")).expect("card");
        cx.simulate_click(win.id, b.origin.x + b.size.width / 2.0, b.origin.y + 40.0);
        cx.draw(win.id);
        assert_eq!(
            win.root().read(cx).session().project.active_colorway.as_str(),
            other
        );
    });
}
