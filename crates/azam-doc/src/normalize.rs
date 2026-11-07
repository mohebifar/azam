//! Loading stored projects: repairing the loose shapes a sync backend may hand back (collections
//! stored as keyed objects, missing collections, `null` entries) before reading them into a
//! [`Project`].

use std::cmp::Ordering;

use serde_json::{Map, Value, json};

use crate::avatar::{Avatar, Sex};
use crate::piece::{DEFAULT_ARRANGEMENT_OFFSET_MM, DEFAULT_SEAM_ALLOWANCE_MM, DEFAULT_SLOT};
use crate::project::{DEFAULT_BASE_SIZE, MAIN_COLORWAY, MAIN_COLORWAY_NAME, Project, UNTITLED};

/// Top-level collections every project has.
const COLLECTIONS: [&str; 10] = [
    "pieces",
    "sewing",
    "fabrics",
    "buttons",
    "holes",
    "stitches",
    "poms",
    "comments",
    "colorways",
    "graphics",
];

/// Repairs a stored project so it has every collection and setting in its expected shape.
///
/// Lists stored as objects keyed by index become arrays (ordered by key), `null` pieces and seams
/// are dropped, missing settings get their defaults and every piece and seam records its own key
/// as its id. Unknown fields are left alone.
///
/// Two inputs are repaired further than strictly needed, because the typed model has no way to
/// hold them: a top-level collection stored as an array becomes an object keyed by index, and
/// `null` points of measure are dropped.
#[must_use]
pub fn normalize_value(doc: Value) -> Value {
    let mut doc = match doc {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    for key in COLLECTIONS {
        let repaired = match doc.get_mut(key).map(Value::take) {
            Some(Value::Object(map)) => map,
            Some(Value::Array(items)) => items
                .into_iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect(),
            _ => Map::new(),
        };
        doc.insert(key.to_owned(), Value::Object(repaired));
    }
    if is_falsy(doc.get("meta")) {
        doc.insert("meta".to_owned(), json!({ "name": UNTITLED }));
    }
    normalize_avatar(&mut doc);

    let mut sizes = as_array(doc.get("sizes"));
    if sizes.is_empty() {
        sizes.push(json!(DEFAULT_BASE_SIZE));
    }
    let first_size = sizes[0].clone();
    doc.insert("sizes".to_owned(), Value::Array(sizes));
    default_if_falsy(&mut doc, "baseSize", first_size);
    let base = doc.get("baseSize").cloned().unwrap_or(Value::Null);
    default_if_falsy(&mut doc, "activeSize", base);

    let colorways_empty = doc
        .get("colorways")
        .and_then(Value::as_object)
        .is_none_or(Map::is_empty);
    if colorways_empty {
        doc.insert(
            "colorways".to_owned(),
            json!({ MAIN_COLORWAY: { "id": MAIN_COLORWAY, "name": MAIN_COLORWAY_NAME, "order": 0, "over": {} } }),
        );
    }
    let active_ok = match doc.get("activeColorway") {
        Some(Value::String(id)) if !id.is_empty() => doc["colorways"]
            .get(id.as_str())
            .is_some_and(|cw| !is_falsy(Some(cw))),
        _ => false,
    };
    if !active_ok {
        let first = doc["colorways"]
            .as_object()
            .and_then(|m| m.keys().next().cloned())
            .map_or(Value::Null, Value::String);
        doc.insert("activeColorway".to_owned(), first);
    }
    default_if_falsy(&mut doc, "units", json!("cm"));
    default_if_falsy(
        &mut doc,
        "sim",
        json!({ "quality": "normal", "selfCollision": true }),
    );

    if let Some(Value::Object(pieces)) = doc.get_mut("pieces") {
        retain_objects(pieces);
        for (key, piece) in pieces.iter_mut() {
            if let Value::Object(pc) = piece {
                normalize_piece(key, pc);
            }
        }
    }
    if let Some(Value::Object(sewing)) = doc.get_mut("sewing") {
        retain_objects(sewing);
        for (key, seam) in sewing.iter_mut() {
            if let Value::Object(sw) = seam {
                sw.insert("id".to_owned(), Value::String(key.clone()));
                for group in ["a", "b"] {
                    let sides = as_array(sw.get(group));
                    sw.insert(group.to_owned(), Value::Array(sides));
                }
            }
        }
    }
    if let Some(Value::Object(poms)) = doc.get_mut("poms") {
        retain_objects(poms);
        for pom in poms.values_mut() {
            if let Value::Object(pm) = pom {
                let refs = as_array(pm.get("refs"));
                pm.insert("refs".to_owned(), Value::Array(refs));
            }
        }
    }
    Value::Object(doc)
}

fn normalize_avatar(doc: &mut Map<String, Value>) {
    let stored = doc.get("avatar").filter(|v| !is_falsy(Some(v))).cloned();
    let sex = match stored.as_ref().and_then(|a| a.get("sex")).and_then(Value::as_str) {
        Some("male") => Sex::Male,
        _ => Sex::Female,
    };
    let Ok(Value::Object(mut avatar)) = serde_json::to_value(Avatar::preset(sex)) else {
        return;
    };
    if let Some(Value::Object(stored)) = stored {
        for (k, v) in stored {
            avatar.insert(k, v);
        }
    }
    doc.insert("avatar".to_owned(), Value::Object(avatar));
}

fn normalize_piece(key: &str, pc: &mut Map<String, Value>) {
    pc.insert("id".to_owned(), Value::String(key.to_owned()));
    let pts = as_array(pc.get("pts"));
    pc.insert("pts".to_owned(), Value::Array(pts));
    let mut internals = as_array(pc.get("internals"));
    for line in &mut internals {
        if let Value::Object(il) = line {
            let pts = as_array(il.get("pts"));
            il.insert("pts".to_owned(), Value::Array(pts));
        }
    }
    pc.insert("internals".to_owned(), Value::Array(internals));
    for list in ["notches", "pins", "graphics"] {
        let items = as_array(pc.get(list));
        pc.insert(list.to_owned(), Value::Array(items));
    }
    default_if_falsy(
        pc,
        "arr",
        json!({ "slot": DEFAULT_SLOT, "off": DEFAULT_ARRANGEMENT_OFFSET_MM }),
    );
    if pc.get("sa").is_none_or(Value::is_null) {
        pc.insert("sa".to_owned(), json!(DEFAULT_SEAM_ALLOWANCE_MM));
    }
}

/// Collections whose entries record their key as `id`.
const KEYED_COLLECTIONS: [&str; 7] = [
    "fabrics",
    "buttons",
    "holes",
    "stitches",
    "poms",
    "comments",
    "colorways",
];

/// Gives entries of keyed collections that have no id their key as id, so they can be read.
fn fill_missing_ids(doc: &mut Value) {
    for key in KEYED_COLLECTIONS {
        if let Some(Value::Object(items)) = doc.get_mut(key) {
            for (k, item) in items.iter_mut() {
                if let Value::Object(fields) = item
                    && !fields.contains_key("id")
                {
                    fields.insert("id".to_owned(), Value::String(k.clone()));
                }
            }
        }
    }
}

/// Drops entries that are not objects (`null` above all).
fn retain_objects(map: &mut Map<String, Value>) {
    map.retain(|_, v| v.is_object());
}

/// Sets `key` to `value` when it is missing or falsy.
fn default_if_falsy(map: &mut Map<String, Value>, key: &str, value: Value) {
    if is_falsy(map.get(key)) {
        map.insert(key.to_owned(), value);
    }
}

/// Whether a stored value counts as unset: missing, `null`, `false`, `0` or `""`.
fn is_falsy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_f64().is_none_or(|x| x == 0.0),
        Some(Value::String(s)) => s.is_empty(),
        Some(_) => false,
    }
}

