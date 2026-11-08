//! `azam-mcp`: an MCP server (stdio) that lets an AI agent design garments in azam.
//!
//! It runs the azam app headless in this process: the agent drafts and edits pattern pieces with
//! CAD tools, sews them, drapes the garment on the avatar and looks at the result with
//! screenshots. `open_window` shows the same project live in an azam window.
//!
//! ```text
//! azam-mcp [--template dress] [--size 1480x900] [PROJECT.azam.json]
//! claude mcp add azam -- /path/to/azam-mcp
//! ```

mod describe;
mod image;
mod tools;

use std::io::{BufRead, Write};

use azam_app::AzamApp;
use blenderui::prelude::*;
use serde_json::{Value, json};

/// Protocol versions this server speaks, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What the agent is told about the server when it connects.
const INSTRUCTIONS: &str = "azam is a garment CAD app: 2D pattern pieces are sewn together and draped \
on a 3D avatar by a cloth simulation. All lengths are millimetres; pattern coordinates have x to the \
right and y down. A typical loop: new_project (the dress template is a good start) -> get_project / \
get_piece to read ids and coordinates -> edit with the CAD tools (move_points, curve_segment, \
add_dart, spread_piece, sew…) -> simulate -> screenshot (view 3d from several angles, or view \
pattern) -> repeat. Each tool call is one undo step. Segments are named by the id of their start \
corner. Seams join edges of equal length: check `mismatch` in get_project after reshaping. \
open_window shows the design live in an azam window for the person you are working with. Save \
with save_project.";

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
                     --template NAME  tee, dress (default), skirt, labadeh, aba or blank\n  \
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

/// A JSON-RPC response to request `id`.
#[allow(clippy::needless_pass_by_value, reason = "results are built in place")]
fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// A JSON-RPC error response.
fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Handles one JSON-RPC message; returns the response, if it needs one.
fn handle(server: &mut tools::Server, cx: &mut App, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str);
    let Some(id) = msg.get("id").cloned() else {
        // a notification (initialized, cancelled…): nothing to answer
        return None;
    };
    let Some(method) = method else {
        // a response to a request we never send
        return None;
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSIONS[0]);
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&PROTOCOL_VERSIONS[0]);
            reply(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "azam", "title": "azam garment CAD", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => reply(&id, json!({})),
        "tools/list" => reply(&id, json!({"tools": tools::list()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            let args = params["arguments"].as_object().cloned().unwrap_or_default();
            let started = std::time::Instant::now();
            let result = match server.call(cx, name, &args) {
                Ok(content) => json!({"content": content, "isError": false}),
                Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
            };
            eprintln!("azam-mcp: {name} took {:.2} s", started.elapsed().as_secs_f64());
            reply(&id, result)
        }
        "resources/list" => reply(&id, json!({"resources": []})),
        "prompts/list" => reply(&id, json!({"prompts": []})),
        other => error(&id, -32601, &format!("no method {other}")),
    })
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
        let mut server = tools::Server::new(window);
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let response = match serde_json::from_str::<Value>(&line) {
                Ok(Value::Array(batch)) => {
                    let out: Vec<Value> = batch.iter().filter_map(|m| handle(&mut server, cx, m)).collect();
                    (!out.is_empty()).then(|| Value::Array(out))
                }
                Ok(msg) => handle(&mut server, cx, &msg),
                Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}"))),
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
