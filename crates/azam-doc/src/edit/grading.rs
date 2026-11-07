//! Authoring grade rules: the size run, and linear grading of points.

use azam_geom::{GradeDelta, PointId, SizeName, Vec2};

use crate::ids::PieceId;
use crate::project::Project;

/// Sets the project's size run to `names` (smallest first; blanks and repeats are dropped).
///
/// A run as long as the old one renames sizes in place, keeping their grade rules. Otherwise the
/// base size stays if it is still in the run, else the pattern is rebased onto a size the runs
/// share ([`set_base_size`]); grade rules for sizes no longer in the run are removed, and the
/// active size falls back to the base. Returns `false` (and changes nothing) if no size is left.
pub fn set_sizes(project: &mut Project, names: &[SizeName]) -> bool {
    let mut run: Vec<SizeName> = Vec::new();
    for name in names {
        let trimmed = name.0.trim();
        if !trimmed.is_empty() && !run.iter().any(|s| s.0 == trimmed) {
            run.push(SizeName(trimmed.to_owned()));
        }
    }
    if run.is_empty() {
        return false;
    }
    if run.len() == project.sizes.len() {
        rename_sizes(project, &run);
        return true;
    }
    if !run.contains(&project.base_size) {
        let shared: Vec<SizeName> = run
            .iter()
            .filter(|s| project.sizes.contains(s))
            .cloned()
            .collect();
        match shared.get(shared.len() / 2) {
            Some(base) => {
                set_base_size(project, base);
            }
            None => project.base_size = run[run.len() / 2].clone(),
        }
    }
    if !run.contains(&project.active_size) {
        project.active_size = project.base_size.clone();
    }
    for piece in project.pieces.values_mut() {
        if let Some(rules) = piece.grade.as_mut() {
            for by_size in rules.values_mut() {
                by_size.retain(|size, _| run.contains(size));
            }
            rules.retain(|_, by_size| !by_size.is_empty());
        }
    }
    project.sizes = run;
    true
}

/// Renames the sizes of the run, position by position, to `run` (as long as the old run), in the
/// base and active size and every grade rule.
fn rename_sizes(project: &mut Project, run: &[SizeName]) {
    let renamed = |s: &SizeName, old: &[SizeName]| old.iter().position(|o| o == s).map(|i| run[i].clone());
    let old = std::mem::replace(&mut project.sizes, run.to_vec());
    if let Some(b) = renamed(&project.base_size, &old) {
        project.base_size = b;
    }
    if let Some(a) = renamed(&project.active_size, &old) {
        project.active_size = a;
    }
    for piece in project.pieces.values_mut() {
        if let Some(rules) = piece.grade.as_mut() {
            for by_size in rules.values_mut() {
                *by_size = std::mem::take(by_size)
                    .into_iter()
                    .filter_map(|(s, d)| renamed(&s, &old).map(|n| (n, d)))
                    .collect();
            }
        }
    }
}

/// Makes `size` the base size: every graded point moves to where `size` puts it, and the grade
/// rules are measured from there (the old base size gets the opposite offset). The pattern looks
/// the same in every size. Returns `false` if `size` is not in the run.
pub fn set_base_size(project: &mut Project, size: &SizeName) -> bool {
    if !project.sizes.contains(size) {
        return false;
    }
    if *size == project.base_size {
        return true;
    }
    let old = std::mem::replace(&mut project.base_size, size.clone());
    let keep_old = project.sizes.contains(&old);
    for piece in project.pieces.values_mut() {
        let Some(rules) = piece.grade.as_mut() else {
            continue;
        };
        for (point, by_size) in rules.iter_mut() {
            let d = by_size.remove(size).unwrap_or_default();
            if let Some(p) = piece.pts.iter_mut().find(|q| q.id == *point) {
                p.pos += Vec2::new(d.dx, d.dy);
            }
            for delta in by_size.values_mut() {
                delta.dx -= d.dx;
                delta.dy -= d.dy;
            }
            if keep_old && (d.dx != 0.0 || d.dy != 0.0) {
                by_size.insert(old.clone(), GradeDelta { dx: -d.dx, dy: -d.dy });
            }
        }
        rules.retain(|_, by_size| by_size.values().any(|d| d.dx != 0.0 || d.dy != 0.0));
    }
    true
}

