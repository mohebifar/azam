//! The app's MCP endpoint: Streamable HTTP (plain JSON responses, no event stream) at
//! `http://127.0.0.1:7465/mcp`, so an MCP client drives the window the person has open.
//!
//! It runs only on demand: the person starts it (the MCP panel in the 3D view's sidebar,
//! File ▸ MCP Server, or Preferences to start it on launch) and can stop it any time.
//! While it runs, a background thread accepts connections on the loopback interface only; each
//! request's tool call runs on the UI thread (through a [`UiSender`]) against the window's
//! [`Server`], so edits land between frames like the person's own. Requests from web pages (an
//! `Origin` that is not local) and other host names (DNS rebinding) are refused.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use blenderui::UiSender;
use blenderui::prelude::*;
use serde_json::{Map, Value};

use super::{Output, Server};
use crate::app::AzamApp;

/// Port the endpoint listens on unless `AZAM_MCP_PORT` or `--mcp-port` says otherwise.
pub const DEFAULT_PORT: u16 = 7465;
/// Largest request body taken, in bytes.
const MAX_BODY: usize = 16 << 20;
/// How long a tool call may take on the UI thread.
const CALL_TIMEOUT: Duration = Duration::from_secs(600);
/// Longest a `simulate` call lets the drape play before pausing it.
const SIM_LIMIT: Duration = Duration::from_secs(180);
/// How long a window screenshot lets the window lay out what it changed (a few frames).
const LAYOUT_WAIT: Duration = Duration::from_millis(300);
/// How often a playing `simulate` call checks the simulated time.
const SIM_POLL: Duration = Duration::from_millis(100);
/// How often the listening thread checks for connections and for being stopped.
const ACCEPT_POLL: Duration = Duration::from_millis(30);

/// Bumped by every start and stop: a listening thread whose run is no longer current ends, and
/// requests are refused while it is even (stopped).
static RUN: AtomicU64 = AtomicU64::new(0);
/// Who is connected and what they did last.
static ACTIVITY: Mutex<Activity> = Mutex::new(Activity {
    client: None,
    last: None,
});

/// Who is connected and what they did last.
struct Activity {
    /// The client's name, from its `initialize`.
    client: Option<String>,
    /// The last tool called, and when.
    last: Option<(String, Instant)>,
}

thread_local! {
    /// The tools' server, on the UI thread (where the window lives).
    static SERVER: RefCell<Option<Server>> = const { RefCell::new(None) };
}

/// The endpoint's address on `port`.
#[must_use]
pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

/// Lets the endpoint act on `window` once it is started. Call once, on the UI thread; nothing
/// listens yet.
pub fn install(window: WindowHandle<AzamApp>) {
    SERVER.with(|s| *s.borrow_mut() = Some(Server::new(window, true)));
}

/// Whether the endpoint is running.
#[must_use]
pub fn is_running() -> bool {
    RUN.load(Ordering::SeqCst) % 2 == 1
}

/// Starts listening on `port` (a no-op while running), running tool calls through `ui`.
///
/// # Errors
/// Fails when the port cannot be opened (another azam has it, say).
pub fn start(ui: UiSender, port: u16) -> Result<(), String> {
    if is_running() {
        return Ok(());
    }
    // a thread just stopped may still hold the port for a moment
    let mut tries = 0;
    let listener = loop {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => break l,
            Err(_) if tries < 10 => {
                tries += 1;
                std::thread::sleep(ACCEPT_POLL);
            }
            Err(e) => return Err(format!("cannot listen on port {port}: {e}")),
        }
    };
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    let run = RUN.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut a) = ACTIVITY.lock() {
        *a = Activity {
            client: None,
            last: None,
        };
    }
    std::thread::Builder::new()
        .name("azam-mcp".into())
        .spawn(move || listen(&listener, run, port, &ui))
        .map(|_| ())
        .map_err(|e| {
            RUN.fetch_add(1, Ordering::SeqCst);
            format!("could not start: {e}")
        })
}

/// Stops listening and refuses requests on connections still open.
pub fn stop() {
    if is_running() {
        RUN.fetch_add(1, Ordering::SeqCst);
    }
}

/// Who is connected and what they last did, while running: `(client, last tool, seconds ago)`.
#[must_use]
pub fn activity() -> (Option<String>, Option<(String, f64)>) {
    ACTIVITY.lock().map_or((None, None), |a| {
        (
            a.client.clone(),
            a.last
                .as_ref()
                .map(|(t, at)| (t.clone(), at.elapsed().as_secs_f64())),
        )
    })
}

