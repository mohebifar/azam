//! The MCP server runs only on demand: started from the 3D view's sidebar, it answers MCP
//! over HTTP and edits the window's project; stopped, it lets go of its port.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use azam_app::AzamApp;
use azam_app::mcp::http;
use blenderui::prelude::*;
use serde_json::{Value, json};

/// A free loopback port.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

/// POSTs `body` to the endpoint on `port`; the response's status and JSON body.
fn post(port: u16, body: &Value) -> std::io::Result<(u16, Value)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let text = body.to_string();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let json = raw
        .split_once("\r\n\r\n")
        .and_then(|(_, b)| serde_json::from_str(b).ok())
        .unwrap_or(Value::Null);
    Ok((status, json))
}

/// Runs `f` on a thread while the window keeps drawing (which runs the tool calls it queues).
fn while_drawing<R: Send + 'static>(
    cx: &mut App,
    win: WindowId,
    f: impl FnOnce() -> R + Send + 'static,
) -> R {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    let started = Instant::now();
    loop {
        cx.draw(win);
        if let Ok(r) = rx.try_recv() {
            return r;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the server did not answer"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_server_starts_from_the_sidebar_serves_tools_and_stops() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(0.0), px(0.0)),
                        size: size(px(1400.0), px(860.0)),
                    })),
                    ..Default::default()
                },
                |_, cx| cx.new(AzamApp::new),
            )
            .expect("window");
        let port = free_port();
        http::install(win.clone());
        win.root().update(cx, |app, _| {
            app.set_mcp_port(port);
            app.toggle_sidebar();
        });
        for _ in 0..3 {
            cx.draw(win.id);
        }
        assert!(!http::is_running(), "off until asked for");
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err(), "nothing listens yet");

        // the sidebar's Start Server button
        let b = cx
            .element_bounds(win.id, "mcp-server")
            .expect("the MCP panel's button");
        cx.simulate_click(
            win.id,
            b.origin.x + b.size.width / 2.0,
            b.origin.y + b.size.height / 2.0,
        );
        cx.draw(win.id);
        assert!(http::is_running(), "the button starts the server");
        assert!(win.root().read(cx).status().contains(&http::url(port)));

        let (init, renamed) = while_drawing(cx, win.id, move || {
            let init = post(
                port,
                &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                        "params": {"protocolVersion": "2025-06-18", "clientInfo": {"name": "test-client"}}}),
            )
            .expect("initialize");
            let call = post(
                port,
                &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                        "params": {"name": "apply_command", "arguments": {"command": {"Rename": {"name": "From MCP"}}}}}),
            )
            .expect("tools/call");
            (init, call)
        });
        assert_eq!(init.0, 200);
        assert_eq!(init.1["result"]["serverInfo"]["name"], "azam");
        assert_eq!(renamed.1["result"]["isError"], false);
        let app = win.root().read(cx);
        assert_eq!(app.session().project.meta.name, "From MCP");
        assert!(app.status().contains("MCP: "), "the status bar says what the agent did");
        assert_eq!(http::activity().0.as_deref(), Some("test-client"));

        // the button stops it again, and the port is let go
        cx.draw(win.id);
        let b = cx
            .element_bounds(win.id, "mcp-server")
            .expect("the button");
        cx.simulate_click(
            win.id,
            b.origin.x + b.size.width / 2.0,
            b.origin.y + b.size.height / 2.0,
        );
        cx.draw(win.id);
        assert!(!http::is_running(), "the button stops the server");
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_ok() {
            assert!(started.elapsed() < Duration::from_secs(2), "the port stays open");
            std::thread::sleep(Duration::from_millis(20));
        }
    });
}