/// Sets one axis of the per-size step of each of `points` (x when `along_x`, else y), keeping
/// each point's own step on the other axis.
pub fn set_grade_step_axis(
    project: &mut Project,
    piece: &PieceId,
    points: &[PointId],
    along_x: bool,
    value: f64,
) {
    for point in points {
        let current = grade_step(project, piece, point);
        let step = if along_x {
            Vec2::new(value, current.y)
        } else {
            Vec2::new(current.x, value)
        };
        grade_points_linear(project, piece, std::slice::from_ref(point), step);
    }
}

/// Grades `points` of `piece` linearly: each size moves them by `step` mm per size away from the
/// base (one size up = `+step`, one down = `-step`). A zero step removes their rules.
pub fn grade_points_linear(project: &mut Project, piece: &PieceId, points: &[PointId], step: Vec2) {
    let Some(base) = project.sizes.iter().position(|s| *s == project.base_size) else {
        return;
    };
    let sizes = project.sizes.clone();
    let Some(pc) = project.pieces.get_mut(piece) else {
        return;
    };
    let rules = pc.grade.get_or_insert_with(Default::default);
    for point in points {
        if step == Vec2::ZERO {
            rules.remove(point);
            continue;
        }
        let by_size = rules.entry(point.clone()).or_default();
        by_size.clear();
        for (i, size) in sizes.iter().enumerate() {
            if i != base {
                let k = i as f64 - base as f64;
                by_size.insert(
                    size.clone(),
                    GradeDelta {
                        dx: step.x * k,
                        dy: step.y * k,
                    },
                );
            }
        }
    }
    if rules.is_empty() {
        pc.grade = None;
    }
}

