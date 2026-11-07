//! The CRDT model of a project: the project's stored JSON mirrored into shared types.
//!
//! Mapping:
//! - every JSON object is a shared map; collections keyed by id (pieces, seams, fabrics,
//!   colorways…) are therefore maps keyed by id, and struct fields are map entries, so edits to
//!   different entries (or different fields of one entry) merge;
//! - every JSON array is a shared array; elements that are objects stay shared maps, so a point of
//!   an outline is a map inside the outline's array and concurrent edits to different points (or
//!   to x and y of one point) merge, while inserts and deletes merge as list edits;
//! - numbers, strings, booleans and null are plain values. Integers and floats are kept apart so
//!   the project reads back with the same number types.
//!
//! Shared maps do not keep key order, but the stored format does (the first colorway, the order
//! extra fields were written in). A map whose key order is not sorted gets a hidden entry
//! [`ORDER_KEY`] with its keys in order; materializing lists those keys first and any others after
//! in sorted order, so a concurrent edit that loses a race on the order list still reads back
//! deterministically.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use azam_doc::{IdGen, Project};
use serde_json::{Map, Number, Value};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Any, Array, ArrayRef, Doc, In, Map as _, MapPrelim, MapRef, Out, ReadTxn, StateVector, Transact,
    TransactionMut, Update,
};

use crate::repair::{dedupe_ids, repair};

/// Name of the root shared map holding the project.
const ROOT: &str = "project";
/// Hidden map entry listing the map's keys in order (absent when the keys are sorted).
pub const ORDER_KEY: &str = "\u{0}order";
/// Largest `old × new` middle section of an array diffed by longest common subsequence; larger
/// sections are matched by position only.
const LCS_LIMIT: usize = 250_000;
/// Width of the id-number range each replica allocates new ids from.
const ID_RANGE: u64 = 1_000_000_000;
/// Number of distinct id ranges replicas pick from.
const ID_SLOTS: u64 = 999_983;

/// Errors of the sync layer.
#[derive(Debug)]
pub enum SyncError {
    /// Bytes that do not decode as an update, state vector or message.
    Decode(String),
    /// An update that could not be integrated.
    Update(String),
    /// The shared document does not read back as a project.
    Project(String),
    /// A file or socket error.
    Io(std::io::Error),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "could not decode sync data: {e}"),
            Self::Update(e) => write!(f, "could not apply update: {e}"),
            Self::Project(e) => write!(f, "shared document is not a project: {e}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// One replica of a shared project.
///
/// Local edits go in through [`SyncDoc::apply_project`], which diffs the new project against the
/// replica and records only the changed entries; remote edits come in through
/// [`SyncDoc::apply_update`] and read back with [`SyncDoc::to_project`].
pub struct SyncDoc {
    doc: Doc,
    root: MapRef,
    /// The document as JSON, as of the last local or remote change. Diffs start from it.
    shadow: Map<String, Value>,
    /// Highest id number handed out from this replica's range.
    ids_high: Cell<u64>,
}

impl fmt::Debug for SyncDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncDoc")
            .field("client_id", &self.client_id())
            .finish_non_exhaustive()
    }
}

impl Default for SyncDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncDoc {
    /// An empty replica with a random client id.
    #[must_use]
    pub fn new() -> Self {
        Self::with_client_id(random_client_id())
    }

    /// An empty replica with client id `client_id` (below 2^53, unique among collaborators).
    #[must_use]
    pub fn with_client_id(client_id: u64) -> Self {
        let doc = Doc::with_client_id(client_id & ((1 << 53) - 1));
        let root = doc.get_or_insert_map(ROOT);
        Self {
            doc,
            root,
            shadow: Map::new(),
            ids_high: Cell::new(0),
        }
    }

    /// A replica holding `project`, with a random client id.
    #[must_use]
    pub fn from_project(project: &Project) -> Self {
        let mut doc = Self::new();
        doc.apply_project(project);
        doc
    }

    /// The replica's client id.
    #[must_use]
    pub fn client_id(&self) -> u64 {
        self.doc.client_id().get()
    }