/// Accepts connections until run `run` is no longer current.
fn listen(listener: &TcpListener, run: u64, port: u16, ui: &UiSender) {
    while RUN.load(Ordering::SeqCst) == run {
        match listener.accept() {
            Ok((stream, _)) => {
                let ui = ui.clone();
                let _ = std::thread::Builder::new()
                    .name("azam-mcp-conn".into())
                    .spawn(move || serve(stream, port, &ui));
            }
            Err(_) => std::thread::sleep(ACCEPT_POLL),
        }
    }
}

/// One HTTP request.
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// Reads the next request of a connection; `Ok(None)` when the client closed it.
fn read_request(reader: &mut impl BufRead) -> std::io::Result<Option<Request>> {
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_owned());
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(bad("not an HTTP request"));
    };
    let path = target.split('?').next().unwrap_or_default().to_owned();
    let method = method.to_owned();
    let mut headers = HashMap::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(bad("headers cut short"));
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
        }
    }
    let mut body = Vec::new();
    if headers
        .get("transfer-encoding")
        .is_some_and(|t| t.eq_ignore_ascii_case("chunked"))
    {
        loop {
            line.clear();
            reader.read_line(&mut line)?;
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or("0"), 16)
                .map_err(|_| bad("bad chunk size"))?;
            if size == 0 {
                // trailers end with an empty line
                loop {
                    line.clear();
                    if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                        break;
                    }
                }
                break;
            }
            if body.len() + size > MAX_BODY {
                return Err(bad("request too large"));
            }
            let start = body.len();
            body.resize(start + size, 0);
            reader.read_exact(&mut body[start..])?;
            line.clear();
            reader.read_line(&mut line)?;
        }
    } else if let Some(n) = headers.get("content-length") {
        let n: usize = n.parse().map_err(|_| bad("bad content length"))?;
        if n > MAX_BODY {
            return Err(bad("request too large"));
        }
        body.resize(n, 0);
        reader.read_exact(&mut body)?;
    }
    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}

/// An HTTP response: status, extra headers and a JSON body (empty for none).
struct Response {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: String,
}

impl Response {
    fn json(status: u16, body: &Value) -> Self {
        Self {
            status,
            headers: vec![("Content-Type", "application/json".into())],
            body: serde_json::to_string(body).unwrap_or_default(),
        }
    }

    fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: String::new(),
        }
    }

    fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("Content-Type", "text/plain; charset=utf-8".into())],
            body: body.to_owned(),
        }
    }

    fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            202 => "Accepted",
            400 => "Bad Request",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            503 => "Service Unavailable",
            _ => "Error",
        };
        let mut head = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\n",
            self.status,
            self.body.len()
        );
        for (k, v) in &self.headers {
            head.push_str(k);
            head.push_str(": ");
            head.push_str(v);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        out.write_all(head.as_bytes())?;
        out.write_all(self.body.as_bytes())?;
        out.flush()
    }
}

/// Serves one connection's requests until the client closes it.
fn serve(stream: TcpStream, port: u16, ui: &UiSender) {
    // an accepted socket can inherit the listener's non-blocking mode
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    let mut out = stream;
    loop {
        let request = match read_request(&mut reader) {
            Ok(Some(r)) => r,
            Ok(None) => return,
            Err(e) => {
                let _ = Response::text(400, &e.to_string()).write(&mut out);
                return;
            }
        };
        let close = request
            .header("connection")
            .is_some_and(|c| c.eq_ignore_ascii_case("close"));
        let response = handle(&request, port, ui);
        if response.write(&mut out).is_err() || close {
            return;
        }
    }
}

