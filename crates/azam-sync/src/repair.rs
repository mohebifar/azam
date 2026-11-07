//! Repairs after merging: concurrent edits can each be valid and still combine into dangling
//! references (one collaborator sews a piece another just deleted).

use std::collections::{HashMap, HashSet};

use azam_doc::{PieceId, PointId, Project, delete_pieces};
use serde_json::Value;

/// Removes references to pieces that no longer exist, the same way deleting those pieces would
/// have (seams, topstitches, seams left without sides, buttons, holes, comments, measurements and mirror links), drops
/// repeated sizes, and resets an active colorway or size that is gone. Returns whether anything
/// changed.
///
/// References to single segments are left alone: the document model itself tolerates them (a
/// linked mirror re-synced after its source lost a point keeps its seams).
///
/// The result depends only on the project, so every replica repairing the same merged document
/// makes the same changes.
pub fn repair(project: &mut Project) -> bool {
    let mut changed = false;
    let exists = |id: &PieceId| project.pieces.contains_key(id);
    let mut missing: HashSet<PieceId> = HashSet::new();
    let mut note = |id: &PieceId| {
        if !exists(id) {
            missing.insert(id.clone());
        }
    };
    for seam in project.sewing.values() {
        for (_, side) in seam.sides() {
            note(&side.piece);
        }
    }
    for item in project
        .buttons
        .values()
        .chain(project.holes.values())
        .chain(project.comments.values())
    {
        if let Some(p) = &item.p {
            note(p);
        }
    }
    for st in project.stitches.values() {
        if let Some(p) = &st.p {
            note(p);
        }
    }
    for pom in project.poms.values() {
        for r in &pom.refs {
            note(&r.p);
        }
    }
    for pc in project.pieces.values() {
        if let Some(m) = &pc.mirror {
            note(&m.src);
        }
    }
    if !missing.is_empty() {
        let ids: Vec<PieceId> = missing.into_iter().collect();
        delete_pieces(project, &ids);
        changed = true;
    }

    // Two collaborators each deleting the piece on one side of a seam leave it with no sides.
    let seams_before = project.sewing.len();
    project
        .sewing
        .retain(|_, seam| !seam.a.is_empty() && !seam.b.is_empty());
    changed |= project.sewing.len() != seams_before;

    if !project.colorways.contains_key(&project.active_colorway)
        && let Some(first) = project.colorways.keys().next()
    {
        project.active_colorway = first.clone();
        changed = true;
    }
    let mut seen = HashSet::new();
    let sizes_before = project.sizes.len();
    project.sizes.retain(|s| seen.insert(s.clone()));
    changed |= project.sizes.len() != sizes_before;
    if !project.sizes.is_empty() {
        if !project.sizes.contains(&project.base_size) {
            project.base_size = project.sizes[project.sizes.len() / 2].clone();
            changed = true;
        }
        if !project.sizes.contains(&project.active_size) {
            project.active_size = project.base_size.clone();
            changed = true;
        }
    }
    changed
}

/// Problems that make a project unusable: references to missing pieces, duplicate point ids,
/// seams without a side and an active colorway or size that does not exist.
/// Empty when the project is valid.
#[must_use]
pub fn problems(project: &Project) -> Vec<String> {
    let mut out = Vec::new();
    let mut points: HashMap<&PieceId, HashSet<&PointId>> = HashMap::new();
    for (id, pc) in &project.pieces {
        if &pc.id != id {
            out.push(format!("piece {id} is stored under another id ({})", pc.id));
        }
        let set: HashSet<&PointId> = pc.pts.iter().map(|p| &p.id).collect();
        if set.len() != pc.pts.len() {
            out.push(format!("piece {id} has duplicate point ids"));
        }
        if let Some(m) = &pc.mirror
            && !project.pieces.contains_key(&m.src)
        {
            out.push(format!("piece {id} mirrors missing piece {}", m.src));
        }
        points.insert(id, set);
    }
    for seam in project.sewing.values() {
        if seam.a.is_empty() || seam.b.is_empty() {
            out.push(format!("seam {} has an empty side", seam.id));
        }
        for (_, side) in seam.sides() {
            if !points.contains_key(&side.piece) {
                out.push(format!("seam {} sews missing piece {}", seam.id, side.piece));
            }
        }
    }
    for st in project.stitches.values() {
        if let Some(p) = &st.p
            && !points.contains_key(p)
        {
            out.push(format!("topstitch {} is on missing piece {p}", st.id));
        }
    }
    for item in project
        .buttons
        .values()
        .chain(project.holes.values())
        .chain(project.comments.values())
    {
        if let Some(p) = &item.p
            && !project.pieces.contains_key(p)
        {
            out.push(format!("item {} is on missing piece {p}", item.id));
        }
    }
    for pom in project.poms.values() {
        for r in &pom.refs {
            if !project.pieces.contains_key(&r.p) {
                out.push(format!("measurement {} uses missing piece {}", pom.id, r.p));
            }
        }
    }
    if !project.colorways.contains_key(&project.active_colorway) && !project.colorways.is_empty() {
        out.push("the active colorway does not exist".to_owned());
    }
    if !project.sizes.is_empty() && !project.sizes.contains(&project.active_size) {
        out.push("the active size is not in the size group".to_owned());
    }
    out
}

/// Drops repeated elements from arrays of objects with ids, keeping each id's first occurrence.
/// Returns whether anything was dropped.
///
/// A list edit that moves elements (flipping a piece reverses its outline) is a delete and an
/// insert; two collaborators moving the same elements concurrently both insert them, and this
/// removes the second copy.
pub(crate) fn dedupe_ids(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => map.values_mut().fold(false, |acc, v| dedupe_ids(v) | acc),
        Value::Array(items) => {
            let mut changed = items.iter_mut().fold(false, |acc, v| dedupe_ids(v) | acc);
            let ids: Option<Vec<String>> = items
                .iter()
                .map(|v| v.as_object()?.get("id")?.as_str().map(str::to_owned))
                .collect();
            if let Some(ids) = ids {
                let mut seen = HashSet::new();
                let keep: Vec<bool> = ids.into_iter().map(|id| seen.insert(id)).collect();
                if keep.contains(&false) {
                    let mut it = keep.into_iter();
                    items.retain(|_| it.next().unwrap_or(true));
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}