    /// Whether the replica holds no project yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shadow.is_empty()
    }

    /// Records the changes that turn the replica into `project` and returns them as an update
    /// (v1 encoding), or `None` when nothing changed.
    pub fn apply_project(&mut self, project: &Project) -> Option<Vec<u8>> {
        let Value::Object(new) = project.to_value() else {
            return None;
        };
        if map_eq(&self.shadow, &new) {
            return None;
        }
        let update = {
            let mut txn = self.doc.transact_mut();
            sync_map(&mut txn, &self.root, &self.shadow, &new);
            txn.encode_update_v1()
        };
        self.shadow = new;
        Some(update)
    }

    /// Integrates an update (v1 encoding) from another replica. Returns whether the document
    /// changed.
    ///
    /// # Errors
    /// Fails if the bytes are not an update or cannot be integrated; the replica is unchanged.
    pub fn apply_update(&mut self, update: &[u8]) -> Result<bool, SyncError> {
        self.apply_updates(std::iter::once(update))
    }

    /// Integrates several updates, reading the document back once at the end. Returns whether the
    /// document changed.
    ///
    /// # Errors
    /// Fails on the first update that does not decode or integrate; earlier ones stay applied.
    pub fn apply_updates<'a>(
        &mut self,
        updates: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<bool, SyncError> {
        let result = (|| {
            let mut txn = self.doc.transact_mut();
            for bytes in updates {
                let update = Update::decode_v1(bytes).map_err(|e| SyncError::Decode(e.to_string()))?;
                txn.apply_update(update)
                    .map_err(|e| SyncError::Update(e.to_string()))?;
            }
            Ok(())
        })();
        let before = std::mem::take(&mut self.shadow);
        self.refresh();
        let changed = !map_eq(&before, &self.shadow);
        result.map(|()| changed)
    }

    /// The document as stored JSON.
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::Object(self.shadow.clone())
    }

    /// The document as a project, repaired so every reference resolves (see
    /// [`crate::repair()`]), with elements that concurrent moves duplicated removed, and with an id generator that allocates from this replica's own id
    /// range, so ids created concurrently on different replicas never collide.
    ///
    /// # Errors
    /// Fails if the replica is empty or the document does not read as a project.
    pub fn to_project(&self) -> Result<Project, SyncError> {
        if self.shadow.is_empty() {
            return Err(SyncError::Project("the shared document is empty".to_owned()));
        }
        let mut value = self.to_value();
        dedupe_ids(&mut value);
        let mut project = match serde_json::from_value::<Project>(value.clone()) {
            Ok(p) => p,
            Err(_) => Project::load(value).map_err(|e| SyncError::Project(e.to_string()))?,
        };
        repair(&mut project);
        self.claim_ids(&mut project);
        Ok(project)
    }

    /// Points `project`'s id generator at this replica's own id range, past every id already
    /// allocated from it.
    pub fn claim_ids(&self, project: &mut Project) {
        let base = id_range_base(self.client_id());
        let range = base..base + ID_RANGE;
        let current = Some(project.ids.last()).filter(|l| project.ids.is_seeded() && range.contains(l));
        let mut last = self.ids_high.get().max(current.unwrap_or(0));
        if last < base {
            last = max_sequence_in(&project.to_value(), base, base + ID_RANGE).unwrap_or(base);
        }
        project.ids = IdGen::after(last);
        self.ids_high.set(last);
    }

    /// The replica's state vector (v1 encoding): what it has seen of every client.
    #[must_use]
    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.transact().state_vector().encode_v1()
    }

    /// The whole document as one update (v1 encoding).
    #[must_use]
    pub fn encode_state(&self) -> Vec<u8> {
        self.doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    }

    /// The changes a replica with state vector `state_vector` (v1 encoding) has not seen.
    ///
    /// # Errors
    /// Fails if the bytes are not a state vector.
    pub fn encode_diff(&self, state_vector: &[u8]) -> Result<Vec<u8>, SyncError> {
        let sv = StateVector::decode_v1(state_vector).map_err(|e| SyncError::Decode(e.to_string()))?;
        Ok(self.doc.transact().encode_diff_v1(&sv))
    }

    /// Re-reads the shared document into the shadow after remote changes.
    fn refresh(&mut self) {
        let txn = self.doc.transact();
        self.shadow = map_to_json(&txn, &self.root);
    }
}