/// The per-size step of `point` of `piece`, read from the size just above the base (or just below
/// it, negated); zero when the point has no rule.
#[must_use]
pub fn grade_step(project: &Project, piece: &PieceId, point: &PointId) -> Vec2 {
    let Some(base) = project.sizes.iter().position(|s| *s == project.base_size) else {
        return Vec2::ZERO;
    };
    let Some(by_size) = project
        .pieces
        .get(piece)
        .and_then(|pc| pc.grade.as_ref())
        .and_then(|rules| rules.get(point))
    else {
        return Vec2::ZERO;
    };
    let delta = |i: usize| project.sizes.get(i).and_then(|s| by_size.get(s));
    if let Some(d) = delta(base + 1) {
        Vec2::new(d.dx, d.dy)
    } else if let Some(d) = base.checked_sub(1).and_then(delta) {
        Vec2::new(-d.dx, -d.dy)
    } else {
        Vec2::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Sex;

    fn project_with_square() -> (Project, PieceId, Vec<PointId>) {
        let mut p = Project::new(None, Sex::Female, 0.0);
        let pts =
            [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].map(|(x, y)| (Vec2::new(x, y), false));
        let id = crate::create_piece(&mut p, &pts).expect("piece");
        let ids = p.pieces[&id].pts.iter().map(|q| q.id.clone()).collect();
        (p, id, ids)
    }

    fn names(list: &[&str]) -> Vec<SizeName> {
        list.iter().map(|&s| SizeName::from(s)).collect()
    }

    #[test]
    fn linear_grading_moves_points_per_size() {
        let (mut p, piece, pts) = project_with_square();
        assert!(set_sizes(&mut p, &names(&["S", "M", "L", "XL"])));
        p.base_size = SizeName::from("M");
        grade_points_linear(&mut p, &piece, &pts[1..3], Vec2::new(5.0, 0.0));
        let outline = |size: &str| p.pieces[&piece].graded_outline(&SizeName::from(size), &p.base_size);
        assert!(
            (outline("XL").poly[1].x - 110.0).abs() < 1e-9,
            "two sizes up: +10 mm"
        );
        assert!(
            (outline("S").poly[1].x - 95.0).abs() < 1e-9,
            "one size down: -5 mm"
        );
        assert!((outline("L").poly[0].x).abs() < 1e-9, "ungraded points stay put");
        assert_eq!(grade_step(&p, &piece, &pts[1]), Vec2::new(5.0, 0.0));
        grade_points_linear(&mut p, &piece, &pts[1..3], Vec2::ZERO);
        assert!(p.pieces[&piece].grade.is_none(), "a zero step clears the rules");
    }

    #[test]
    fn a_new_base_size_keeps_every_size_in_place() {
        let (mut p, piece, pts) = project_with_square();
        set_sizes(&mut p, &names(&["S", "M", "L"]));
        p.base_size = SizeName::from("M");
        grade_points_linear(&mut p, &piece, &pts[1..2], Vec2::new(5.0, 2.0));
        let shape = |p: &Project, size: &str| {
            p.pieces[&piece]
                .graded_outline(&SizeName::from(size), &p.base_size)
                .poly
        };
        let before: Vec<_> = ["S", "M", "L"].iter().map(|s| shape(&p, s)).collect();
        assert!(set_base_size(&mut p, &SizeName::from("L")));
        let after: Vec<_> = ["S", "M", "L"].iter().map(|s| shape(&p, s)).collect();
        for (b, a) in before.iter().zip(&after) {
            for (q, r) in b.iter().zip(a) {
                assert!(q.distance(*r) < 1e-9, "every size keeps its shape");
            }
        }
        assert!(
            (p.pieces[&piece].pts[1].pos.x - 105.0).abs() < 1e-9,
            "the drafted pattern is now L"
        );
    }

    #[test]
    fn renaming_sizes_keeps_their_rules() {
        let (mut p, piece, pts) = project_with_square();
        set_sizes(&mut p, &names(&["S", "M", "L"]));
        p.base_size = SizeName::from("M");
        grade_points_linear(&mut p, &piece, &pts[1..2], Vec2::new(5.0, 0.0));
        assert!(set_sizes(&mut p, &names(&["Small", "Medium", "Large"])));
        assert_eq!(p.base_size, SizeName::from("Medium"));
        assert_eq!(grade_step(&p, &piece, &pts[1]), Vec2::new(5.0, 0.0));
    }

    #[test]
    fn one_axis_changes_per_point() {
        let (mut p, piece, pts) = project_with_square();
        set_sizes(&mut p, &names(&["S", "M", "L"]));
        p.base_size = SizeName::from("M");
        grade_points_linear(&mut p, &piece, &pts[1..2], Vec2::new(1.0, 7.0));
        grade_points_linear(&mut p, &piece, &pts[2..3], Vec2::new(2.0, -3.0));
        set_grade_step_axis(&mut p, &piece, &pts[1..3], true, 4.0);
        assert_eq!(grade_step(&p, &piece, &pts[1]), Vec2::new(4.0, 7.0));
        assert_eq!(
            grade_step(&p, &piece, &pts[2]),
            Vec2::new(4.0, -3.0),
            "each keeps its own y"
        );
    }

    #[test]
    fn a_new_size_run_keeps_base_and_drops_old_rules() {
        let (mut p, piece, pts) = project_with_square();
        set_sizes(&mut p, &names(&["S", "M", "L"]));
        p.base_size = SizeName::from("M");
        p.active_size = SizeName::from("L");
        grade_points_linear(&mut p, &piece, &pts[..1], Vec2::new(0.0, 4.0));
        assert!(set_sizes(&mut p, &names(&[" M ", "M", ""])));
        assert_eq!(p.sizes, names(&["M"]));
        assert_eq!(p.base_size, SizeName::from("M"));
        assert_eq!(
            p.active_size,
            SizeName::from("M"),
            "L is gone, so the base is shown"
        );
        assert!(
            p.pieces[&piece]
                .grade
                .as_ref()
                .is_none_or(std::collections::BTreeMap::is_empty),
            "rules for S and L are dropped"
        );
        assert!(!set_sizes(&mut p, &names(&["", " "])), "an empty run is refused");
        assert_eq!(p.sizes, names(&["M"]));
    }
}