/// A stored list: arrays lose their `null` entries; objects keyed by index become arrays in key
/// order; anything else is empty.
fn as_array(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(items)) => items.iter().filter(|x| !x.is_null()).cloned().collect(),
        Some(Value::Object(map)) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| utf16_order(a, b));
            keys.into_iter().map(|k| map[k].clone()).collect()
        }
        _ => Vec::new(),
    }
}

/// String order by UTF-16 code units, the order stored keys were sorted in.
fn utf16_order(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Errors when reading a stored project.
#[derive(Debug)]
pub struct LoadError(serde_json::Error);

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid project: {}", self.0)
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl Project {
    /// Reads a stored project, repairing loose shapes first (see [`normalize_value`]). Entries of
    /// keyed collections without an id get their key as id.
    ///
    /// # Errors
    /// Returns an error if a field has the wrong type after repair.
    pub fn load(value: Value) -> Result<Self, LoadError> {
        let mut doc = normalize_value(value);
        fill_missing_ids(&mut doc);
        serde_json::from_value(doc).map_err(LoadError)
    }

    /// Reads a stored project from JSON text.
    ///
    /// # Errors
    /// Returns an error if the text is not JSON or a field has the wrong type after repair.
    pub fn from_json(text: &str) -> Result<Self, LoadError> {
        Self::load(serde_json::from_str(text).map_err(LoadError)?)
    }

    /// The project as a JSON value, in the stored format.
    #[must_use]
    pub fn to_value(&self) -> Value {
        // Every field is plain data with string keys, so serialization cannot fail.
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// The project as JSON text, in the stored format.
    #[must_use]
    pub fn to_json(&self) -> String {
        self.to_value().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_lists_become_arrays_in_key_order() {
        let v = json!({ "b": 2, "a": 1, "10": 3 });
        assert_eq!(as_array(Some(&v)), vec![json!(3), json!(1), json!(2)]);
        assert_eq!(as_array(Some(&json!([1, null, 2]))), vec![json!(1), json!(2)]);
    }

    #[test]
    fn collections_stored_as_arrays_become_keyed_objects() {
        let v = normalize_value(json!({ "buttons": [{ "id": "b" }], "pieces": null }));
        assert_eq!(v["buttons"], json!({ "0": { "id": "b" } }));
        assert_eq!(v["pieces"], json!({}));
        let p = Project::load(json!({ "poms": { "pm": { "refs": [] } } })).expect("loads");
        assert_eq!(p.poms[0].id.as_str(), "pm");
    }

    #[test]
    fn empty_input_becomes_a_loadable_project() {
        let p = Project::load(Value::Null).expect("defaults fill every required field");
        assert_eq!(p.meta.name, UNTITLED);
        assert_eq!(p.sizes, vec!["M".into()]);
        assert_eq!(p.active_colorway.as_str(), MAIN_COLORWAY);
    }
}