/// Whether `host` (a `Host` header, or an `Origin`'s host) names this machine's loopback.
fn is_loopback(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host.rsplit_once(':').map_or(host, |(h, p)| {
            if p.chars().all(|c| c.is_ascii_digit()) {
                h
            } else {
                host
            }
        })
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

/// Answers one request.
fn handle(request: &Request, port: u16, ui: &UiSender) -> Response {
    if !is_running() {
        return Response::text(503, "azam's MCP server was stopped");
    }
    // DNS rebinding: a page on another name must not reach the port through this address
    if request.header("host").is_some_and(|h| !is_loopback(h)) {
        return Response::text(403, "only local clients may use this server");
    }
    // a web page's request carries its origin: only local tools may drive the app
    if let Some(origin) = request.header("origin") {
        let host = origin.split("://").nth(1).unwrap_or(origin);
        if !is_loopback(host) {
            return Response::text(403, "web pages may not use this server");
        }
    }
    if !matches!(request.path.as_str(), "/mcp" | "/mcp/" | "/") {
        return Response::text(404, &format!("the MCP endpoint is {}", url(port)));
    }
    if request.method != "POST" {
        let mut r = Response::empty(405);
        r.headers.push(("Allow", "POST".into()));
        return r;
    }
    let msg: Value = match serde_json::from_slice(&request.body) {
        Ok(v) => v,
        Err(e) => {
            return Response::json(
                400,
                &super::error(&Value::Null, -32700, &format!("parse error: {e}")),
            );
        }
    };
    if msg["method"] == "initialize"
        && let Ok(mut a) = ACTIVITY.lock()
    {
        a.client = msg["params"]["clientInfo"]["name"].as_str().map(str::to_owned);
    }
    let mut call = |name: &str, args: &Map<String, Value>| dispatch(ui, name, args);
    match super::respond_any(&msg, true, &mut call) {
        Some(answer) => Response::json(200, &answer),
        None => Response::empty(202),
    }
}

/// Runs `f` on the UI thread with the window's server and waits for its result.
fn on_ui<R: Send + 'static>(
    ui: &UiSender,
    f: impl FnOnce(&mut Server, &mut App) -> R + Send + 'static,
) -> Result<R, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let sent = ui.send(move |cx| {
        let out = SERVER.with(|s| s.borrow_mut().as_mut().map(|server| f(server, cx)));
        let _ = tx.send(out);
    });
    if !sent {
        return Err("azam is closing".into());
    }
    rx.recv_timeout(CALL_TIMEOUT)
        .map_err(|_| "azam did not answer in time".to_owned())?
        .ok_or_else(|| "azam has no window open".to_owned())
}

/// Runs tool `name` in the app. `simulate` plays the drape in the window and checks on it
/// now and then, so the window keeps drawing while it runs.
fn dispatch(ui: &UiSender, name: &str, args: &Map<String, Value>) -> Output {
    let started = Instant::now();
    if let Ok(mut a) = ACTIVITY.lock() {
        a.last = Some((name.to_owned(), started));
    }
    let (name, args) = (name.to_owned(), args.clone());
    let out = if name == "simulate" {
        let a = args.clone();
        let until = on_ui(ui, move |s, cx| s.start_simulation(cx, &a))??;
        loop {
            std::thread::sleep(SIM_POLL);
            let give_up = started.elapsed() > SIM_LIMIT;
            let a = args.clone();
            if let Some(out) = on_ui(ui, move |s, cx| s.poll_simulation(cx, &a, until, give_up))? {
                break out;
            }
        }
    } else if name == "screenshot"
        && args
            .get("view")
            .and_then(Value::as_str)
            .is_some_and(|v| v != "3d")
    {
        // the window lays out a maximized editor or another workspace on its next frames
        let a = args.clone();
        if on_ui(ui, move |s, cx| s.begin_screenshot(cx, &a))?? {
            std::thread::sleep(LAYOUT_WAIT);
            on_ui(ui, Server::finish_screenshot)?
        } else {
            on_ui(ui, move |s, cx| s.call(cx, "screenshot", &args))?
        }
    } else {
        let n = name.clone();
        on_ui(ui, move |s, cx| s.call(cx, &n, &args))?
    };
    eprintln!("azam: MCP {name} took {:.2} s", started.elapsed().as_secs_f64());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse_with_a_length_or_chunked() {
        let raw = b"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:7465\r\nContent-Length: 2\r\n\r\n{}";
        let r = read_request(&mut &raw[..]).expect("reads").expect("a request");
        assert_eq!(
            (r.method.as_str(), r.path.as_str(), r.body.as_slice()),
            ("POST", "/mcp", &b"{}"[..])
        );
        let raw =
            b"POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{\"a\r\n4\r\n\":1}\r\n0\r\n\r\n";
        let r = read_request(&mut &raw[..]).expect("reads").expect("a request");
        assert_eq!(r.body, b"{\"a\":1}");
        assert!(read_request(&mut &b""[..]).expect("reads").is_none());
    }

    #[test]
    fn only_loopback_hosts_pass() {
        assert!(is_loopback("127.0.0.1:7465"));
        assert!(is_loopback("localhost"));
        assert!(is_loopback("[::1]:7465"));
        assert!(!is_loopback("evil.example:7465"));
        assert!(!is_loopback("192.168.1.2:7465"));
    }
}
