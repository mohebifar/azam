//! Renders the main UI states to PNGs for design review: `cargo run --release -p azam-app
//! --example ui_tour -- <out-dir>`.

use azam_app::AzamApp;
use blenderui::prelude::*;

fn save(path: &std::path::Path, w: usize, h: usize, px: &[u32]) {
    let rgba: Vec<u8> = px
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255])
        .collect();
    let file = std::fs::File::create(path).expect("create");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png")
        .write_image_data(&rgba)
        .expect("png");
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "tour".into()));
    std::fs::create_dir_all(&out).expect("out dir");
    Application::headless().run(move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(1480.0), px(900.0)),
            })),
            ..Default::default()
        };
        let win = cx
            .open_window(options, |_, cx| cx.new(AzamApp::new))
            .expect("window");
        let shot = |cx: &mut App, name: &str| {
            for _ in 0..3 {
                win.root().update(cx, |_, cx| cx.notify());
                cx.draw(win.id);
            }
            let (w, h, px) = cx.draw(win.id);
            save(&out.join(format!("{name}.png")), w, h, &px);
            println!("{name}");
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        shot(cx, "01-start");
        cx.simulate_mouse_move(win.id, 598.0, 280.0);
        std::thread::sleep(std::time::Duration::from_millis(600));
        shot(cx, "01b-tooltip");
        // File menu
        if let Some(b) = cx.element_bounds(win.id, "menu-0") {
            cx.simulate_click(win.id, b.origin.x + 5.0, b.origin.y + 5.0);
        }
        shot(cx, "02-file-menu");
        if let Some(b) = cx.element_bounds(win.id, "submenu-New") {
            cx.simulate_mouse_move(win.id, b.origin.x + 20.0, b.origin.y + 5.0);
        }
        shot(cx, "02b-new-submenu");
        cx.simulate_key(win.id, "", "Escape");
        cx.simulate_mouse_move(win.id, 300.0, 400.0);
        cx.simulate_key(win.id, "", "F3");
        cx.simulate_key(win.id, "f", "f");
        cx.simulate_key(win.id, "i", "i");
        shot(cx, "03-search");
        cx.simulate_key(win.id, "", "Escape");
        cx.simulate_mouse_move(win.id, 300.0, 400.0);
        cx.simulate_key(win.id, "z", "z");
        shot(cx, "04-pie");
        cx.simulate_key(win.id, "", "Escape");
        win.root().update(cx, |app, _| {
            app.open_context_menu(
                azam_app::operators::ContextMenu::Piece,
                point(px(720.0), px(430.0)),
            );
        });
        shot(cx, "04b-context-menu");
        cx.simulate_key(win.id, "", "Escape");
        if let Some(b) = cx.element_bounds(win.id, "editor-type-1") {
            cx.simulate_click(win.id, b.origin.x + 8.0, b.origin.y + b.size.height / 2.0);
        }
        shot(cx, "04c-editor-type");
        cx.simulate_key(win.id, "", "Escape");
        // properties tabs down the side of the properties editor
        for (i, name) in [
            "05-tab-project",
            "06-tab-piece",
            "07-tab-item",
            "08-tab-fabric",
            "09-tab-avatar",
            "10-tab-sim",
        ]
        .iter()
        .enumerate()
        {
            cx.simulate_click(win.id, 1162.0, 325.0 + 28.0 * i as f32);
            shot(cx, name);
        }
        // the Dart tool's settings in the pattern editor's header
        cx.simulate_mouse_move(win.id, 860.0, 650.0);
        cx.simulate_key(win.id, "d", "d");
        shot(cx, "10b-dart-tool");
        // Adjust Last Operation after cutting a dart into the back's hem
        let hem = win
            .root()
            .read(cx)
            .session()
            .project
            .pieces
            .values()
            .find(|p| p.name == "Back")
            .map(|pc| {
                let ol = pc.outline();
                let longest = ol
                    .segments
                    .iter()
                    .max_by(|a, b| a.len.total_cmp(&b.len))
                    .expect("segments");
                longest.point_at(0.5).pos
            });
        if let (Some(q), Some(canvas)) = (hem, cx.element_bounds(win.id, azam_app::pattern::PATTERN_ID)) {
            let s = win.root().read(cx).pattern_view().to_screen(q);
            cx.simulate_click(win.id, canvas.origin.x + s.x as f32, canvas.origin.y + s.y as f32);
        }
        shot(cx, "10c-adjust-last-operation");
        cx.simulate_key_with(win.id, "z", "z", true, false);
        cx.simulate_key(win.id, "w", "w");
        // a logo printed on the front, draped, in the viewport and the path tracer
        let logo = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../third_party/bui/public/Logo.png"
        );
        if std::path::Path::new(logo).is_file() {
            win.root().update(cx, |app, _| {
                app.edit_project("Add Graphic", |p| {
                    let front = p.pieces.values().find(|pc| pc.name == "Front").map(|pc| {
                        let b = azam_geom::bbox(pc.pts.iter().map(|q| q.pos)).expect("outline");
                        (pc.id.clone(), b.center())
                    });
                    if let Some((id, c)) = front {
                        azam_doc::add_graphic(p, &id, c, azam_geom::Vec2::new(140.0, 140.0), Some(logo));
                    }
                });
                app.set_playing(true);
            });
            for _ in 0..240 {
                win.root().update(cx, |_, cx| cx.notify());
                cx.draw(win.id);
            }
            win.root().update(cx, |app, _| app.set_playing(false));
            shot(cx, "11-graphic-3d");
            // a zipper down a side seam of the front and back
            win.root().update(cx, |app, _| {
                app.edit_project("Add Zipper", |p| {
                    let sides: Vec<_> = p
                        .pieces
                        .values()
                        .filter(|pc| pc.name == "Front" || pc.name == "Back")
                        .map(|pc| {
                            let ol = pc.outline();
                            // the side seam: the most upright long segment
                            let side = ol
                                .segments
                                .iter()
                                .filter(|s| s.len > 150.0)
                                .max_by(|a, b| {
                                    let upright = |s: &azam_geom::Segment| {
                                        let d = s.ctrl[s.ctrl.len() - 1] - s.ctrl[0];
                                        d.y.abs() / d.length().max(1e-9)
                                    };
                                    upright(a).total_cmp(&upright(b))
                                })
                                .expect("segments");
                            (pc.id.clone(), side.id.clone())
                        })
                        .collect();
                    for (piece, seg) in sides {
                        azam_doc::add_zipper_half(p, &piece, &seg);
                    }
                });
            });
            shot(cx, "11c-zipper");
            win.root().update(cx, |app, _| app.set_show_arrangement(true));
            shot(cx, "12-arrangement-points");
            win.root().update(cx, |app, _| app.set_show_arrangement(false));
            win.root().update(cx, |app, _| {
                app.toggle_sidebar();
                app.toggle_tool_labels();
            });
            shot(cx, "13-sidebar-and-tool-names");
            let rendered = win.root().read(cx).render_still(640, 400, 32);
            if let Ok((image, _)) = rendered {
                let _ = image.write_png(&out.join("11b-graphic-render.png"));
            }
        }
    });
}
