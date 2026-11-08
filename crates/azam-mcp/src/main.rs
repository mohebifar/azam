//! `azam-mcp`: azam's MCP tools over stdio, on a headless window of its own.
//!
//! The app itself serves the same tools over HTTP on the window the person has open
//! (`azam_app::mcp::http`); this binary is for when no window is: scripts, CI, or an agent working
//! alone. `open_window` shows its project live in an azam window.
//!
//! ```text
//! azam-mcp [--template dress] [--size 1480x900] [PROJECT.azam.json]
//! claude mcp add azam -- /path/to/azam-mcp
//! ```

use std::io::{BufRead, Write};

use azam_app::AzamApp;
use azam_app::mcp;
use blenderui::prelude::*;
use serde_json::Value;

/// Command-line options.
struct Options {
    template: Option<String>,
    open: Option<String>,
    size: (f32, f32),
}

/// The options in the command line; exits on `--help` or a mistake.
fn options() -> Options {
    let mut o = Options {
        template: Some("dress".into()),
        open: None,
        size: (1480.0, 900.0),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                eprintln!(
                    "azam-mcp: an MCP server (stdio) for designing garments in azam\n\n\
                     Usage: azam-mcp [--template NAME] [--size WxH] [PROJECT.azam.json]\n\n  \
                     --template NAME  tee, dress (default), skirt or blank\n  \
                     --size WxH       window size for view=window screenshots (default 1480x900)\n\n\
                     Register it with Claude Code: claude mcp add azam -- {}",
                    std::env::current_exe().map_or_else(|_| "azam-mcp".into(), |p| p.display().to_string())
                );
                std::process::exit(0);
            }
            "--template" => o.template = args.next(),
            "--size" => {
                let parsed = args.next().and_then(|s| {
                    let (w, h) = s.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                });
                let Some(size) = parsed else {
                    eprintln!("azam-mcp: --size takes WIDTHxHEIGHT");
                    std::process::exit(2);
                };
                o.size = size;
            }
            path if !path.starts_with('-') => o.open = Some(path.to_owned()),
            other => {
                eprintln!("azam-mcp: unknown option {other}");
                std::process::exit(2);
            }
        }
    }
    o
}

fn main() {
    let options = options();
    Application::headless().run(move |cx| {
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(options.size.0), px(options.size.1)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("azam".into()),
            }),
            ..Default::default()
        };
        let window = match cx.open_window(window_options, |_, cx| cx.new(AzamApp::new)) {
            Ok(window) => window,
            Err(e) => {
                eprintln!("azam-mcp: could not open a window: {e}");
                return;
            }
        };
        window.root().update(cx, |app, _| {
            if let Some(t) = options
                .template
                .as_deref()
                .and_then(azam_app::operators::Template::from_key)
            {
                app.open_template(t);
            }
            if let Some(path) = &options.open
                && let Err(e) = app.open_file(std::path::Path::new(path))
            {
                eprintln!("azam-mcp: could not open {path}: {e}");
            }
        });
        // lay the window out once, so the first screenshot is quick
        cx.draw(window.id);
        let mut server = mcp::Server::new(window, false);
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let mut call = |name: &str, args: &serde_json::Map<String, Value>| {
                let started = std::time::Instant::now();
                let out = server.call(cx, name, args);
                eprintln!("azam-mcp: {name} took {:.2} s", started.elapsed().as_secs_f64());
                out
            };
            let response = match serde_json::from_str::<Value>(&line) {
                Ok(msg) => mcp::respond_any(&msg, false, &mut call),
                Err(e) => Some(mcp::error(&Value::Null, -32700, &format!("parse error: {e}"))),
            };
            if let Some(response) = response {
                let text = serde_json::to_string(&response).unwrap_or_default();
                if writeln!(stdout, "{text}").and_then(|()| stdout.flush()).is_err() {
                    break;
                }
            }
        }
    });
}
