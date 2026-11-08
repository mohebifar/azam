//! Shared helpers for the sync tests: templates and random edits made with azam-doc's commands.

#![allow(dead_code, reason = "each test binary uses a subset of the helpers")]
#![allow(
    clippy::cmp_owned,
    reason = "JSON text comparison is the exact, order-sensitive check"
)]

use azam_doc::{
    Command, FlipAxis, ItemKind, PieceTransform, Project, SeamSide, Sex, Template, add_button, add_comment,
    add_stitch, apply, from_template, move_item,
};
use azam_geom::Vec2;

/// A project drafted from `template`, at a fixed time so runs are reproducible.
pub fn template(template: Template) -> Project {
    let sex = Sex::Female;
    from_template(template, sex, None, None, 1_700_000_000_000.0)
}

/// One random edit, described by numbers the property tests generate.
#[derive(Clone, Copy, Debug)]
pub struct Op {
    pub kind: u8,
    pub a: u32,
    pub b: u32,
    pub x: f64,
    pub y: f64,
}

fn pick<T: Clone>(items: &[T], i: u32) -> Option<T> {
    (!items.is_empty()).then(|| items[i as usize % items.len()].clone())
}

/// Applies `op` to `project` with azam-doc's edit functions. Edits the document refuses are
/// skipped. Returns a label for the edit that ran.
#[allow(clippy::too_many_lines, reason = "one arm per kind of edit")]
pub fn apply_op(project: &mut Project, op: Op) -> &'static str {
    let pieces: Vec<_> = project.pieces.keys().cloned().collect();
    let piece = pick(&pieces, op.a);
    let points = |p: &azam_doc::PieceId| -> Vec<azam_doc::PointId> {
        project
            .pieces
            .get(p)
            .map(|pc| pc.pts.iter().map(|q| q.id.clone()).collect())
            .unwrap_or_default()
    };
    let corners = |p: &azam_doc::PieceId| -> Vec<azam_doc::PointId> {
        project
            .pieces
            .get(p)
            .map(|pc| pc.pts.iter().filter(|q| !q.curve).map(|q| q.id.clone()).collect())
            .unwrap_or_default()
    };
    let delta = Vec2::new(op.x, op.y);
    let command = match (op.kind % 18, piece) {
        (0 | 1, Some(p)) => pick(&points(&p), op.b).map(|pt| Command::MovePoints {
            piece: p,
            points: vec![pt],
            delta,
        }),
        (2, Some(p)) => Some(Command::TransformPieces {
            pieces: vec![p],
            transform: PieceTransform::Translate(delta),
        }),
        (3, Some(p)) => pick(&corners(&p), op.b).map(|s| Command::SplitSegment {
            piece: p,
            segment: s,
            t: 0.2 + (op.x.abs() % 0.6),
        }),
        (4, Some(p)) => pick(&points(&p), op.b).map(|pt| Command::DeletePoints {
            piece: p,
            points: vec![pt],
        }),
        (5, Some(p)) => Some(Command::DuplicatePiece { piece: p }),
        (6, Some(p)) => Some(Command::MirrorPiece {
            piece: p,
            linked: op.b % 2 == 0,
        }),
        (7, Some(p)) if pieces.len() > 1 => Some(Command::DeletePieces { pieces: vec![p] }),
        (8, Some(p)) => {
            let q = pick(&pieces, op.b).unwrap_or_else(|| p.clone());
            match (pick(&corners(&p), op.b), pick(&corners(&q), op.a)) {
                (Some(s), Some(t)) if p != q || s != t => Some(Command::AddSeam {
                    a: vec![SeamSide::segment(&p, &s.0, false)],
                    b: vec![SeamSide::segment(&q, &t.0, true)],
                }),
                _ => None,
            }
        }
        (9, _) => {
            let seams: Vec<_> = project.sewing.keys().cloned().collect();
            pick(&seams, op.b).map(|seam| Command::DeleteSeam { seam })
        }
        (10, Some(p)) => pick(&corners(&p), op.b).map(|s| Command::SetSegmentAllowance {
            piece: p,
            segment: s,
            mm: (op.x.abs() % 30.0).round(),
        }),
        (11, Some(p)) => pick(&corners(&p), op.b).map(|s| Command::SetHem {
            piece: p,
            segment: s,
            mm: (op.y % 40.0).round(),
        }),
        (12, _) => Some(Command::Rename {
            name: format!("Garment {}", op.b % 100),
        }),
        (13, _) => pick(&project.sizes, op.b).map(|size| Command::SetActiveSize { size }),
        (14, Some(p)) => Some(Command::FlipPieces {
            pieces: vec![p],
            axis: if op.b % 2 == 0 {
                FlipAxis::Horizontal
            } else {
                FlipAxis::Vertical
            },
        }),
        (15, _) => Some(Command::CreatePiece {
            points: vec![
                (Vec2::new(op.x, op.y), false),
                (Vec2::new(op.x + 100.0, op.y), false),
                (Vec2::new(op.x + 50.0, op.y + 80.0), op.b % 3 == 0),
                (Vec2::new(op.x, op.y + 90.0), false),
            ],
        }),
        (16, Some(p)) => {
            match op.b % 3 {
                0 => {
                    add_button(project, &p, delta);
                }
                1 => {
                    add_comment(project, &p, delta, "check fit", 1_700_000_000_000.0);
                }
                _ => {
                    let seg = pick(&corners(&p), op.a);
                    add_stitch(project, &p, seg.as_ref());
                }
            }
            return "parts";
        }
        (17, _) => {
            let buttons: Vec<_> = project.buttons.keys().cloned().collect();
            if let Some(b) = pick(&buttons, op.b) {
                move_item(project, ItemKind::Button, &b, delta);
            }
            return "move item";
        }
        _ => None,
    };
    match command {
        Some(c) => {
            let label = c.label();
            if apply(project, c).is_ok() {
                label
            } else {
                "refused"
            }
        }
        None => "none",
    }
}

/// Path and values of the first difference between two JSON values (key order counts).
pub fn first_diff(a: &serde_json::Value, b: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let kx: Vec<_> = x.keys().collect();
            let ky: Vec<_> = y.keys().collect();
            if kx != ky {
                return Some(format!("keys {kx:?} vs {ky:?}"));
            }
            x.iter()
                .find_map(|(k, v)| first_diff(v, &y[k]).map(|d| format!("{k}.{d}")))
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("len {} vs {}: {a} vs {b}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (v, w))| first_diff(v, w).map(|d| format!("[{i}].{d}")))
        }
        _ => (a != b || a.to_string() != b.to_string()).then(|| format!("{a} vs {b}")),
    }
}