/// A random client id below 2^53.
#[must_use]
pub fn random_client_id() -> u64 {
    yrs::ClientID::random().get()
}

/// First id number of the range replica `client_id` allocates from.
fn id_range_base(client_id: u64) -> u64 {
    (client_id % ID_SLOTS + 1) * ID_RANGE
}

/// Largest sequence number of a generated-looking id (`<letters>_<digits>`) in `lo..hi` anywhere
/// in `value`.
fn max_sequence_in(value: &Value, lo: u64, hi: u64) -> Option<u64> {
    let of = |s: &str| sequence_of(s).filter(|n| (lo..hi).contains(n));
    match value {
        Value::String(s) => of(s),
        Value::Array(items) => items.iter().filter_map(|v| max_sequence_in(v, lo, hi)).max(),
        Value::Object(map) => map
            .iter()
            .filter_map(|(k, v)| of(k).max(max_sequence_in(v, lo, hi)))
            .max(),
        _ => None,
    }
}

/// The sequence number of `id` if it looks generated.
fn sequence_of(id: &str) -> Option<u64> {
    let (kind, number) = id.split_once('_')?;
    let generated = !kind.is_empty()
        && kind.bytes().all(|b| b.is_ascii_lowercase())
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit());
    if generated { number.parse().ok() } else { None }
}

// ---------------------------------------------------------------------------------------------
// JSON → shared types.

/// A JSON value as a shared-type input.
fn to_in(v: &Value) -> In {
    match v {
        Value::Object(map) => In::Map(map_prelim(map)),
        Value::Array(items) => In::Array(items.iter().map(to_in).collect()),
        other => In::Any(to_any(other)),
    }
}

/// A JSON object as a shared map input, with its key order when the keys are not sorted.
fn map_prelim(map: &Map<String, Value>) -> MapPrelim {
    let mut prelim: MapPrelim = map.iter().map(|(k, v)| (k.as_str(), to_in(v))).collect();
    if !is_sorted(map.keys()) {
        prelim.insert(Arc::from(ORDER_KEY), order_in(map));
    }
    prelim
}

/// The key-order list of `map`.
fn order_in(map: &Map<String, Value>) -> In {
    In::Array(map.keys().map(|k| In::Any(Any::from(k.as_str()))).collect())
}

/// A JSON scalar as a plain value. Integers stay integers; negative zero is stored as zero (the
/// update encoding cannot tell them apart).
fn to_any(v: &Value) -> Any {
    match v {
        Value::Null | Value::Object(_) | Value::Array(_) => Any::Null,
        Value::Bool(b) => Any::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Any::BigInt(i)
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                Any::Number(if f == 0.0 { 0.0 } else { f })
            }
        }
        Value::String(s) => Any::from(s.as_str()),
    }
}

fn is_sorted<'a>(keys: impl IntoIterator<Item = &'a String>) -> bool {
    let mut prev: Option<&String> = None;
    for k in keys {
        if prev.is_some_and(|p| p > k) {
            return false;
        }
        prev = Some(k);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Diffing: record the changes from `old` to `new` in the shared types that currently hold `old`.

/// Order-sensitive deep equality (serde's map equality ignores key order).
pub(crate) fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => map_eq(a, b),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| value_eq(x, y))
        }
        _ => a == b,
    }
}

fn map_eq(a: &Map<String, Value>, b: &Map<String, Value>) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((ka, va), (kb, vb))| ka == kb && value_eq(va, vb))
}

fn sync_map(txn: &mut TransactionMut, map: &MapRef, old: &Map<String, Value>, new: &Map<String, Value>) {
    for (k, nv) in new {
        match old.get(k) {
            Some(ov) if value_eq(ov, nv) => {}
            Some(ov) => sync_entry(txn, map, k, ov, nv),
            None => {
                map.insert(txn, k.as_str(), to_in(nv));
            }
        }
    }
    for k in old.keys() {
        if !new.contains_key(k) {
            map.remove(txn, k);
        }
    }
    if !old.keys().eq(new.keys()) {
        sync_order(txn, map, new);
    }
}

