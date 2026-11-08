//! The tools the server offers: their schemas and what each one does to the app.
//!
//! Every edit goes through the session as one undo step, so `undo` takes back exactly one tool
//! call, and a live window (or any collaborator) sees each edit as it lands.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::app::AzamApp;
use crate::automation::SimReport;
use crate::editors::Editor;
use crate::operators::{Shading, Template};
use azam_doc::{
    Command, Created, FabricId, FabricPreset, FlipAxis, PieceId, PieceTransform, PointId, Project, SeamId,
    SeamSide,
};
use azam_geom::Vec2;
use azam_render::{OrbitCamera, ViewPreset};
use blenderui::prelude::*;
use glam::DAffine2;
use serde_json::{Map, Value, json};

use super::describe;
use super::image::{base64, png_rgba, shrink_rgba, xrgb_to_rgba};

/// Tool arguments.
type Args = Map<String, Value>;
/// A tool's result: MCP content blocks, or why it failed.
pub type Output = Result<Vec<Value>, String>;

/// Most screen frames one `simulate` call steps.
const MAX_SIM_FRAMES: usize = 20_000;
/// Size of a 3D screenshot when none is asked for: portrait, a whole dress in frame.
const VIEW3D_SIZE: (u32, u32) = (800, 1000);
/// Widest a window screenshot is sent (a Retina window is halved).
const MAX_WINDOW_WIDTH: u32 = 1600;
/// Largest image side a tool makes.
const MAX_SIDE: u32 = 2400;

/// A text content block.
fn text(s: impl Into<String>) -> Value {
    json!({"type": "text", "text": s.into()})
}

/// A JSON value as a text content block.
fn json_text(v: &Value) -> Value {
    text(serde_json::to_string_pretty(v).unwrap_or_default())
}

/// A PNG as an image content block.
fn png_block(png: &[u8]) -> Value {
    json!({"type": "image", "data": base64(png), "mimeType": "image/png"})
}

// -- argument parsing -------------------------------------------------------------------------

fn get<'a>(a: &'a Args, k: &str) -> Option<&'a Value> {
    a.get(k).filter(|v| !v.is_null())
}

