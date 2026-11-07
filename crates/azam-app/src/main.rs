//! The `azam` binary: `azam [project.json]` opens the window (on that project);
//! `azam --screenshot out.png [--size WxH]` renders one
//! frame headless; `azam --render out.png [--frames N] [--render-size WxH] [--samples N]` drapes
//! for `N` frames and path-traces the 3D view (Blender's F12) headless.

use azam_app::AzamApp;
use blenderui::prelude::*;

/// Default window size, in logical pixels.
const WINDOW_SIZE: (f32, f32) = (1480.0, 900.0);

/// Command-line options.
struct Args {
    screenshot: Option<String>,
    render: Option<String>,
    render_size: (u32, u32),
    samples: u32,
    workspace: Option<String>,
    template: Option<String>,
    /// A project file to open (the first argument that is not an option or its value).
    open: Option<String>,
    frames: usize,
    size: (f32, f32),
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let size = value("--size")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(WINDOW_SIZE);
    let render_size = value("--render-size")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1280, 800));
    Args {
        screenshot: value("--screenshot"),
        render: value("--render"),
        render_size,
        samples: value("--samples")
            .and_then(|f| f.parse().ok())
            .unwrap_or(azam_app::render_view::RENDER_SAMPLES),
        workspace: value("--workspace"),
        template: value("--template"),
        open: {
            const WITH_VALUE: [&str; 8] = [
                "--screenshot",
                "--render",
                "--render-size",
                "--samples",
                "--workspace",
                "--template",
                "--frames",
                "--size",
            ];
            let mut open = None;
            let mut k = 1;
            while k < args.len() {
                if WITH_VALUE.contains(&args[k].as_str()) {
                    k += 2;
                    continue;
                }
                if !args[k].starts_with("--") {
                    open = Some(args[k].clone());
                    break;
                }
                k += 1;
            }
            open
        },
        frames: value("--frames").and_then(|f| f.parse().ok()).unwrap_or(0),
        size,
    }
}

fn write_png(path: &str, width: usize, height: usize, pixels: &[u32]) -> std::io::Result<()> {
    let rgba: Vec<u8> = pixels
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255])
        .collect();
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .map_err(std::io::Error::other)
}

fn main() {
    let args = parse_args();
    let headless = args.screenshot.is_some() || args.render.is_some();
    let application = if headless {
        Application::headless()
    } else {
        Application::new()
    };
    application.run(move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(args.size.0), px(args.size.1)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("azam".into()),
            }),
            window_min_size: Some(size(px(900.0), px(560.0))),
            ..Default::default()
        };
        let window = match cx.open_window(options, |_, cx| cx.new(AzamApp::new)) {
            Ok(window) => window,
            Err(e) => {
                eprintln!("azam: could not open a window: {e}");
                return;
            }
        };
        let (workspace, template) = (
            args.workspace
                .as_deref()
                .and_then(azam_app::app::Workspace::from_name),
            args.template
                .as_deref()
                .and_then(azam_app::operators::Template::from_key),
        );
        let open = args.open.clone();
        window.root().update(cx, |app, _| {
            if let Some(t) = template {
                app.open_template(t);
            }
            if let Some(path) = &open
                && let Err(e) = app.open_file(std::path::Path::new(path))
            {
                eprintln!("azam: could not open {path}: {e}");
            }
            if let Some(w) = workspace {
                app.set_workspace(w);
            }
        });
        if headless {
            // The first frame lays out the areas; the second renders the viewport at its size.
            for _ in 0..3 {
                cx.draw(window.id);
            }
            if args.render.is_some() {
                // The avatar is built on a worker thread: wait for the garment to exist.
                for _ in 0..2000 {
                    if window.root().read(cx).sim_time().is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    window.root().update(cx, |_, cx| cx.notify());
                    cx.draw(window.id);
                }
            }
            if args.frames > 0 {
                window.root().update(cx, |app, _| app.set_playing(true));
                for _ in 0..args.frames {
                    window.root().update(cx, |_, cx| cx.notify());
                    cx.draw(window.id);
                }
                window.root().update(cx, |app, _| app.set_playing(false));
            }
            if let Some(path) = &args.screenshot {
                let (w, h, pixels) = cx.draw(window.id);
                match write_png(path, w, h, &pixels) {
                    Ok(()) => println!("wrote {path} ({w}x{h})"),
                    Err(e) => eprintln!("azam: could not write {path}: {e}"),
                }
            }
            if let Some(path) = &args.render {
                let (w, h) = args.render_size;
                let app = window.root().read(cx);
                match app.render_still(w, h, args.samples) {
                    Ok((image, stats)) => {
                        println!(
                            "rendered {w}x{h} @ {} spp, {} triangles: trace {:.2} s, denoise + tone map {:.2} s",
                            args.samples, stats.triangles, stats.trace_s, stats.develop_s
                        );
                        match image.write_png(std::path::Path::new(path)) {
                            Ok(()) => println!("wrote {path}"),
                            Err(e) => eprintln!("azam: could not write {path}: {e}"),
                        }
                    }
                    Err(e) => eprintln!("azam: could not render: {e}"),
                }
            }
        }
    });
}