fn sync_entry(txn: &mut TransactionMut, map: &MapRef, key: &str, old: &Value, new: &Value) {
    match (old, new, map.get(txn, key)) {
        (Value::Object(o), Value::Object(n), Some(Out::YMap(child))) => sync_map(txn, &child, o, n),
        (Value::Array(o), Value::Array(n), Some(Out::YArray(child))) => sync_array(txn, &child, o, n),
        _ => {
            map.insert(txn, key, to_in(new));
        }
    }
}

/// Brings the key-order list of `map` in line with `new`'s key order.
fn sync_order(txn: &mut TransactionMut, map: &MapRef, new: &Map<String, Value>) {
    if let Some(Out::YArray(order)) = map.get(txn, ORDER_KEY) {
        let current: Vec<Value> = order.iter(txn).map(|o| out_to_json(txn, o)).collect();
        let wanted: Vec<Value> = new.keys().map(|k| Value::from(k.as_str())).collect();
        sync_array(txn, &order, &current, &wanted);
    } else if !is_sorted(new.keys()) {
        map.insert(txn, ORDER_KEY, order_in(new));
    }
}

/// Id of an object element, used to match array elements across versions.
pub(crate) fn elem_id(v: &Value) -> Option<&str> {
    v.as_object()?.get("id")?.as_str()
}

/// Whether `a` and `b` are the same element: same id, or equal when neither has one.
fn same_elem(a: &Value, b: &Value) -> bool {
    match (elem_id(a), elem_id(b)) {
        (Some(x), Some(y)) => x == y,
        (None, None) => value_eq(a, b),
        _ => false,
    }
}

/// Index pairs of elements kept between `old` and `new`, in increasing order.
fn matching(old: &[Value], new: &[Value]) -> Vec<(usize, usize)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| same_elem(a, b)).count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| same_elem(a, b))
        .count();
    let mut pairs: Vec<(usize, usize)> = (0..prefix).map(|i| (i, i)).collect();
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid = &new[prefix..new.len() - suffix];
    if !old_mid.is_empty() && !new_mid.is_empty() && old_mid.len() * new_mid.len() <= LCS_LIMIT {
        pairs.extend(
            lcs(old_mid, new_mid)
                .into_iter()
                .map(|(i, j)| (i + prefix, j + prefix)),
        );
    }
    let (ol, nl) = (old.len(), new.len());
    pairs.extend((0..suffix).rev().map(|k| (ol - 1 - k, nl - 1 - k)));
    pairs
}