fn req_str<'a>(a: &'a Args, k: &str) -> Result<&'a str, String> {
    get(a, k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{k}` (a string) is required"))
}

fn opt_str<'a>(a: &'a Args, k: &str) -> Option<&'a str> {
    get(a, k).and_then(Value::as_str)
}

fn opt_f64(a: &Args, k: &str) -> Result<Option<f64>, String> {
    match get(a, k) {
        None => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite())
            .map(Some)
            .ok_or_else(|| format!("`{k}` must be a number")),
    }
}

fn req_f64(a: &Args, k: &str) -> Result<f64, String> {
    opt_f64(a, k)?.ok_or_else(|| format!("`{k}` (a number) is required"))
}

fn opt_bool(a: &Args, k: &str) -> Option<bool> {
    get(a, k).and_then(Value::as_bool)
}

/// A point given as `{x, y}` or `[x, y]`.
fn vec2(v: &Value, what: &str) -> Result<Vec2, String> {
    let (x, y) = match v {
        Value::Array(xy) if xy.len() >= 2 => (xy[0].as_f64(), xy[1].as_f64()),
        Value::Object(o) => (
            o.get("x").and_then(Value::as_f64),
            o.get("y").and_then(Value::as_f64),
        ),
        _ => (None, None),
    };
    match (x, y) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(Vec2::new(x, y)),
        _ => Err(format!("`{what}` must be {{\"x\": mm, \"y\": mm}}")),
    }
}

fn req_vec2(a: &Args, k: &str) -> Result<Vec2, String> {
    vec2(get(a, k).ok_or_else(|| format!("`{k}` is required"))?, k)
}

fn array<'a>(a: &'a Args, k: &str) -> Result<&'a Vec<Value>, String> {
    get(a, k)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("`{k}` (an array) is required"))
}

/// The piece `v` names: its id, or its name (ignoring case).
fn piece_id(project: &Project, v: &Value) -> Result<PieceId, String> {
    let s = v
        .as_str()
        .ok_or("a piece is given by its id or name (a string)")?;
    if project.pieces.contains_key(&PieceId::new(s)) {
        return Ok(PieceId::new(s));
    }
    let by_name: Vec<&PieceId> = project
        .pieces
        .values()
        .filter(|p| p.name.eq_ignore_ascii_case(s))
        .map(|p| &p.id)
        .collect();
    match by_name.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("no piece {s:?} (get_project lists the pieces)")),
        _ => Err(format!("several pieces are named {s:?}: use an id")),
    }
}

fn req_piece(project: &Project, a: &Args, k: &str) -> Result<PieceId, String> {
    piece_id(project, get(a, k).ok_or_else(|| format!("`{k}` is required"))?)
}

fn pieces(project: &Project, a: &Args, k: &str) -> Result<Vec<PieceId>, String> {
    match get(a, k) {
        Some(Value::String(_)) => Ok(vec![req_piece(project, a, k)?]),
        _ => array(a, k)?.iter().map(|v| piece_id(project, v)).collect(),
    }
}

/// The outline point `v` names on `piece`: its id, or its index.
fn point_id(project: &Project, piece: &PieceId, v: &Value) -> Result<PointId, String> {
    let pc = &project.pieces[piece];
    if let Some(i) = v.as_u64() {
        return pc
            .pts
            .get(i as usize)
            .map(|p| p.id.clone())
            .ok_or_else(|| format!("{} has {} points; there is no point {i}", pc.name, pc.pts.len()));
    }
    let s = v
        .as_str()
        .ok_or("a point is given by its id (a string) or index")?;
    pc.pts
        .iter()
        .find(|p| p.id.0 == s)
        .map(|p| p.id.clone())
        .ok_or_else(|| format!("{} has no point {s:?} (get_piece lists them)", pc.name))
}

/// The segment `v` names on `piece`: the id of its start corner, or its index among the segments.
fn segment_id(project: &Project, piece: &PieceId, v: &Value) -> Result<PointId, String> {
    let pc = &project.pieces[piece];
    let outline = pc.outline();
    if let Some(i) = v.as_u64() {
        return outline
            .segments
            .get(i as usize)
            .map(|s| s.id.clone())
            .ok_or_else(|| {
                format!(
                    "{} has {} segments; there is no segment {i}",
                    pc.name,
                    outline.segments.len()
                )
            });
    }
    let s = v
        .as_str()
        .ok_or("a segment is given by its id (a string) or index")?;
    outline
        .segments
        .iter()
        .find(|seg| seg.id.0 == s)
        .map(|seg| seg.id.clone())
        .ok_or_else(|| {
            format!("{} has no segment {s:?}: a segment's id is the id of its start corner (get_piece lists them)", pc.name)
        })
}

fn req_segment(project: &Project, piece: &PieceId, a: &Args, k: &str) -> Result<PointId, String> {
    segment_id(
        project,
        piece,
        get(a, k).ok_or_else(|| format!("`{k}` is required"))?,
    )
}

/// What a command created, as JSON.
fn created(list: &[Created]) -> Value {
    Value::Array(
        list.iter()
            .map(|c| match c {
                Created::Piece(id) => json!({"piece": id.0}),
                Created::Point(id) => json!({"point": id.0}),
                Created::Seam(id) => json!({"seam": id.0}),
                Created::InternalLine(id) => json!({"internalLine": id.0}),
            })
            .collect(),
    )
}

/// RFC 7386 JSON merge patch.
fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    if let Value::Object(t) = target {
        for (k, v) in patch {
            if v.is_null() {
                t.remove(k);
            } else {
                merge_patch(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
    }
}

// -- the tool list ----------------------------------------------------------------------------

/// A tool definition.
fn tool(name: &str, description: &str, properties: &Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
    })
}

const POINT: &str = "{\"x\": mm, \"y\": mm}";

/// Every tool, for `tools/list`.
#[allow(clippy::too_many_lines, reason = "one entry per tool")]
#[must_use]
pub fn list(interactive: bool) -> Vec<Value> {
    let piece = json!({"type": "string", "description": "Piece id or name"});
    let pieces = json!({"type": "array", "items": {"type": "string"}, "description": "Piece ids or names"});
    let segment = json!({"type": ["string", "integer"], "description": "Segment: the id of its start corner, or its index (get_piece lists segments)"});
    let point = json!({"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}, "required": ["x", "y"]});
    let side = json!({
        "type": "object",
        "properties": {
            "piece": piece, "segment": segment,
            "reverse": {"type": "boolean", "description": "Walk the segment from its end to its start"},
            "from": {"type": "number", "description": "Start fraction along the segment (default 0)"},
            "to": {"type": "number", "description": "End fraction along the segment (default 1)"},
        },
        "required": ["piece", "segment"],
    });
    let camera = json!({
        "angle": {"type": "string", "enum": ["current", "front", "back", "left", "right", "top", "three-quarter"], "description": "Camera direction. current: the person's own 3D view as they left it (the default in the app; three-quarter otherwise)"},
        "yaw": {"type": "number", "description": "Degrees about the vertical axis; 0 faces the avatar's front (overrides angle)"},
        "pitch": {"type": "number", "description": "Degrees of elevation; positive looks down"},
        "zoom": {"type": "number", "description": "Above 1 moves closer (default 1: the whole avatar in frame)"},
    });
    let mut screenshot_props = json!({
        "view": {"type": "string", "enum": ["3d", "pattern", "window"], "description": "3d: the avatar and garment; pattern: the 2D pattern editor maximized; window: the whole app as a person sees it (default 3d)"},
        "shading": {"type": "string", "enum": ["solid", "fitmap"], "description": "3D shading; fitmap colours strain (blue loose, red tight)"},
        "width": {"type": "integer"}, "height": {"type": "integer"},
        "workspace": {"type": "string", "enum": ["design", "grading", "colorway", "review", "techpack"], "description": "For view=window: the workspace to show"},
        "select": piece,
    });
    if let (Value::Object(m), Value::Object(c)) = (&mut screenshot_props, camera.clone()) {
        m.extend(c);
    }
    let mut render_props = json!({
        "width": {"type": "integer"}, "height": {"type": "integer"},
        "samples": {"type": "integer", "description": "Samples per pixel (default 64)"},
    });
    if let (Value::Object(m), Value::Object(c)) = (&mut render_props, camera) {
        m.extend(c);
    }
    let mut tools = vec![
        tool(
            "new_project",
            "Start a new garment from a template drafted to the avatar's measurements: dress, tee, skirt, labadeh, aba or blank (empty). Replaces the open project (save it first if needed).",
            &json!({"template": {"type": "string", "enum": ["dress", "tee", "skirt", "labadeh", "aba", "blank"]}, "name": {"type": "string"}}),
            &["template"],
        ),
        tool(
            "open_project",
            "Open a saved azam project (.azam.json).",
            &json!({"path": {"type": "string"}}),
            &["path"],
        ),
        tool(
            "save_project",
            "Save the project. Without a path it saves where it was opened or last saved, else to ~/<name>.azam.json.",
            &json!({"path": {"type": "string"}}),
            &[],
        ),
        tool(
            "export",
            "Export the pattern or the draped garment: svg, dxf (AAMA), dxf-graded, astm, pattern-pdf (tiled 1:1), techpack-pdf, marker-svg, marker-dxf, obj, gltf or fbx.",
            &json!({"format": {"type": "string", "enum": ["svg", "dxf", "dxf-graded", "astm", "pattern-pdf", "techpack-pdf", "marker-svg", "marker-dxf", "obj", "gltf", "fbx"]}, "path": {"type": "string"}}),
            &["format", "path"],
        ),
        tool(
            "get_project",
            "Summary of the garment: avatar measurements, fabrics, colorways, every piece (id, name, bounding box, arrangement slot) and every seam (sides sewn and both lengths, so mismatches show). Coordinates are mm, x right, y down.",
            &json!({}),
            &[],
        ),
        tool(
            "get_piece",
            "Every detail of one piece: its outline points (id, x, y, curve flag; curve points are Bezier controls between corners), its segments (id = start corner's id, end corner, length, seams on it, hem, allowance), internal lines and arrangement around the avatar.",
            &json!({"piece": piece}),
            &["piece"],
        ),
        tool(
            "get_json",
            "The project file's JSON, whole or at a JSON pointer (e.g. /fabrics or /pieces/<id>/arr). For anything the other tools do not show.",
            &json!({"pointer": {"type": "string"}}),
            &[],
        ),
        tool(
            "create_piece",
            &format!(
                "Draw a new pattern piece. Give `points` (outline in order, each {POINT} plus optional \"curve\": true for a Bezier control point between corners), or a `rect` or `ellipse` box [x0, y0, x1, y1]. Place it away from other pieces. Returns the new piece's id."
            ),
            &json!({
                "points": {"type": "array", "items": {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}, "curve": {"type": "boolean"}}, "required": ["x", "y"]}},
                "rect": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4},
                "ellipse": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4},
                "name": {"type": "string"}, "fabric": {"type": "string"},
                "slot": {"type": "string", "description": "Arrangement slot around the avatar: front, back, frontL, frontR, backL, backR, left, right, armL, armR, neck, legLF, legLB, legRF, legRB"},
            }),
            &[],
        ),
        tool(
            "move_points",
            "Move outline points of a piece: each entry gives the point (id or index) and either an absolute position x/y or an offset dx/dy (mm), and optionally sets whether it is a curve point. Linked mirrors, symmetric halves and length links follow.",
            &json!({
                "piece": piece,
                "points": {"type": "array", "items": {"type": "object", "properties": {"point": {"type": ["string", "integer"]}, "x": {"type": "number"}, "y": {"type": "number"}, "dx": {"type": "number"}, "dy": {"type": "number"}, "curve": {"type": "boolean"}}, "required": ["point"]}},
            }),
            &["piece", "points"],
        ),
        tool(
            "insert_point",
            "Insert a corner into a segment at fraction t (0..1) of its length, keeping its curve. Returns the new point's id.",
            &json!({"piece": piece, "segment": segment, "t": {"type": "number"}}),
            &["piece", "segment", "t"],
        ),
        tool(
            "curve_segment",
            &format!(
                "Bend a segment into a curve: either through a point {POINT}, or by `bulge`: how far the curve's middle sits from the straight line between its ends, in mm (positive out of the piece, negative into it, 0 straightens; get_piece shows each segment's current bulgeMm)."
            ),
            &json!({"piece": piece, "segment": segment, "through": point, "bulge": {"type": "number"}}),
            &["piece", "segment"],
        ),
        tool(
            "delete_points",
            "Delete outline points (at least three corners must remain).",
            &json!({"piece": piece, "points": {"type": "array", "items": {"type": ["string", "integer"]}}}),
            &["piece", "points"],
        ),
        tool(
            "set_segment_length",
            "Make a straight segment a given length (mm), moving its end corner along it.",
            &json!({"piece": piece, "segment": segment, "length": {"type": "number"}}),
            &["piece", "segment", "length"],
        ),
        tool(
            "transform_pieces",
            "Move, rotate and/or scale whole pieces in the pattern: dx/dy in mm, rotate in degrees (clockwise on screen), scale a factor; rotation and scale are about `center` (default: the pieces' bounding-box centre).",
            &json!({"pieces": pieces, "dx": {"type": "number"}, "dy": {"type": "number"}, "rotate": {"type": "number"}, "scale": {"type": "number"}, "scale_y": {"type": "number", "description": "A different vertical scale (default: same as scale)"}, "center": point}),
            &["pieces"],
        ),
        tool(
            "flip_pieces",
            "Mirror pieces in place, horizontally (left-right) or vertically.",
            &json!({"pieces": pieces, "axis": {"type": "string", "enum": ["horizontal", "vertical"]}}),
            &["pieces"],
        ),
        tool(
            "duplicate_piece",
            "Copy a piece, or with mirror: true make its mirror image (linked: true keeps the mirror following later edits of the original, like a left/right pair). Returns the new piece.",
            &json!({"piece": piece, "mirror": {"type": "boolean"}, "linked": {"type": "boolean"}}),
            &["piece"],
        ),
        tool(
            "delete_pieces",
            "Delete pieces and the seams on them.",
            &json!({"pieces": pieces}),
            &["pieces"],
        ),
        tool(
            "cut_piece",
            "Cut a piece in two along a straight line from `from` to `to` (the line must cross the outline exactly twice). Returns the new piece.",
            &json!({"piece": piece, "from": point, "to": point}),
            &["piece", "from", "to"],
        ),
        tool(
            "spread_piece",
            "Slash and spread along a line from `from` to `to` crossing the piece: `fan` mm opens a wedge at the line's start (adds flare), or `pleat` mm makes a knife pleat that deep. The half to the line's left (of its direction) moves.",
            &json!({"piece": piece, "from": point, "to": point, "fan": {"type": "number"}, "pleat": {"type": "number"}}),
            &["piece", "from", "to"],
        ),
        tool(
            "add_dart",
            "Cut a dart into a segment, centred at fraction t (default 0.5), `width` mm wide at the edge and up to `depth` mm deep, and sew its legs.",
            &json!({"piece": piece, "segment": segment, "t": {"type": "number"}, "width": {"type": "number"}, "depth": {"type": "number"}}),
            &["piece", "segment", "width", "depth"],
        ),
        tool(
            "round_corner",
            "Round a corner with a fillet of about `radius` mm.",
            &json!({"piece": piece, "point": {"type": ["string", "integer"]}, "radius": {"type": "number"}}),
            &["piece", "point", "radius"],
        ),
        tool(
            "add_internal_line",
            "Draw an internal line inside a piece (fold line, placement line, style line).",
            &json!({"piece": piece, "points": {"type": "array", "items": point}, "closed": {"type": "boolean"}}),
            &["piece", "points"],
        ),
        tool(
            "sew",
            "Sew stretches of outline together: sides `a` are sewn to sides `b` (several sides on one end sew them end to end, e.g. a waistband to two skirt panels). Each side is {piece, segment, reverse?, from?, to?}. Sewn edges should run the same way round the garment: set reverse on one side when the seam would twist. Returns the seam and both lengths.",
            &json!({"a": {"type": "array", "items": side.clone()}, "b": {"type": "array", "items": side}}),
            &["a", "b"],
        ),
        tool(
            "delete_seam",
            "Unsew a seam.",
            &json!({"seam": {"type": "string"}}),
            &["seam"],
        ),
        tool(
            "set_allowance",
            "Seam allowance in mm: of a seam (`seam`), of one segment (`piece` + `segment`), or of every piece (neither).",
            &json!({"mm": {"type": "number"}, "seam": {"type": "string"}, "piece": piece, "segment": segment}),
            &["mm"],
        ),
        tool(
            "set_hem",
            "Hem allowance of a segment in mm (0 removes it); with `auto`: true, adds the suggested hems to every free edge instead.",
            &json!({"piece": piece, "segment": segment, "mm": {"type": "number"}, "auto": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "set_piece",
            "Set a piece's properties: name, fabric, grain (degrees), layer (higher sits outside), state (active, frozen, off), seam allowance, and its arrangement around the avatar before draping: slot, offset (mm from the body), height (mm of its centre above the floor), along (mm along a limb slot) and angle (degrees around the body).",
            &json!({"piece": piece, "name": {"type": "string"}, "fabric": {"type": "string"}, "grain": {"type": "number"}, "layer": {"type": "integer"}, "state": {"type": "string", "enum": ["active", "frozen", "off"]}, "slot": {"type": "string"}, "offset": {"type": "number"}, "height": {"type": "number"}, "along": {"type": "number"}, "angle": {"type": "number"}}),
            &["piece"],
        ),
        tool(
            "set_fabric",
            "Change a fabric (or add one with new: true): name, colour (#rrggbb, as the active colorway shows it) and preset, which sets its look and physics: poplin, jersey, rib, denim, silk, chiffon, wool, linen, satin, leather, scuba, fleece, corduroy. Assign it to pieces with set_piece.",
            &json!({"fabric": {"type": "string", "description": "Fabric id (default: the first fabric)"}, "new": {"type": "boolean"}, "name": {"type": "string"}, "color": {"type": "string"}, "preset": {"type": "string"}}),
            &[],
        ),
        tool(
            "set_avatar",
            "Change the avatar's body: any of the measurement fields get_project shows under `avatar` (mm, e.g. height, bust, waist, hip), and sex (female, male). The garment re-drapes on the new body.",
            &json!({"measurements": {"type": "object"}}),
            &["measurements"],
        ),
        tool(
            "patch_project",
            "Apply a JSON merge patch (RFC 7386) to the project file's JSON as one undoable edit, for anything without its own tool (sizes, tech pack notes, colorways, topstitches…). get_json shows the structure. The result must still be a valid project.",
            &json!({"patch": {"type": "object"}}),
            &["patch"],
        ),
        tool(
            "apply_command",
            "Apply an azam_doc::Command as JSON (serde's externally tagged form), e.g. {\"MovePoints\": {\"piece\": \"p1\", \"points\": [\"a1\"], \"delta\": [10, 0]}} or {\"Batch\": [...]}. Vectors are [x, y] in mm.",
            &json!({"command": {"type": "object"}}),
            &["command"],
        ),
        tool(
            "undo",
            "Undo the last edits (one tool call is one step).",
            &json!({"steps": {"type": "integer"}}),
            &[],
        ),
        tool(
            "redo",
            "Redo undone edits.",
            &json!({"steps": {"type": "integer"}}),
            &[],
        ),
        tool(
            "history",
            "The undo history: steps done (oldest first) and steps that redo would bring back.",
            &json!({}),
            &[],
        ),
        tool(
            "simulate",
            "Drape the garment on the avatar: run the cloth simulation for `seconds` of simulated time (default 5; it stops early once settled). reset: true starts from the flat arranged pieces again (do this after big pattern changes). Returns the fit and, unless show is false, a 3D picture.",
            &json!({"seconds": {"type": "number"}, "reset": {"type": "boolean"}, "show": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "screenshot",
            "See the design. view=3d renders the avatar and garment from any angle (call simulate first to see it draped); view=pattern shows the 2D pattern editor with every piece and seam; view=window shows the whole app as a person sees it.",
            &screenshot_props,
            &[],
        ),
        tool(
            "render",
            "Path-trace a photoreal still of the draped garment (slower than screenshot).",
            &render_props,
            &[],
        ),
        tool(
            "open_window",
            "Open the azam app in a window on this computer, joined live to this session: the person sees every edit as it happens, and their edits in the window come back here.",
            &json!({}),
            &[],
        ),
        tool(
            "share_session",
            "Host this project as a live session on the network and return the share code and address, for joining from an azam window (File > Join Session).",
            &json!({}),
            &[],
        ),
        tool(
            "join_session",
            "Join a live session someone shares from their azam window (File > Share gives the code): their project replaces this one and edits flow both ways.",
            &json!({"target": {"type": "string", "description": "Share code or host:port"}}),
            &["target"],
        ),
    ];
    if interactive {
        // in the app the person's own window is the live view: nothing to open, share or join
        tools.retain(|t| !LIVE_TOOLS.contains(&t["name"].as_str().unwrap_or_default()));
    }
    tools
}

/// Tools that link a headless server to windows; the app itself leaves them out.
const LIVE_TOOLS: [&str; 3] = ["open_window", "share_session", "join_session"];

/// Tools that only look: they leave the status bar alone.
const READ_ONLY: [&str; 6] = [
    "get_project",
    "get_piece",
    "get_json",
    "history",
    "screenshot",
    "render",
];

// -- the server -------------------------------------------------------------------------------

/// The app the tools act on, and what the server started.
pub struct Server {
    window: WindowHandle<AzamApp>,
    /// Whether the window is the person's own (the app's HTTP endpoint) rather than a headless
    /// one: tools then keep the person's view (camera, workspace, pattern framing) as it was.
    interactive: bool,
    /// A window screenshot begun and not yet taken.
    pending_shot: Option<PendingShot>,
    /// Windows opened with `open_window`.
    children: Vec<std::process::Child>,
}

impl Server {
    /// A server acting on `window` (`interactive`: the person's own window).
    #[must_use]
    pub fn new(window: WindowHandle<AzamApp>, interactive: bool) -> Self {
        Self {
            window,
            interactive,
            pending_shot: None,
            children: Vec::new(),
        }
    }

    fn app<'a>(&self, cx: &'a App) -> &'a AzamApp {
        self.window.root().read(cx)
    }

    fn with<R>(&self, cx: &mut App, f: impl FnOnce(&mut AzamApp, &mut Context<AzamApp>) -> R) -> R {
        self.window.root().update(cx, |app, cx| {
            let out = f(app, cx);
            // the window redraws with what changed
            cx.notify();
            out
        })
    }

    /// Changes the project with `f` as one undo step named `label`. When `f` fails the project
    /// stays as it was and nothing is recorded.
    fn edit<T>(
        &self,
        cx: &mut App,
        label: &str,
        f: impl FnOnce(&mut Project) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with(cx, |app, _| {
            let mut project = app.session().project.clone();
            let out = f(&mut project)?;
            if project != app.session().project {
                app.session_mut().edit(label, move |p| *p = project);
            }
            Ok(out)
        })
    }

    /// Applies `command` as one undo step; returns what it created.
    fn command(&self, cx: &mut App, command: Command) -> Result<Vec<Created>, String> {
        let label = command.label();
        self.edit(cx, label, |p| {
            azam_doc::apply(p, command)
                .map(|a| a.created)
                .map_err(|e| e.to_string())
        })
    }

    fn project<'a>(&self, cx: &'a App) -> &'a Project {
        &self.app(cx).session().project
    }

    /// Runs tool `name` with `args`.
    ///
    /// # Errors
    /// Why the tool could not run; the project is left unchanged.
    pub fn call(&mut self, cx: &mut App, name: &str, args: &Args) -> Output {
        // take in what collaborators (a live window) did since the last call
        self.with(cx, |app, _| app.pull_collaborators());
        let out = self.run(cx, name, args);
        if self.interactive && out.is_ok() && !READ_ONLY.contains(&name) {
            // the person sees what the agent just did in the status bar
            self.with(cx, |app, _| {
                app.status = format!("Claude Code: {}", name.replace('_', " "));
            });
        }
        out
    }

    #[allow(clippy::too_many_lines, reason = "one arm per tool")]
    fn run(&mut self, cx: &mut App, name: &str, args: &Args) -> Output {
        match name {
            "new_project" => self.new_project(cx, args),
            "open_project" => {
                let path = PathBuf::from(req_str(args, "path")?);
                if self.is_live(cx) {
                    let project = crate::session::Session::load(&path)?;
                    self.replace_live(cx, "Open", project, Some(path));
                } else {
                    self.with(cx, |app, _| app.open_file(&path))?;
                }
                Ok(vec![json_text(&self.summary(cx))])
            }
            "save_project" => self.save(cx, args),
            "export" => self.export(cx, args),
            "get_project" => Ok(vec![json_text(&self.summary(cx))]),
            "get_piece" => {
                let project = self.project(cx);
                let id = req_piece(project, args, "piece")?;
                Ok(vec![json_text(&describe::piece(project, &project.pieces[&id]))])
            }
            "get_json" => {
                let value = self.project(cx).to_value();
                let pointer = opt_str(args, "pointer").unwrap_or("");
                let found = value
                    .pointer(pointer)
                    .ok_or_else(|| format!("nothing at {pointer:?}"))?;
                Ok(vec![json_text(found)])
            }
            "create_piece" => self.create_piece(cx, args),
            "move_points" => self.move_points(cx, args),
            "insert_point" => {
                let project = self.project(cx);
                let piece = req_piece(project, args, "piece")?;
                let segment = req_segment(project, &piece, args, "segment")?;
                let t = req_f64(args, "t")?;
                let made = self.command(cx, Command::SplitSegment { piece, segment, t })?;
                Ok(vec![json_text(&json!({"created": created(&made)}))])
            }
            "curve_segment" => self.curve_segment(cx, args),
            "delete_points" => {
                let project = self.project(cx);
                let piece = req_piece(project, args, "piece")?;
                let points = array(args, "points")?
                    .iter()
                    .map(|v| point_id(project, &piece, v))
                    .collect::<Result<_, _>>()?;
                self.command(cx, Command::DeletePoints { piece, points })?;
                Ok(vec![text("Deleted.")])
            }
            "set_segment_length" => {
                let project = self.project(cx);
                let piece = req_piece(project, args, "piece")?;
                let segment = req_segment(project, &piece, args, "segment")?;
                let length = req_f64(args, "length")?;
                if length <= 0.0 {
                    return Err("the length must be positive".into());
                }
                // the segment is named by its start corner, and the edit takes that corner's index
                let index = project.pieces[&piece]
                    .point_index(&segment)
                    .ok_or("no such segment")?;
                self.edit(cx, "Segment length", |p| {
                    azam_doc::set_segment_length(p, &piece, index, length)
                        .then_some(())
                        .ok_or_else(|| {
                            "only a straight segment (no curve points) can be set to a length".to_owned()
                        })
                })?;
                Ok(vec![text(format!("Segment {} is now {length} mm.", segment.0))])
            }
            "transform_pieces" => self.transform(cx, args),
            "flip_pieces" => {
                let pieces = pieces(self.project(cx), args, "pieces")?;
                let axis = match opt_str(args, "axis").unwrap_or("horizontal") {
                    "vertical" => FlipAxis::Vertical,
                    _ => FlipAxis::Horizontal,
                };
                self.command(cx, Command::FlipPieces { pieces, axis })?;
                Ok(vec![text("Flipped.")])
            }
            "duplicate_piece" => {
                let piece = req_piece(self.project(cx), args, "piece")?;
                let command = if opt_bool(args, "mirror") == Some(true) {
                    Command::MirrorPiece {
                        piece,
                        linked: opt_bool(args, "linked").unwrap_or(false),
                    }
                } else {
                    Command::DuplicatePiece { piece }
                };
                let made = self.command(cx, command)?;
                Ok(vec![json_text(&json!({"created": created(&made)}))])
            }
            "delete_pieces" => {
                let pieces = pieces(self.project(cx), args, "pieces")?;
                self.command(cx, Command::DeletePieces { pieces })?;
                Ok(vec![text("Deleted.")])
            }
            "cut_piece" => {
                let piece = req_piece(self.project(cx), args, "piece")?;
                let (a, b) = (req_vec2(args, "from")?, req_vec2(args, "to")?);
                let made = self.edit(cx, "Cut", |p| {
                    azam_doc::cut_piece(p, &piece, a, b).map_err(|e| e.to_string())
                })?;
                Ok(vec![json_text(&json!({"kept": piece.0, "created": made.0}))])
            }
            "spread_piece" => {
                let piece = req_piece(self.project(cx), args, "piece")?;
                let (a, b) = (req_vec2(args, "from")?, req_vec2(args, "to")?);
                let how = match (opt_f64(args, "fan")?, opt_f64(args, "pleat")?) {
                    (Some(amount), None) => azam_doc::Spread::Fan { amount },
                    (None, Some(depth)) => azam_doc::Spread::Pleat { depth },
                    _ => return Err("give one of `fan` or `pleat` (mm)".into()),
                };
                let label = if matches!(how, azam_doc::Spread::Fan { .. }) {
                    "Slash & Spread"
                } else {
                    "Pleat"
                };
                self.edit(cx, label, |p| {
                    azam_doc::spread_piece(p, &piece, a, b, how).map_err(|e| e.to_string())
                })?;
                Ok(vec![text("Spread.")])
            }
            "add_dart" => {
                let project = self.project(cx);
                let piece = req_piece(project, args, "piece")?;
                let segment = req_segment(project, &piece, args, "segment")?;
                let t = opt_f64(args, "t")?.unwrap_or(0.5);
                let (width, depth) = (req_f64(args, "width")?, req_f64(args, "depth")?);
                let seam = self.edit(cx, "Dart", |p| {
                    azam_doc::add_dart(p, &piece, &segment, t, width, depth).map_err(|e| e.to_string())
                })?;
                Ok(vec![json_text(&json!({"seam": seam.0}))])
            }
            "round_corner" => {
                let project = self.project(cx);
                let piece = req_piece(project, args, "piece")?;
                let point = point_id(project, &piece, get(args, "point").ok_or("`point` is required")?)?;
                let index = project.pieces[&piece]
                    .point_index(&point)
                    .ok_or("no such point")?;
                let radius = req_f64(args, "radius")?;
                self.edit(cx, "Fillet", |p| {
                    azam_doc::fillet_corner(p, &piece, index, radius)
                        .then_some(())
                        .ok_or_else(|| "that point is not a corner that can be rounded".to_owned())
                })?;
                Ok(vec![text("Rounded.")])
            }
            "add_internal_line" => {
                let piece = req_piece(self.project(cx), args, "piece")?;
                let points = array(args, "points")?
                    .iter()
                    .map(|v| {
                        Ok(azam_doc::LinePoint {
                            id: None,
                            pos: vec2(v, "points")?,
                            curve: v.get("curve").and_then(Value::as_bool).unwrap_or(false),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let closed = opt_bool(args, "closed").unwrap_or(false);
                let made = self.command(
                    cx,
                    Command::AddInternalLine {
                        piece,
                        points,
                        closed,
                    },
                )?;
                Ok(vec![json_text(&json!({"created": created(&made)}))])
            }
            "sew" => self.sew(cx, args),
            "delete_seam" => {
                let seam = SeamId::new(req_str(args, "seam")?);
                self.command(cx, Command::DeleteSeam { seam })?;
                Ok(vec![text("Unsewn.")])
            }
            "set_allowance" => {
                let mm = req_f64(args, "mm")?;
                let command = if let Some(seam) = opt_str(args, "seam") {
                    Command::SetSeamAllowance {
                        seam: SeamId::new(seam),
                        mm,
                    }
                } else if get(args, "piece").is_some() {
                    let project = self.project(cx);
                    let piece = req_piece(project, args, "piece")?;
                    let segment = req_segment(project, &piece, args, "segment")?;
                    Command::SetSegmentAllowance { piece, segment, mm }
                } else {
                    Command::SetAllAllowances { mm }
                };
                self.command(cx, command)?;
                Ok(vec![text("Allowance set.")])
            }
            "set_hem" => {
                if opt_bool(args, "auto") == Some(true) {
                    self.command(cx, Command::MarkHems)?;
                    return Ok(vec![text("Hems added to the free edges.")]);
                }
                let project = self.project(cx);
                let piece = req_piece(project, args, "piece")?;
                let segment = req_segment(project, &piece, args, "segment")?;
                let mm = req_f64(args, "mm")?;
                self.command(cx, Command::SetHem { piece, segment, mm })?;
                Ok(vec![text("Hem set.")])
            }
            "set_piece" => self.set_piece(cx, args),
            "set_fabric" => self.set_fabric(cx, args),
            "set_avatar" => self.set_avatar(cx, args),
            "patch_project" => {
                let patch = get(args, "patch").ok_or("`patch` is required")?.clone();
                self.edit(cx, "Patch", |p| {
                    let mut value = p.to_value();
                    merge_patch(&mut value, &patch);
                    let text = serde_json::to_string(&value).map_err(|e| e.to_string())?;
                    let ids = p.ids.clone();
                    *p = Project::from_json(&text)
                        .map_err(|e| format!("not a valid project after the patch: {e}"))?;
                    p.ids = ids;
                    Ok(())
                })?;
                Ok(vec![text("Patched.")])
            }
            "apply_command" => {
                let command: Command =
                    serde_json::from_value(get(args, "command").ok_or("`command` is required")?.clone())
                        .map_err(|e| format!("not a command: {e}"))?;
                let made = self.command(cx, command)?;
                Ok(vec![json_text(&json!({"created": created(&made)}))])
            }
            "undo" | "redo" => {
                let steps = get(args, "steps").and_then(Value::as_u64).unwrap_or(1).max(1);
                let labels: Vec<String> = self.with(cx, |app, _| {
                    (0..steps)
                        .map_while(|_| {
                            if name == "undo" {
                                app.session_mut().undo()
                            } else {
                                app.session_mut().redo()
                            }
                        })
                        .collect()
                });
                if labels.is_empty() {
                    return Err(format!("nothing to {name}"));
                }
                Ok(vec![text(format!(
                    "{}: {}",
                    if name == "undo" { "Undid" } else { "Redid" },
                    labels.join(", ")
                ))])
            }
            "history" => {
                let (done, undone) = self.app(cx).session().history();
                Ok(vec![json_text(&json!({"done": done, "redoable": undone}))])
            }
            "simulate" => self.simulate(cx, args),
            "screenshot" => self.screenshot(cx, args),
            "render" => self.render(cx, args),
            "open_window" => self.open_window(cx),
            "share_session" => self.share(cx),
            "join_session" => {
                let target = req_str(args, "target")?.to_owned();
                self.with(cx, |app, cx| app.join_session(&target, cx))?;
                // wait for the session's project to arrive
                for _ in 0..400 {
                    if self.with(cx, |app, _| app.pull_collaborators()) {
                        return Ok(vec![text("Joined."), json_text(&self.summary(cx))]);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Ok(vec![text(
                    "Joining… the project has not arrived yet; call get_project in a moment.",
                )])
            }
            _ => Err(format!("no tool {name:?}")),
        }
    }

    fn summary(&self, cx: &App) -> Value {
        let app = self.app(cx);
        describe::project(
            &app.session().project,
            app.session().path.as_deref(),
            &app.simulation_status(),
        )
    }

    fn new_project(&self, cx: &mut App, args: &Args) -> Output {
        let key = req_str(args, "template")?;
        let template = Template::from_key(key).ok_or_else(|| format!("no template {key:?}"))?;
        let name = opt_str(args, "name").map(str::to_owned);
        if self.is_live(cx) {
            let mut project = crate::app::new_project(template);
            if let Some(name) = name {
                project.meta.name = name;
            }
            self.replace_live(cx, "New project", project, None);
        } else {
            self.with(cx, |app, _| {
                app.open_template(template);
                if let Some(name) = name {
                    app.session_mut().project.meta.name = name;
                }
            });
        }
        Ok(vec![json_text(&self.summary(cx))])
    }

    /// Whether the project is shared live (a window or collaborators follow it).
    fn is_live(&self, cx: &App) -> bool {
        self.app(cx).session().collab.is_some()
    }

    /// Replaces a live project through the session, as one undoable edit, so the session (and the
    /// windows following it) carries on with the new project.
    fn replace_live(&self, cx: &mut App, label: &str, project: Project, path: Option<PathBuf>) {
        self.with(cx, |app, _| {
            let session = app.session_mut();
            session.edit(label, move |p| *p = project);
            session.path = path;
            session.selection.clear();
            app.reframe_pattern();
        });
    }

    fn save(&self, cx: &mut App, args: &Args) -> Output {
        let path = match opt_str(args, "path") {
            Some(p) => PathBuf::from(p),
            None => self.app(cx).session().path.clone().unwrap_or_else(|| {
                let project = self.project(cx);
                let slug: String = project
                    .meta
                    .name
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect();
                let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
                home.join(format!(
                    "{}.azam.json",
                    if slug.is_empty() { "untitled".into() } else { slug }
                ))
            }),
        };
        self.with(cx, |app, _| app.session_mut().save(&path))?;
        Ok(vec![text(format!("Saved {}", path.display()))])
    }

    fn export(&self, cx: &mut App, args: &Args) -> Output {
        use crate::export::ExportKind as K;
        let kind = match req_str(args, "format")? {
            "svg" => K::Svg,
            "dxf" => K::Dxf,
            "dxf-graded" => K::DxfGraded,
            "astm" => K::Astm,
            "pattern-pdf" => K::PatternPdf,
            "techpack-pdf" => K::TechPackPdf,
            "marker-svg" => K::MarkerSvg,
            "marker-dxf" => K::MarkerDxf,
            "obj" => K::Obj,
            "gltf" => K::Gltf,
            "fbx" => K::Fbx,
            other => return Err(format!("no export format {other:?}")),
        };
        if matches!(kind, K::Obj | K::Gltf | K::Fbx) {
            self.with(cx, |app, _| app.prepare_scene())?;
        }
        let path = PathBuf::from(req_str(args, "path")?);
        self.app(cx).export_to(kind, &path)?;
        Ok(vec![text(format!("Wrote {}", path.display()))])
    }

    fn create_piece(&self, cx: &mut App, args: &Args) -> Output {
        let shape = |k: &str| -> Result<Option<(Vec2, Vec2)>, String> {
            match get(args, k).and_then(Value::as_array) {
                None => Ok(None),
                Some(b) => {
                    let n: Vec<f64> = b.iter().filter_map(Value::as_f64).collect();
                    if n.len() == 4 {
                        Ok(Some((Vec2::new(n[0], n[1]), Vec2::new(n[2], n[3]))))
                    } else {
                        Err(format!("`{k}` is [x0, y0, x1, y1]"))
                    }
                }
            }
        };
        let points: Option<Vec<(Vec2, bool)>> = match get(args, "points") {
            None => None,
            Some(_) => Some(
                array(args, "points")?
                    .iter()
                    .map(|v| {
                        Ok((
                            vec2(v, "points")?,
                            v.get("curve").and_then(Value::as_bool).unwrap_or(false),
                        ))
                    })
                    .collect::<Result<_, String>>()?,
            ),
        };
        let (rect, ellipse) = (shape("rect")?, shape("ellipse")?);
        let name = opt_str(args, "name").map(str::to_owned);
        let fabric = opt_str(args, "fabric").map(FabricId::new);
        let slot = opt_str(args, "slot").map(str::to_owned);
        let id = self.edit(cx, "Create pattern", |p| {
            let id = if let Some(points) = points {
                if points.iter().filter(|(_, c)| !c).count() < 3 {
                    return Err("a piece needs at least three corners".into());
                }
                azam_doc::create_piece(p, &points)
            } else if let Some((a, b)) = rect {
                azam_doc::create_shape_piece(p, azam_doc::Shape::Rectangle, a, b)
            } else if let Some((a, b)) = ellipse {
                azam_doc::create_shape_piece(p, azam_doc::Shape::Ellipse, a, b)
            } else {
                return Err("give `points`, `rect` or `ellipse`".into());
            }
            .ok_or("could not make that piece (too small, or the outline crosses itself)")?;
            let pc = p.pieces.get_mut(&id).ok_or("the piece vanished")?;
            if let Some(name) = name {
                pc.name = name;
            }
            if let Some(fabric) = fabric {
                if !p.fabrics.contains_key(&fabric) {
                    return Err(format!("no fabric {:?}", fabric.0));
                }
                p.pieces.get_mut(&id).ok_or("the piece vanished")?.fabric = fabric;
            }
            if let Some(slot) = slot {
                p.pieces.get_mut(&id).ok_or("the piece vanished")?.arr.slot = slot;
            }
            Ok(id)
        })?;
        let project = self.project(cx);
        Ok(vec![json_text(&describe::piece(project, &project.pieces[&id]))])
    }

    fn move_points(&self, cx: &mut App, args: &Args) -> Output {
        let project = self.project(cx);
        let piece = req_piece(project, args, "piece")?;
        let pc = &project.pieces[&piece];
        let mut moves = Vec::new();
        let mut curves = Vec::new();
        for entry in array(args, "points")? {
            let o = entry
                .as_object()
                .ok_or("each entry is {point, x?, y?, dx?, dy?, curve?}")?;
            let id = point_id(project, &piece, o.get("point").ok_or("each entry needs `point`")?)?;
            let index = pc.point_index(&id).ok_or("no such point")?;
            let at = pc.pts[index].pos;
            let num = |k: &str| o.get(k).and_then(Value::as_f64);
            let target = Vec2::new(
                num("x").unwrap_or(at.x) + num("dx").unwrap_or(0.0),
                num("y").unwrap_or(at.y) + num("dy").unwrap_or(0.0),
            );
            if target != at {
                moves.push(Command::MovePoints {
                    piece: piece.clone(),
                    points: vec![id],
                    delta: target - at,
                });
            }
            if let Some(curve) = o.get("curve").and_then(Value::as_bool) {
                curves.push((index, curve));
            }
        }
        self.edit(cx, "Move points", |p| {
            if !moves.is_empty() {
                azam_doc::apply(p, Command::Batch(moves)).map_err(|e| e.to_string())?;
            }
            for (index, curve) in curves {
                azam_doc::set_point_curve(p, &piece, index, curve);
            }
            Ok(())
        })?;
        let project = self.project(cx);
        Ok(vec![json_text(&describe::piece(
            project,
            &project.pieces[&piece],
        ))])
    }

    fn curve_segment(&self, cx: &mut App, args: &Args) -> Output {
        let project = self.project(cx);
        let piece = req_piece(project, args, "piece")?;
        let segment = req_segment(project, &piece, args, "segment")?;
        let through = match (get(args, "through"), opt_f64(args, "bulge")?) {
            (Some(v), _) => vec2(v, "through")?,
            (None, Some(bulge)) => {
                let outline = project.pieces[&piece].outline();
                let k = outline.segment_index(&segment).ok_or("no such segment")?;
                let seg = &outline.segments[k];
                let (a, b) = (seg.poly[0], seg.poly[seg.poly.len() - 1]);
                let mid = (a + b) / 2.0;
                let dir = (b - a).normalize_or_zero();
                let mut normal = Vec2::new(-dir.y, dir.x);
                if describe::inside(&outline.poly, mid + normal) {
                    normal = -normal;
                }
                mid + normal * bulge
            }
            (None, None) => return Err("give `through` or `bulge`".into()),
        };
        self.command(
            cx,
            Command::SetCurvature {
                piece: piece.clone(),
                segment,
                through,
            },
        )?;
        let project = self.project(cx);
        Ok(vec![json_text(&describe::piece(
            project,
            &project.pieces[&piece],
        ))])
    }

    fn transform(&self, cx: &mut App, args: &Args) -> Output {
        let project = self.project(cx);
        let pieces = pieces(project, args, "pieces")?;
        let center = if let Some(v) = get(args, "center") {
            vec2(v, "center")?
        } else {
            let mut lo = Vec2::splat(f64::MAX);
            let mut hi = Vec2::splat(f64::MIN);
            for id in &pieces {
                for p in &project.pieces[id].pts {
                    lo = lo.min(p.pos);
                    hi = hi.max(p.pos);
                }
            }
            (lo + hi) / 2.0
        };
        let scale = opt_f64(args, "scale")?.unwrap_or(1.0);
        let scale = Vec2::new(scale, opt_f64(args, "scale_y")?.unwrap_or(scale));
        if scale.x <= 0.0 || scale.y <= 0.0 {
            return Err("scale must be positive (flip_pieces mirrors)".into());
        }
        let angle = opt_f64(args, "rotate")?.unwrap_or(0.0).to_radians();
        let shift = Vec2::new(
            opt_f64(args, "dx")?.unwrap_or(0.0),
            opt_f64(args, "dy")?.unwrap_or(0.0),
        );
        let map = DAffine2::from_translation(shift + center)
            * DAffine2::from_angle(angle)
            * DAffine2::from_scale(scale)
            * DAffine2::from_translation(-center);
        self.command(
            cx,
            Command::TransformPieces {
                pieces,
                transform: PieceTransform::Affine(map),
            },
        )?;
        Ok(vec![text("Transformed.")])
    }

    fn sew(&self, cx: &mut App, args: &Args) -> Output {
        let project = self.project(cx);
        let sides = |k: &str| -> Result<Vec<SeamSide>, String> {
            array(args, k)?
                .iter()
                .map(|v| {
                    let o = v
                        .as_object()
                        .ok_or("each side is {piece, segment, reverse?, from?, to?}")?;
                    let piece = req_piece(project, o, "piece")?;
                    let segment = req_segment(project, &piece, o, "segment")?;
                    let t0 = opt_f64(o, "from")?.unwrap_or(0.0);
                    let t1 = opt_f64(o, "to")?.unwrap_or(1.0);
                    Ok(SeamSide::new(
                        &piece,
                        &segment.0,
                        opt_bool(o, "reverse").unwrap_or(false),
                        t0,
                        t1,
                    ))
                })
                .collect()
        };
        let (a, b) = (sides("a")?, sides("b")?);
        if a.is_empty() || b.is_empty() {
            return Err("both `a` and `b` need at least one side".into());
        }
        let made = self.command(cx, Command::AddSeam { a, b })?;
        let id = made
            .iter()
            .find_map(|c| match c {
                Created::Seam(id) => Some(id.clone()),
                _ => None,
            })
            .ok_or("the seam was not made")?;
        let seam = describe::seams(self.project(cx))
            .into_iter()
            .find(|s| s["id"] == id.0)
            .unwrap_or_default();
        Ok(vec![json_text(&seam)])
    }

    fn set_piece(&self, cx: &mut App, args: &Args) -> Output {
        let piece = req_piece(self.project(cx), args, "piece")?;
        let a = args.clone();
        self.edit(cx, "Piece properties", |p| {
            if let Some(f) = opt_str(&a, "fabric")
                && !p.fabrics.contains_key(&FabricId::new(f))
            {
                return Err(format!("no fabric {f:?}"));
            }
            let pc = p.pieces.get_mut(&piece).ok_or("no such piece")?;
            if let Some(v) = opt_str(&a, "name") {
                v.clone_into(&mut pc.name);
            }
            if let Some(v) = opt_str(&a, "fabric") {
                pc.fabric = FabricId::new(v);
            }
            if let Some(v) = opt_f64(&a, "grain")? {
                pc.grain = v;
            }
            if let Some(v) = get(&a, "layer").and_then(Value::as_i64) {
                pc.layer = i32::try_from(v).map_err(|_| "layer is out of range")?;
            }
            if let Some(v) = get(&a, "state") {
                pc.state = serde_json::from_value(v.clone()).map_err(|_| "state is active, frozen or off")?;
            }
            if let Some(v) = opt_str(&a, "slot") {
                v.clone_into(&mut pc.arr.slot);
                // a new slot starts from where the slot puts it
                pc.arr.moved = None;
                pc.arr.turned = None;
            }
            if let Some(v) = opt_f64(&a, "offset")? {
                pc.arr.off = Some(v);
            }
            if let Some(v) = opt_f64(&a, "height")? {
                pc.arr.h = Some(v);
            }
            if let Some(v) = opt_f64(&a, "along")? {
                pc.arr.t = Some(v);
            }
            if let Some(v) = opt_f64(&a, "angle")? {
                pc.arr.ang = Some(v);
            }
            Ok(())
        })?;
        let project = self.project(cx);
        Ok(vec![json_text(&describe::piece(
            project,
            &project.pieces[&piece],
        ))])
    }

    fn set_fabric(&self, cx: &mut App, args: &Args) -> Output {
        let a = args.clone();
        let id = self.edit(cx, "Fabric", |p| {
            let id = if opt_bool(&a, "new") == Some(true) {
                let preset = opt_str(&a, "preset")
                    .and_then(FabricPreset::from_key)
                    .unwrap_or(FabricPreset::Jersey);
                let fabric = azam_doc::make_fabric(p.id_gen(), preset);
                azam_doc::add_fabric(p, fabric)
            } else {
                match opt_str(&a, "fabric") {
                    Some(f) => FabricId::new(f),
                    None => p
                        .fabrics
                        .keys()
                        .next()
                        .cloned()
                        .ok_or("the project has no fabric")?,
                }
            };
            let fabric = p
                .fabrics
                .get_mut(&id)
                .ok_or_else(|| format!("no fabric {:?}", id.0))?;
            if let Some(name) = opt_str(&a, "name") {
                name.clone_into(&mut fabric.name);
            }
            if let Some(key) = opt_str(&a, "preset") {
                let preset = FabricPreset::from_key(key).ok_or_else(|| format!("no preset {key:?}"))?;
                let spec = preset.spec();
                key.clone_into(&mut fabric.preset);
                spec.weave.clone_into(&mut fabric.weave);
                fabric.rough = spec.rough;
                fabric.sheen = spec.sheen;
                fabric.opacity = spec.opacity.unwrap_or(1.0);
                fabric.phys = spec.physics();
            }
            if let Some(color) = opt_str(&a, "color") {
                let ok = color.len() == 7
                    && color.starts_with('#')
                    && color[1..].chars().all(|c| c.is_ascii_hexdigit());
                if !ok {
                    return Err("colour is #rrggbb".into());
                }
                let base = p.colorways.values().min_by_key(|c| c.order).map(|c| c.id.clone());
                if base.is_none() || base.as_ref() == Some(&p.active_colorway) {
                    if let Some(f) = p.fabrics.get_mut(&id) {
                        color.clone_into(&mut f.color);
                    }
                } else if let Some(cw) = p.colorways.get_mut(&p.active_colorway) {
                    cw.over
                        .entry(id.clone())
                        .or_default()
                        .insert("color".into(), Value::from(color));
                }
            }
            Ok(id)
        })?;
        let project = self.project(cx);
        let f = project.effective_fabric(&id).ok_or("no such fabric")?;
        Ok(vec![json_text(
            &json!({"id": f.id.0, "name": f.name, "preset": f.preset, "color": f.color}),
        )])
    }

    fn set_avatar(&self, cx: &mut App, args: &Args) -> Output {
        let patch = get(args, "measurements")
            .ok_or("`measurements` is required")?
            .clone();
        self.edit(cx, "Avatar", |p| {
            let mut body = serde_json::to_value(p.avatar.body).map_err(|e| e.to_string())?;
            merge_patch(&mut body, &patch);
            p.avatar.body =
                serde_json::from_value(body).map_err(|e| format!("not valid measurements: {e}"))?;
            Ok(())
        })?;
        Ok(vec![json_text(&json!({"avatar": self.project(cx).avatar.body}))])
    }

    fn simulate(&self, cx: &mut App, args: &Args) -> Output {
        let seconds = opt_f64(args, "seconds")?.unwrap_or(5.0).clamp(0.0, 120.0);
        let reset = opt_bool(args, "reset").unwrap_or(false);
        let report: SimReport = self.with(cx, |app, _| {
            if reset {
                app.reset_simulation()?;
            }
            app.simulate(seconds, MAX_SIM_FRAMES)
        })?;
        self.sim_output(cx, args, &report)
    }

    /// Starts the simulation playing in the person's window (`reset` first, if asked) and returns
    /// the simulated time to stop at. [`Self::poll_simulation`] finishes it, so the window keeps
    /// drawing the drape while it runs.
    ///
    /// # Errors
    /// Fails before the avatar exists or without a renderer.
    pub fn start_simulation(&self, cx: &mut App, args: &Args) -> Result<f64, String> {
        let seconds = opt_f64(args, "seconds")?.unwrap_or(5.0).clamp(0.0, 120.0);
        let reset = opt_bool(args, "reset").unwrap_or(false);
        self.with(cx, |app, _| {
            if reset {
                app.reset_simulation()?;
            } else {
                app.prepare_scene()?;
            }
            let until = app.sim_time().unwrap_or(0.0) + seconds;
            app.set_playing(true);
            Ok(until)
        })
    }

    /// Whether the simulation started by [`Self::start_simulation`] reached `until` (or stopped,
    /// or `give_up` is set): then it pauses and returns the tool's result.
    pub fn poll_simulation(&self, cx: &mut App, args: &Args, until: f64, give_up: bool) -> Option<Output> {
        let app = self.app(cx);
        let done = give_up || !app.is_playing() || app.sim_time().unwrap_or(0.0) >= until;
        if !done {
            return None;
        }
        let stopped = (!app.is_playing()).then(|| app.status().to_owned());
        let fit = self.with(cx, |app, _| {
            app.set_playing(false);
            app.fit_summary()
        });
        let time = self.app(cx).sim_time().unwrap_or(0.0);
        let report = SimReport {
            frames: 0,
            time,
            stopped: stopped.or_else(|| give_up.then(|| "took too long; paused".to_owned())),
            fit,
        };
        Some(self.sim_output(cx, args, &report))
    }

    /// The `simulate` tool's result for `report`.
    fn sim_output(&self, cx: &mut App, args: &Args, report: &SimReport) -> Output {
        let mut out = vec![json_text(&json!({
            "simulatedSeconds": (report.time * 100.0).round() / 100.0,
            "stoppedBecause": report.stopped,
            "tightShare": report.fit.map(|f| f.0),
            "overLimitShare": report.fit.map(|f| f.1),
            "status": self.app(cx).simulation_status(),
        }))];
        if opt_bool(args, "show") != Some(false) {
            out.push(png_block(&self.view3d(cx, &Args::new())?));
        }
        Ok(out)
    }

    /// The camera `args` ask for, framed on the avatar and garment.
    fn camera(&self, cx: &mut App, args: &Args) -> Result<OrbitCamera, String> {
        self.with(cx, |app, _| app.prepare_scene())?;
        let mut camera = OrbitCamera::default();
        if let Some((lo, hi)) = self.app(cx).scene_bounds() {
            camera.frame(lo, hi);
        }
        let default = if self.interactive {
            "current"
        } else {
            "three-quarter"
        };
        match opt_str(args, "angle").unwrap_or(default) {
            "current" => camera = self.app(cx).camera(),
            "front" => camera.set_preset(ViewPreset::Front),
            "back" => camera.set_preset(ViewPreset::Back),
            "left" => camera.set_preset(ViewPreset::Left),
            "right" => camera.set_preset(ViewPreset::Right),
            "top" => camera.set_preset(ViewPreset::Top),
            "three-quarter" | "3/4" => {
                camera.yaw = 0.5;
                camera.pitch = 0.1;
            }
            other => return Err(format!("no angle {other:?}")),
        }
        if let Some(yaw) = opt_f64(args, "yaw")? {
            camera.yaw = yaw.to_radians() as f32;
        }
        if let Some(pitch) = opt_f64(args, "pitch")? {
            camera.pitch = pitch.clamp(-89.0, 89.0).to_radians() as f32;
        }
        if let Some(zoom) = opt_f64(args, "zoom")? {
            if zoom <= 0.0 {
                return Err("zoom must be positive".into());
            }
            camera.distance /= zoom as f32;
        }
        Ok(camera)
    }

    /// Image size from `args`, or `default`.
    fn size(args: &Args, default: (u32, u32)) -> (u32, u32) {
        let side = |k: &str, d: u32| {
            get(args, k)
                .and_then(Value::as_u64)
                .map_or(d, |v| (v as u32).clamp(16, MAX_SIDE))
        };
        (side("width", default.0), side("height", default.1))
    }

    /// A PNG of the 3D view as `args` ask.
    fn view3d(&self, cx: &mut App, args: &Args) -> Result<Vec<u8>, String> {
        let camera = self.camera(cx, args)?;
        let (w, h) = Self::size(args, VIEW3D_SIZE);
        let shading = if opt_str(args, "shading") == Some("fitmap") {
            Shading::FitMap
        } else {
            Shading::Solid
        };
        let rgba = self.with(cx, |app, _| app.render_view3d(&camera, w, h, shading, true))?;
        png_rgba(w, h, &rgba)
    }

    fn screenshot(&mut self, cx: &mut App, args: &Args) -> Output {
        if self.begin_screenshot(cx, args)? {
            self.finish_screenshot(cx)
        } else {
            Ok(vec![png_block(&self.view3d(cx, args)?)])
        }
    }

    /// Prepares the window for a screenshot of `view` pattern or window (maximizes the pattern
    /// editor, switches workspace). Returns `false` for the 3D view, which needs no window.
    /// A person's window lays the change out on its next frames: let it draw a few before
    /// [`Self::finish_screenshot`].
    ///
    /// # Errors
    /// Fails for an unknown view, workspace or piece.
    pub fn begin_screenshot(&mut self, cx: &mut App, args: &Args) -> Result<bool, String> {
        if let Some(piece) = get(args, "select") {
            let id = piece_id(self.project(cx), piece)?;
            self.with(cx, |app, _| {
                let sel = &mut app.session_mut().selection;
                sel.pieces.clear();
                sel.pieces.insert(id.clone());
                sel.active = Some(id);
            });
        }
        let keep = self.keep_view(cx);
        let area = match opt_str(args, "view").unwrap_or("3d") {
            "3d" => return Ok(false),
            "pattern" => self.with(cx, |app, _| {
                if app.area_with(Editor::Pattern).is_none() {
                    app.set_workspace(crate::app::Workspace::Design);
                }
                let area = app.area_with(Editor::Pattern);
                if let Some(id) = area {
                    app.toggle_maximize(id);
                }
                area
            }),
            "window" => {
                if let Some(name) = opt_str(args, "workspace") {
                    let ws = crate::app::Workspace::from_name(name)
                        .ok_or_else(|| format!("no workspace {name:?}"))?;
                    self.with(cx, |app, _| app.set_workspace(ws));
                }
                None
            }
            other => return Err(format!("no view {other:?} (3d, pattern, window)")),
        };
        self.pending_shot = Some(PendingShot { keep, area });
        Ok(true)
    }

    /// Captures the window [`Self::begin_screenshot`] prepared and puts the person's view back.
    ///
    /// # Errors
    /// Fails when no screenshot was begun or the image cannot be encoded.
    pub fn finish_screenshot(&mut self, cx: &mut App) -> Output {
        let pending = self.pending_shot.take().ok_or("no screenshot was begun")?;
        let shot = self.draw_window(cx);
        if let Some(id) = pending.area {
            self.with(cx, |app, _| {
                app.toggle_maximize(id);
                app.reframe_pattern();
            });
        }
        self.restore_view(cx, pending.keep);
        Ok(vec![png_block(&shot?)])
    }

    /// What a screenshot may change in the person's window, to put back afterwards.
    fn keep_view(&self, cx: &App) -> Option<KeptView> {
        let app = self.app(cx);
        self.interactive.then(|| KeptView {
            tab: app.active_tab,
            pattern: app.pattern_view,
            framed: app.framed,
            framed_size: app.framed_size,
            status: app.status.clone(),
        })
    }

    /// Puts back what [`Self::keep_view`] kept.
    fn restore_view(&self, cx: &mut App, kept: Option<KeptView>) {
        let Some(kept) = kept else { return };
        self.with(cx, |app, _| {
            if app.active_tab != kept.tab {
                app.activate_tab(kept.tab);
            }
            app.pattern_view = kept.pattern;
            // a view not framed yet (a window just opened) still frames itself
            app.framed = kept.framed;
            app.framed_size = kept.framed_size;
            app.status = kept.status;
        });
    }

    /// The whole window as a PNG, after the frames it takes to lay out and draw.
    fn draw_window(&self, cx: &mut App) -> Result<Vec<u8>, String> {
        let id = self.window.id;
        for _ in 0..3 {
            self.with(cx, |_, cx| cx.notify());
            cx.draw(id);
        }
        let (w, h, pixels) = cx.draw(id);
        let (w, h, rgba) = shrink_rgba(w as u32, h as u32, &xrgb_to_rgba(&pixels), MAX_WINDOW_WIDTH);
        png_rgba(w, h, &rgba)
    }

    fn render(&self, cx: &mut App, args: &Args) -> Output {
        let camera = self.camera(cx, args)?;
        let (w, h) = Self::size(args, VIEW3D_SIZE);
        let samples = get(args, "samples")
            .and_then(Value::as_u64)
            .map_or(64, |s| s.clamp(1, 4096) as u32);
        let (image, stats) = self.with(cx, |app, _| {
            let before = app.camera();
            app.set_camera(camera);
            let out = app.render_still(w, h, samples);
            app.set_camera(before);
            out
        })?;
        Ok(vec![
            png_block(&png_rgba(image.width, image.height, &image.rgba)?),
            text(format!(
                "{w}x{h} at {samples} samples per pixel, {} triangles, {:.1} s",
                stats.triangles,
                stats.trace_s + stats.develop_s
            )),
        ])
    }

    /// Hosts the project as a live session (once); returns its code and address.
    fn host(
        &self,
        cx: &mut App,
        listen: SocketAddr,
        discovery: bool,
    ) -> Result<(String, Option<SocketAddr>), String> {
        if let Some(collab) = &self.app(cx).session().collab {
            let status = collab.status();
            return Ok((status.code.clone(), status.listen));
        }
        let config = azam_sync::Config {
            presence: azam_sync::Presence::named("Claude", azam_sync::random_client_id()),
            listen: Some(listen),
            discovery,
            wake: None,
        };
        let addr = self.with(cx, |app, cx| app.host_session(config, cx))?;
        let code = self
            .app(cx)
            .session()
            .collab
            .as_ref()
            .map(|c| c.status().code.clone())
            .unwrap_or_default();
        Ok((code, addr))
    }

    fn open_window(&mut self, cx: &mut App) -> Output {
        let (_, addr) = self.host(cx, SocketAddr::from(([127, 0, 0, 1], 0)), false)?;
        let port = addr.ok_or("the session is not listening")?.port();
        let bin = azam_binary();
        let child = std::process::Command::new(&bin)
            .args(["--join", &format!("127.0.0.1:{port}")])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                format!(
                    "could not start {}: {e} (set AZAM_BIN to the azam binary)",
                    bin.display()
                )
            })?;
        self.children.push(child);
        Ok(vec![text(format!(
            "Opened an azam window joined live at 127.0.0.1:{port}. Edits made here appear there and edits made there come back here."
        ))])
    }

    fn share(&self, cx: &mut App) -> Output {
        let (code, addr) = self.host(cx, SocketAddr::from(([0, 0, 0, 0], 0)), true)?;
        let port = addr.map(|a| a.port());
        Ok(vec![json_text(&json!({
            "code": code,
            "port": port,
            "howToJoin": format!("In azam: File > Join Session, then enter {code}{}", port.map(|p| format!(" or localhost:{p}")).unwrap_or_default()),
        }))])
    }
}

/// A window screenshot begun and not yet taken.
struct PendingShot {
    /// The person's view, to put back.
    keep: Option<KeptView>,
    /// The area maximized for it.
    area: Option<crate::areas::AreaId>,
}

/// The person's view as a screenshot found it.
struct KeptView {
    tab: usize,
    pattern: crate::pattern::PatternView,
    framed: bool,
    framed_size: Option<(f32, f32)>,
    status: String,
}

/// The `azam` app binary: `AZAM_BIN`, else next to this server, else on the `PATH`.
fn azam_binary() -> PathBuf {
    if let Some(bin) = std::env::var_os("AZAM_BIN") {
        return PathBuf::from(bin);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("azam")))
        .filter(|p| Path::new(p).exists())
        .unwrap_or_else(|| PathBuf::from("azam"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_patch_follows_rfc_7386() {
        let mut v = json!({"a": "b", "c": {"d": "e", "f": "g"}});
        merge_patch(&mut v, &json!({"a": "z", "c": {"f": null}}));
        assert_eq!(v, json!({"a": "z", "c": {"d": "e"}}));
    }

    #[test]
    fn points_inside_a_square_are_found() {
        let sq = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        assert!(describe::inside(&sq, Vec2::new(5.0, 5.0)));
        assert!(!describe::inside(&sq, Vec2::new(15.0, 5.0)));
    }

    #[test]
    fn every_tool_has_a_schema_and_a_unique_name() {
        let tools = list(false);
        let mut names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names.len(), tools.len());
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), tools.len());
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));
    }
}
