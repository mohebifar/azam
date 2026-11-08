//! The `azam` binary: `azam [project.json]` opens the window (on that project);
//! `azam --screenshot out.png [--size WxH]` renders one
//! frame headless; `azam --render out.png [--frames N] [--render-size WxH] [--samples N]` drapes
//! for `N` frames and path-traces the 3D view (Blender's F12) headless. `azam --help` lists the
//! options.

use azam_app::AzamApp;
use blenderui::prelude::*;

/// Default window size, in logical pixels.
const WINDOW_SIZE: (f32, f32) = (1480.0, 900.0);

/// Command-line options.
#[derive(Debug)]
struct Args {
    screenshot: Option<String>,
    render: Option<String>,
    render_size: (u32, u32),
    samples: u32,
    workspace: Option<String>,
    template: Option<String>,
    shading: Option<azam_app::operators::Shading>,
    /// A project file to open (the first argument that is not an option or its value).
    open: Option<String>,
    /// A live session to join (`host:port` or a share code).
    join: Option<String>,
    /// Port of the MCP server.
    mcp_port: Option<u16>,
    /// Start the MCP server on opening.
    mcp: bool,
    frames: usize,
    size: (f32, f32),
}

/// What `azam --help` prints.
const USAGE: &str = "azam: garment design with Blender's interface

Usage:
  azam [PROJECT.azam.json]                 open the window (on a saved project)
  azam --screenshot OUT.png [options]      render one frame of the window headless
  azam --render OUT.png [options]          path-trace the 3D view headless (Blender's F12)

Options:
  --workspace NAME     design, grading, colorway, review or techpack
  --template NAME      tee, dress, skirt, labadeh, aba or blank
  --frames N           simulate for N redraws first (300 is about 10 s of simulation)
  --shading NAME       3D view shading: solid, fitmap or rendered
  --size WxH           window size for --screenshot (default 1480x900)
  --render-size WxH    image size for --render (default 1280x800)
  --samples N          samples per pixel for --render
  --join TARGET        join a live session (host:port or a share code) on opening
  --mcp                start the MCP server (HTTP) on opening
  --mcp-port N         its port (default 7465, or AZAM_MCP_PORT)
  -h, --help           show this help
";

/// Options that take a value.
const WITH_VALUE: [&str; 11] = [
    "--join",
    "--mcp-port",
    "--shading",
    "--screenshot",
    "--render",
    "--render-size",
    "--samples",
    "--workspace",
    "--template",
    "--frames",
    "--size",
];

/// Parses `WxH`.
fn parse_size<T: std::str::FromStr>(flag: &str, s: &str) -> Result<(T, T), String> {
    s.split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .ok_or_else(|| format!("{flag} takes WIDTHxHEIGHT, not {s:?}"))
}

/// Parses `value` of `flag` as a number.
fn parse_num<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("{flag} takes a number, not {value:?}"))
}

