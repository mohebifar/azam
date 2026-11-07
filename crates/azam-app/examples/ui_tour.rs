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
    });
}
