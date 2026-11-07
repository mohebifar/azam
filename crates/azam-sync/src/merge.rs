//! Three-way merges of project states, for undoing one collaborator's step without reverting
//! what others did since.

use azam_doc::Project;
use serde_json::{Map, Value};

use crate::model::{elem_id, value_eq};
use crate::repair::repair;

/// Applies the change from `base` to `ours` on top of `theirs`.
///
/// Undo while collaborating: `base` is the project right after the step, `ours` the project
/// before it and `theirs` the current, merged project; the result reverts the step but keeps
/// everyone else's edits. Where both changed the same value, `ours` wins. Objects merge per key;
/// arrays of objects with ids merge per element; other arrays are replaced whole.
///
/// Falls back to `ours` if the merged document does not read as a project.
#[must_use]
pub fn rebase(base: &Project, ours: &Project, theirs: &Project) -> Project {
    let merged = merge3(&base.to_value(), &ours.to_value(), &theirs.to_value());
    let mut project = serde_json::from_value::<Project>(merged.clone())
        .or_else(|_| Project::load(merged))
        .unwrap_or_else(|_| ours.clone());
    repair(&mut project);
    project.ids = theirs.ids.clone();
    project
}

/// Three-way merge of JSON values (see [`rebase`]).
#[must_use]
pub fn merge3(base: &Value, ours: &Value, theirs: &Value) -> Value {
    if value_eq(ours, base) {
        return theirs.clone();
    }
    if value_eq(theirs, base) || value_eq(theirs, ours) {
        return ours.clone();
    }
    match (base, ours, theirs) {
        (Value::Object(b), Value::Object(o), Value::Object(t)) => Value::Object(merge_maps(b, o, t)),
        (Value::Array(b), Value::Array(o), Value::Array(t)) if all_ids(b) && all_ids(o) && all_ids(t) => {
            Value::Array(merge_lists(b, o, t))
        }
        _ => ours.clone(),
    }
}

fn merge_maps(b: &Map<String, Value>, o: &Map<String, Value>, t: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, tv) in t {
        match (b.get(k), o.get(k)) {
            (Some(bv), Some(ov)) => {
                out.insert(k.clone(), merge3(bv, ov, tv));
            }
            // We removed it; keep it only if they changed it meanwhile.
            (Some(bv), None) => {
                if !value_eq(bv, tv) {
                    out.insert(k.clone(), tv.clone());
                }
            }
            (None, Some(ov)) => {
                out.insert(k.clone(), ov.clone());
            }
            (None, None) => {
                out.insert(k.clone(), tv.clone());
            }
        }
    }
    for (k, ov) in o {
        if t.contains_key(k) {
            continue;
        }
        // They removed it: bring it back only if we added or changed it.
        if b.get(k).is_none_or(|bv| !value_eq(bv, ov)) {
            out.insert(k.clone(), ov.clone());
        }
    }
    out
}

fn all_ids(items: &[Value]) -> bool {
    !items.is_empty() && items.iter().all(|v| elem_id(v).is_some())
}

fn find<'a>(items: &'a [Value], id: &str) -> Option<&'a Value> {
    items.iter().find(|v| elem_id(v) == Some(id))
}

fn merge_lists(b: &[Value], o: &[Value], t: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(t.len().max(o.len()));
    for tv in t {
        let id = elem_id(tv).unwrap_or_default();
        match (find(b, id), find(o, id)) {
            (Some(bv), Some(ov)) => out.push(merge3(bv, ov, tv)),
            (Some(bv), None) => {
                if !value_eq(bv, tv) {
                    out.push(tv.clone());
                }
            }
            (None, Some(ov)) => out.push(ov.clone()),
            (None, None) => out.push(tv.clone()),
        }
    }
    // Elements we added (or changed while they removed them) go after their predecessor in ours.
    for (i, ov) in o.iter().enumerate() {
        let id = elem_id(ov).unwrap_or_default();
        if find(t, id).is_some() || find(b, id).is_some_and(|bv| value_eq(bv, ov)) {
            continue;
        }
        let at = o[..i]
            .iter()
            .rev()
            .find_map(|prev| {
                let pid = elem_id(prev)?;
                out.iter().position(|v| elem_id(v) == Some(pid))
            })
            .map_or(0, |p| p + 1);
        out.insert(at, ov.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn undo_keeps_other_edits() {
        let before =
            json!({ "name": "a", "show": true, "pts": [{ "id": "p", "x": 1 }, { "id": "q", "x": 2 }] });
        let after = json!({ "name": "b", "show": true, "pts": [{ "id": "p", "x": 1 }, { "id": "q", "x": 2 }, { "id": "r", "x": 3 }] });
        let now = json!({ "name": "b", "show": false, "pts": [{ "id": "p", "x": 5 }, { "id": "q", "x": 2 }, { "id": "r", "x": 3 }] });
        let undone = merge3(&after, &before, &now);
        assert_eq!(
            undone,
            json!({ "name": "a", "show": false, "pts": [{ "id": "p", "x": 5 }, { "id": "q", "x": 2 }] })
        );
        let redone = merge3(&before, &after, &undone);
        assert_eq!(redone, now);
    }
}