/// The options in `args` (without the program name); `Ok(None)` when help was asked for.
fn parse_args(args: &[String]) -> Result<Option<Args>, String> {
    let mut a = Args {
        screenshot: None,
        render: None,
        render_size: (1280, 800),
        samples: azam_app::render_view::RENDER_SAMPLES,
        workspace: None,
        template: None,
        shading: None,
        open: None,
        join: None,
        mcp_port: None,
        mcp: false,
        frames: 0,
        size: WINDOW_SIZE,
    };
    let mut k = 0;
    while k < args.len() {
        let flag = args[k].as_str();
        if flag == "-h" || flag == "--help" {
            return Ok(None);
        }
        if WITH_VALUE.contains(&flag) {
            let value = args
                .get(k + 1)
                .ok_or_else(|| format!("{flag} needs a value"))?
                .clone();
            match flag {
                "--screenshot" => a.screenshot = Some(value),
                "--render" => a.render = Some(value),
                "--join" => a.join = Some(value),
                "--mcp-port" => a.mcp_port = Some(parse_num(flag, &value)?),
                "--render-size" => a.render_size = parse_size(flag, &value)?,
                "--samples" => a.samples = parse_num(flag, &value)?,
                "--workspace" => {
                    if azam_app::app::Workspace::from_name(&value).is_none() {
                        return Err(format!(
                            "no workspace {value:?} (design, grading, colorway, review, techpack)"
                        ));
                    }
                    a.workspace = Some(value);
                }
                "--template" => {
                    if azam_app::operators::Template::from_key(&value).is_none() {
                        return Err(format!(
                            "no template {value:?} (tee, dress, skirt, labadeh, aba, blank)"
                        ));
                    }
                    a.template = Some(value);
                }
                "--frames" => a.frames = parse_num(flag, &value)?,
                "--shading" => {
                    use azam_app::operators::Shading;
                    a.shading = Some(match value.as_str() {
                        "solid" => Shading::Solid,
                        "fitmap" | "fit" => Shading::FitMap,
                        "rendered" => Shading::Rendered,
                        _ => return Err(format!("no shading {value:?} (solid, fitmap, rendered)")),
                    });
                }
                _ => a.size = parse_size(flag, &value)?,
            }
            k += 2;
            continue;
        }
        if flag == "--mcp" {
            a.mcp = true;
            k += 1;
            continue;
        }
        if flag.starts_with("-psn_") {
            // the process serial number older macOS passes to apps opened from the Finder
            k += 1;
            continue;
        }
        if flag.starts_with('-') {
            return Err(format!("unknown option {flag}"));
        }
        if a.open.is_some() {
            return Err(format!("one project at a time ({flag} is a second)"));
        }
        a.open = Some(flag.to_owned());
        k += 1;
    }
    Ok(Some(a))
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

/// The command line's options; prints the usage and exits for `--help` or a mistake.
fn args_or_exit() -> Args {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&raw) {
        Ok(Some(args)) => args,
        Ok(None) => {
            print!("{USAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("azam: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}

/// Switches the window's 3D view to `shading`; for Rendered shading, lets its path trace refine.
fn apply_shading(window: &WindowHandle<AzamApp>, cx: &mut App, shading: azam_app::operators::Shading) {
    window.root().update(cx, |app, _| app.set_shading(shading));
    if shading == azam_app::operators::Shading::Rendered {
        for _ in 0..3000 {
            window.root().update(cx, |_, cx| cx.notify());
            cx.draw(window.id);
            if window.root().read(cx).live_render_samples().unwrap_or(0) >= 48 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// Path-traces `app`'s 3D view at `size` with `samples` per pixel and writes it to `path`.
fn write_render(app: &AzamApp, path: &str, size: (u32, u32), samples: u32) {
    let (w, h) = size;
    match app.render_still(w, h, samples) {
        Ok((image, stats)) => {
            println!(
                "rendered {w}x{h} @ {samples} spp, {} triangles: trace {:.2} s, denoise + tone map {:.2} s",
                stats.triangles, stats.trace_s, stats.develop_s
            );
            match image.write_png(std::path::Path::new(path)) {
                Ok(()) => println!("wrote {path}"),
                Err(e) => eprintln!("azam: could not write {path}: {e}"),
            }
        }
        Err(e) => eprintln!("azam: could not render: {e}"),
    }
}

/// Opens the window's links to the outside: the MCP server (started now only with `--mcp`
/// or when Preferences ask for it), and the live session `--join` names.
fn connect(window: &WindowHandle<AzamApp>, cx: &mut App, args: &Args) {
    let port = args
        .mcp_port
        .or_else(|| std::env::var("AZAM_MCP_PORT").ok()?.parse().ok())
        .unwrap_or(azam_app::mcp::http::DEFAULT_PORT);
    azam_app::mcp::http::install(window.clone());
    let start = args.mcp || window.root().read(cx).mcp_autostart();
    window.root().update(cx, |app, cx| {
        app.set_mcp_port(port);
        if start {
            app.start_mcp_server(cx);
        }
    });
    if let Some(target) = args.join.clone() {
        window.root().update(cx, |app, cx| {
            if let Err(e) = app.join_session(&target, cx) {
                eprintln!("azam: could not join {target}: {e}");
            }
        });
    }
}

fn main() {
    let args = args_or_exit();
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
        let prefs = if headless {
            None
        } else {
            azam_app::prefs::prefs_path()
        };
        window.root().update(cx, |app, _| {
            if let Some(path) = prefs {
                app.use_prefs_file(path);
            }
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
        if !headless {
            connect(&window, cx, &args);
        }
        if headless {
            // The first frame lays out the areas; the second renders the viewport at its size.
            for _ in 0..3 {
                cx.draw(window.id);
            }
            {
                // The avatar is built on a worker thread: wait for the garment to exist, so a
                // screenshot shows the body and garment rather than "Building avatar…".
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
            if let Some(shading) = args.shading {
                apply_shading(&window, cx, shading);
            }
            if let Some(path) = &args.screenshot {
                let (w, h, pixels) = cx.draw(window.id);
                match write_png(path, w, h, &pixels) {
                    Ok(()) => println!("wrote {path} ({w}x{h})"),
                    Err(e) => eprintln!("azam: could not write {path}: {e}"),
                }
            }
            if let Some(path) = &args.render {
                write_render(window.root().read(cx), path, args.render_size, args.samples);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Option<Args>, String> {
        parse_args(&line.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
    }

    #[test]
    fn options_parse_and_mistakes_are_reported() {
        let a = parse("--screenshot out.png --workspace grading --frames 30 shirt.azam.json")
            .expect("valid")
            .expect("not help");
        assert_eq!(a.screenshot.as_deref(), Some("out.png"));
        assert_eq!(a.workspace.as_deref(), Some("grading"));
        assert_eq!(a.frames, 30);
        assert_eq!(a.open.as_deref(), Some("shirt.azam.json"));
        assert_eq!(
            parse("--join 127.0.0.1:4000")
                .expect("valid")
                .expect("not help")
                .join
                .as_deref(),
            Some("127.0.0.1:4000")
        );
        assert!(matches!(parse("--help"), Ok(None)));
        assert!(matches!(parse("-h"), Ok(None)));
        assert!(parse("--bogus").unwrap_err().contains("unknown option"));
        assert!(parse("--template nope").unwrap_err().contains("no template"));
        assert!(parse("--frames many").unwrap_err().contains("number"));
        assert!(parse("--size 10").unwrap_err().contains("WIDTHxHEIGHT"));
        assert!(parse("--screenshot").unwrap_err().contains("needs a value"));
        assert!(
            parse("-psn_0_12345")
                .expect("ignored")
                .expect("opens")
                .open
                .is_none()
        );
    }
}
