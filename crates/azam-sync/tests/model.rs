//! The CRDT model: exact round trips, convergence of concurrent edits, valid merged projects.

mod common;

use azam_doc::{Project, Template};
use azam_sync::{SyncDoc, problems};
use common::{Op, apply_op, template};
use proptest::prelude::*;

fn json(p: &Project) -> String {
    p.to_value().to_string()
}

#[test]
fn every_template_round_trips_exactly() {
    for t in Template::ALL {
        let project = template(t);
        let a = SyncDoc::from_project(&project);
        assert_eq!(
            json(&a.to_project().expect("reads back")),
            json(&project),
            "{t:?} locally"
        );
        // Through the update encoding, on another replica.
        let mut b = SyncDoc::new();
        b.apply_update(&a.encode_state()).expect("applies");
        assert_eq!(
            json(&b.to_project().expect("reads back")),
            json(&project),
            "{t:?} remotely"
        );
        assert!(
            b.to_project().expect("reads").eq(&project)
                || json(&b.to_project().expect("reads")) == json(&project)
        );
    }
}

#[test]
fn unchanged_projects_record_nothing() {
    let project = template(Template::Tee);
    let mut doc = SyncDoc::from_project(&project);
    assert!(doc.apply_project(&project).is_none());
}

/// Two replicas of `t` edit concurrently, then exchange their updates.
fn concurrent(t: Template, ops_a: &[Op], ops_b: &[Op]) -> Result<(), TestCaseError> {
    let start = template(t);
    let mut a = SyncDoc::from_project(&start);
    let mut b = SyncDoc::new();
    b.apply_update(&a.encode_state())
        .map_err(|e| TestCaseError::fail(e.to_string()))?;
    let mut pa = a.to_project().map_err(|e| TestCaseError::fail(e.to_string()))?;
    let mut pb = b.to_project().map_err(|e| TestCaseError::fail(e.to_string()))?;
    let mut ua = Vec::new();
    let mut ub = Vec::new();
    for op in ops_a {
        apply_op(&mut pa, *op);
        ua.extend(a.apply_project(&pa));
        // Every local state reads back exactly.
        let back = a.to_project().map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert!(
            json(&back) == json(&pa),
            "local read-back differs at {:?} after {:?}",
            common::first_diff(&back.to_value(), &pa.to_value()),
            op
        );
    }
    for op in ops_b {
        apply_op(&mut pb, *op);
        ub.extend(b.apply_project(&pb));
    }
    a.apply_updates(ub.iter().map(Vec::as_slice))
        .map_err(|e| TestCaseError::fail(e.to_string()))?;
    b.apply_updates(ua.iter().map(Vec::as_slice))
        .map_err(|e| TestCaseError::fail(e.to_string()))?;
    let diff = common::first_diff(&a.to_value(), &b.to_value());
    prop_assert!(diff.is_none(), "replicas differ at {:?}", diff);
    // Both read back as the same valid project; pushing the repairs keeps them converged.
    let ma = a.to_project().map_err(|e| TestCaseError::fail(e.to_string()))?;
    let mb = b.to_project().map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert_eq!(json(&ma), json(&mb));
    prop_assert!(
        problems(&ma).is_empty(),
        "merged project has problems: {:?}",
        problems(&ma)
    );
    let ra = a.apply_project(&ma);
    let rb = b.apply_project(&mb);
    if let Some(u) = ra {
        b.apply_update(&u)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
    }
    if let Some(u) = rb {
        a.apply_update(&u)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
    }
    prop_assert_eq!(
        a.to_value().to_string(),
        b.to_value().to_string(),
        "converged after repairs"
    );
    prop_assert_eq!(
        json(&a.to_project().map_err(|e| TestCaseError::fail(e.to_string()))?),
        json(&ma)
    );
    Ok(())
}

fn op() -> impl Strategy<Value = Op> {
    (
        any::<u8>(),
        any::<u32>(),
        any::<u32>(),
        -200.0..200.0f64,
        -200.0..200.0f64,
    )
        .prop_map(|(kind, a, b, x, y)| Op { kind, a, b, x, y })
}

fn any_template() -> impl Strategy<Value = Template> {
    prop::sample::select(Template::ALL.to_vec())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn concurrent_edits_converge(t in any_template(), ops_a in prop::collection::vec(op(), 0..12), ops_b in prop::collection::vec(op(), 0..12)) {
        concurrent(t, &ops_a, &ops_b)?;
    }

    #[test]
    fn edit_sequences_round_trip(t in any_template(), ops in prop::collection::vec(op(), 0..20)) {
        let mut project = template(t);
        let mut doc = SyncDoc::from_project(&project);
        let mut remote = SyncDoc::new();
        remote.apply_update(&doc.encode_state()).map_err(|e| TestCaseError::fail(e.to_string()))?;
        for op in ops {
            apply_op(&mut project, op);
            if let Some(u) = doc.apply_project(&project) {
                remote.apply_update(&u).map_err(|e| TestCaseError::fail(e.to_string()))?;
            }
            prop_assert_eq!(doc.to_value().to_string(), project.to_value().to_string());
            prop_assert!(doc.to_value() == remote.to_value(), "remote replica follows");
        }
    }
}
