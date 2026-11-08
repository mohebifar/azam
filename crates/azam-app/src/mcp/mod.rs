//! The Model Context Protocol: an AI agent (Claude Code and others) designs garments through
//! tools that read and edit the project, drape it and take screenshots.
//!
//! The app serves it over HTTP on the loopback interface ([`http`]), acting on the window the
//! person has open; the `azam-mcp` binary serves the same tools over stdio on a headless window.

pub mod describe;
pub mod http;
pub mod image;
pub mod tools;

use serde_json::{Map, Value, json};

pub use tools::{Output, Server};

/// Protocol versions spoken, newest first.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What the agent is told about the server when it connects.
const INSTRUCTIONS: &str = "azam is a garment CAD app: 2D pattern pieces are sewn together and \
draped on a 3D avatar by a cloth simulation. All lengths are millimetres; pattern coordinates have \
x to the right and y down. A typical loop: get_project / get_piece to read ids and coordinates -> \
edit with the CAD tools (move_points, curve_segment, add_dart, spread_piece, sew…) -> simulate -> \
screenshot (view 3d from several angles, or view pattern) -> repeat. new_project starts from a \
template (dress is a good start). Each tool call is one undo step. Segments are named by the id of \
their start corner. Seams join edges of equal length: check `mismatch` in get_project after \
reshaping. Save with save_project.";

/// Said in addition when the tools act on the person's own window.
const INTERACTIVE_INSTRUCTIONS: &str = " These tools act on the azam window the person has open: \
they see every edit as it lands and may be editing too, so read the project again (get_project) \
before acting on old ids. screenshot view=3d shows their own 3D view unless you pick an angle.";

/// A JSON-RPC response to request `id`.
fn reply(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// A JSON-RPC error response.
#[must_use]
pub fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Answers one JSON-RPC message (`None` for notifications and responses). Tool calls go to
/// `call`; `interactive` says whether the tools act on the person's own window.
pub fn respond(
    msg: &Value,
    interactive: bool,
    call: &mut dyn FnMut(&str, &Map<String, Value>) -> Output,
) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str)?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSIONS[0]);
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&PROTOCOL_VERSIONS[0]);
            let mut instructions = INSTRUCTIONS.to_owned();
            if interactive {
                instructions.push_str(INTERACTIVE_INSTRUCTIONS);
            }
            reply(
                &id,
                &json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "azam", "title": "azam garment CAD", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": instructions,
                }),
            )
        }
        "ping" => reply(&id, &json!({})),
        "tools/list" => reply(&id, &json!({"tools": tools::list(interactive)})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            let args = params["arguments"].as_object().cloned().unwrap_or_default();
            let result = match call(name, &args) {
                Ok(content) => json!({"content": content, "isError": false}),
                Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
            };
            reply(&id, &result)
        }
        "resources/list" => reply(&id, &json!({"resources": []})),
        "prompts/list" => reply(&id, &json!({"prompts": []})),
        other => error(&id, -32601, &format!("no method {other}")),
    })
}

/// Answers a message that may be a batch; `None` when nothing needs an answer.
pub fn respond_any(
    msg: &Value,
    interactive: bool,
    call: &mut dyn FnMut(&str, &Map<String, Value>) -> Output,
) -> Option<Value> {
    match msg {
        Value::Array(batch) => {
            let out: Vec<Value> = batch
                .iter()
                .filter_map(|m| respond(m, interactive, call))
                .collect();
            (!out.is_empty()).then_some(Value::Array(out))
        }
        msg => respond(msg, interactive, call),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_lists_and_notifications_are_answered_as_the_protocol_asks() {
        let mut call = |_: &str, _: &Map<String, Value>| -> Output { Ok(vec![]) };
        let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}});
        let r = respond(&init, true, &mut call).expect("answered");
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert!(
            r["result"]["instructions"]
                .as_str()
                .is_some_and(|s| s.contains("window"))
        );
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert!(respond(&note, true, &mut call).is_none());
        let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        let r = respond(&list, true, &mut call).expect("answered");
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(names.contains(&"screenshot") && !names.contains(&"open_window"));
        let bad = json!({"jsonrpc": "2.0", "id": 3, "method": "nope"});
        assert_eq!(
            respond(&bad, true, &mut call).expect("answered")["error"]["code"],
            -32601
        );
    }
}
