//! Drives the Render window headless: F12 opens it in place of the editors and starts rendering,
//! Escape closes it.

use azam_app::AzamApp;
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
fn f12_opens_the_render_window_and_escape_closes_it() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        for _ in 0..3 {
            cx.draw(win.id);
        }
        // Wait for the avatar (built on a worker thread) and the garment.
        for _ in 0..4000 {
            if win.root().read(cx).sim_time().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        assert!(win.root().read(cx).sim_time().is_some(), "the garment exists");
        assert!(win.root().read(cx).render_progress().is_none());

        // Click the status bar so the window has keyboard focus.
        cx.simulate_click(win.id, 700.0, 850.0);
        cx.draw(win.id);
        cx.simulate_key(win.id, "", "F12");
        cx.draw(win.id);
        let progress = win
            .root()
            .read(cx)
            .render_progress()
            .expect("F12 opens the render window");
        assert!(
            progress.starts_with("Preparing")
                || progress.starts_with("Sample")
                || progress.starts_with("Done"),
            "{progress}"
        );
        // The Render Result takes the place of the editor areas.
        win.root().update(cx, |_, cx| cx.notify());
        cx.draw(win.id);
        assert!(
            cx.element_bounds(win.id, "viewport3d").is_none(),
            "the editors make way"
        );
        let image = cx.element_bounds(win.id, "render-image").expect("render image");
        assert!(image.size.width > 600.0 && image.size.height > 300.0, "{image:?}");
        // Samples arrive while the window keeps drawing.
        for _ in 0..2000 {
            let p = win.root().read(cx).render_progress().unwrap_or_default();
            if p.starts_with("Sample") || p.starts_with("Done") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            win.root().update(cx, |_, cx| cx.notify());
            cx.draw(win.id);
        }
        let progress = win.root().read(cx).render_progress().unwrap_or_default();
        assert!(
            progress.contains("/64") || progress.starts_with("Done"),
            "{progress}"
        );

        cx.simulate_key(win.id, "", "Escape");
        cx.draw(win.id);
        assert!(
            win.root().read(cx).render_progress().is_none(),
            "Escape closes it"
        );
    });
}
