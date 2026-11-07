//! Adding fabrics and colorways: each starts as a copy of an existing one.

use crate::ids::{ColorwayId, FabricId, prefix};
use crate::project::{Colorway, Project};

/// Prefix of generated colorway ids.
const COLORWAY_PREFIX: &str = "cw";

/// `name` with Blender's numeric suffix (`Name.001`, `Name.002`…) that no name in `taken` has.
fn next_name(name: &str, taken: &[&str]) -> String {
    let base = match name.rsplit_once('.') {
        Some((stem, digits)) if digits.len() == 3 && digits.bytes().all(|b| b.is_ascii_digit()) => stem,
        _ => name,
    };
    // one of the first `taken.len() + 1` suffixes is always free
    (1..=taken.len() + 1)
        .map(|n| format!("{base}.{n:03}"))
        .find(|candidate| !taken.contains(&candidate.as_str()))
        .unwrap_or_else(|| base.to_owned())
}

/// Adds a copy of fabric `source` (physics, colour, weave…) under a new id and a `.001`-style
/// name, and returns the new id; `None` if there is no such fabric. Colorway overrides of the
/// source carry over to the copy.
pub fn duplicate_fabric(project: &mut Project, source: &FabricId) -> Option<FabricId> {
    let mut fabric = project.fabrics.get(source)?.clone();
    let id = FabricId::new(project.fresh_id(prefix::FABRIC));
    let names: Vec<&str> = project.fabrics.values().map(|f| f.name.as_str()).collect();
    fabric.name = next_name(&fabric.name, &names);
    fabric.id = id.clone();
    for cw in project.colorways.values_mut() {
        if let Some(over) = cw.over.get(source).cloned() {
            cw.over.insert(id.clone(), over);
        }
    }
    project.fabrics.insert(id.clone(), fabric);
    Some(id)
}

/// Adds a copy of colorway `source` with its fabric overrides, last in the list, and returns its
/// id; `None` if there is no such colorway.
pub fn duplicate_colorway(project: &mut Project, source: &ColorwayId) -> Option<ColorwayId> {
    let src = project.colorways.get(source)?;
    let names: Vec<&str> = project.colorways.values().map(|c| c.name.as_str()).collect();
    let name = next_name(&src.name, &names);
    let order = project.colorways.values().map(|c| c.order + 1).max().unwrap_or(0);
    let over = src.over.clone();
    let extra = src.extra.clone();
    let id = project.fresh_id(COLORWAY_PREFIX);
    let mut cw = Colorway::new(&id, &name, order);
    cw.over = over;
    cw.extra = extra;
    let id = cw.id.clone();
    project.colorways.insert(id.clone(), cw);
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;

    fn project() -> Project {
        Project::new(None, Sex::Female, 0.0)
    }

    #[test]
    fn names_take_the_next_free_suffix() {
        let taken = ["Jersey", "Jersey.001"];
        assert_eq!(next_name("Jersey", &taken), "Jersey.002");
        assert_eq!(next_name("Jersey.001", &taken), "Jersey.002");
        assert_eq!(next_name("Denim", &taken), "Denim.001");
    }

    #[test]
    fn a_duplicated_fabric_is_a_separate_copy() {
        let mut p = project();
        let src = p.fabrics.keys().next().cloned().expect("a fabric");
        let copy = duplicate_fabric(&mut p, &src).expect("copied");
        assert_ne!(copy, src);
        assert_eq!(p.fabrics.len(), 2);
        assert_eq!(p.fabrics[&copy].phys, p.fabrics[&src].phys);
        assert_eq!(p.fabrics[&copy].name, format!("{}.001", p.fabrics[&src].name));
        p.fabrics.get_mut(&copy).expect("copy").color = "#ff0000".into();
        assert_ne!(
            p.fabrics[&src].color, "#ff0000",
            "editing the copy leaves the source alone"
        );
    }

    #[test]
    fn a_duplicated_colorway_keeps_its_overrides() {
        let mut p = project();
        let src = p.active_colorway.clone();
        let fabric = p.fabrics.keys().next().cloned().expect("a fabric");
        let mut over = serde_json::Map::new();
        over.insert("color".into(), "#123456".into());
        p.colorways
            .get_mut(&src)
            .expect("colorway")
            .over
            .insert(fabric.clone(), over);
        let copy = duplicate_colorway(&mut p, &src).expect("copied");
        assert_eq!(p.colorways.len(), 2);
        assert_eq!(p.colorways[&copy].over[&fabric]["color"], "#123456");
        assert!(p.colorways[&copy].order > p.colorways[&src].order);
        assert!(duplicate_colorway(&mut p, &ColorwayId::new("missing")).is_none());
    }
}