/// Longest common subsequence of `a` and `b` under [`same_elem`], as index pairs.
fn lcs(a: &[Value], b: &[Value]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut t = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i * w + j] = if same_elem(&a[i], &b[j]) {
                t[(i + 1) * w + j + 1] + 1
            } else {
                t[(i + 1) * w + j].max(t[i * w + j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if same_elem(&a[i], &b[j]) {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[(i + 1) * w + j] >= t[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

fn sync_array(txn: &mut TransactionMut, arr: &ArrayRef, old: &[Value], new: &[Value]) {
    let pairs = matching(old, new);
    let mut pos: u32 = 0;
    let (mut oi, mut ni) = (0, 0);
    for &(mo, mn) in pairs.iter().chain(std::iter::once(&(old.len(), new.len()))) {
        let del = &old[oi..mo];
        let ins = &new[ni..mn];
        let common = del.len().min(ins.len());
        // Unmatched elements at the same place in both versions are edited in place.
        for (o, n) in del.iter().zip(ins) {
            sync_element(txn, arr, pos, o, n);
            pos += 1;
        }
        if del.len() > common {
            arr.remove_range(txn, pos, (del.len() - common) as u32);
        }
        for n in &ins[common..] {
            arr.insert(txn, pos, to_in(n));
            pos += 1;
        }
        if mo < old.len() {
            if !value_eq(&old[mo], &new[mn]) {
                sync_element(txn, arr, pos, &old[mo], &new[mn]);
            }
            pos += 1;
        }
        oi = mo + 1;
        ni = mn + 1;
    }
}

/// Turns the element at `pos` from `old` into `new`, editing it in place when both are maps of
/// the same id (or both without one) or both arrays, and replacing it otherwise.
fn sync_element(txn: &mut TransactionMut, arr: &ArrayRef, pos: u32, old: &Value, new: &Value) {
    match (old, new, arr.get(txn, pos)) {
        (Value::Object(o), Value::Object(n), Some(Out::YMap(m))) if elem_id(old) == elem_id(new) => {
            sync_map(txn, &m, o, n);
        }
        (Value::Array(o), Value::Array(n), Some(Out::YArray(a))) => sync_array(txn, &a, o, n),
        _ => {
            arr.remove(txn, pos);
            arr.insert(txn, pos, to_in(new));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Shared types → JSON.

fn out_to_json<T: ReadTxn>(txn: &T, out: Out) -> Value {
    match out {
        Out::Any(any) => any_to_json(&any),
        Out::YMap(map) => Value::Object(map_to_json(txn, &map)),
        Out::YArray(arr) => Value::Array(arr.iter(txn).map(|o| out_to_json(txn, o)).collect()),
        _ => Value::Null,
    }
}

fn map_to_json<T: ReadTxn>(txn: &T, map: &MapRef) -> Map<String, Value> {
    let mut entries: HashMap<String, Value> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (key, out) in map.iter(txn) {
        if key == ORDER_KEY {
            if let Out::YArray(list) = out {
                order = list
                    .iter(txn)
                    .filter_map(|o| match o {
                        Out::Any(Any::String(s)) => Some(s.to_string()),
                        _ => None,
                    })
                    .collect();
            }
        } else {
            entries.insert(key.to_owned(), out_to_json(txn, out));
        }
    }
    let mut result = Map::new();
    for key in order {
        if let Some(v) = entries.remove(&key) {
            result.insert(key, v);
        }
    }
    let mut rest: Vec<(String, Value)> = entries.into_iter().collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    result.extend(rest);
    result
}

fn any_to_json(any: &Any) -> Value {
    match any {
        Any::Null | Any::Undefined => Value::Null,
        Any::Bool(b) => Value::Bool(*b),
        Any::Number(f) => Number::from_f64(*f).map_or(Value::Null, Value::Number),
        Any::BigInt(i) => Value::from(*i),
        Any::String(s) => Value::String(s.to_string()),
        Any::Buffer(bytes) => Value::Array(bytes.iter().map(|&b| Value::from(b)).collect()),
        Any::Array(items) => Value::Array(items.iter().map(any_to_json).collect()),
        Any::Map(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), any_to_json(&map[k])))
                    .collect(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round_trip(v: &Value) -> Value {
        let mut doc = SyncDoc::with_client_id(1);
        let Value::Object(map) = v else { panic!("object") };
        {
            let mut txn = doc.doc.transact_mut();
            sync_map(&mut txn, &doc.root, &Map::new(), map);
        }
        doc.refresh();
        doc.to_value()
    }

    #[test]
    fn key_order_and_number_types_survive() {
        let v =
            json!({ "z": 1, "a": 1.0, "m": [ { "id": "p", "x": 2.5, "c": 1 }, "s", null, true ], "e": {} });
        let back = round_trip(&v);
        assert_eq!(back.to_string(), v.to_string());
    }

    #[test]
    fn diffs_keep_moved_points_as_edits() {
        let a = json!({ "pts": [ { "id": "a", "x": 0 }, { "id": "b", "x": 1 }, { "id": "c", "x": 2 } ] });
        let b = json!({ "pts": [ { "id": "a", "x": 0 }, { "id": "n", "x": 9 }, { "id": "b", "x": 5 } ] });
        let mut doc = SyncDoc::with_client_id(7);
        let Value::Object(am) = &a else { panic!() };
        let Value::Object(bm) = &b else { panic!() };
        {
            let mut txn = doc.doc.transact_mut();
            sync_map(&mut txn, &doc.root, &Map::new(), am);
        }
        doc.refresh();
        {
            let mut txn = doc.doc.transact_mut();
            sync_map(&mut txn, &doc.root, am, bm);
        }
        doc.refresh();
        assert_eq!(doc.to_value().to_string(), b.to_string());
    }
}
