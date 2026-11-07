//! Peers over localhost TCP and the shared update log.

mod common;

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use azam_doc::{Command, PieceTransform, Project, Template, apply};
use azam_geom::Vec2;
use azam_sync::{Collab, Config, JoinTarget, Presence, SyncDoc, problems};
use common::{Op, apply_op, first_diff, template};

fn config(name: &str) -> Config {
    Config {
        presence: Presence::named(name, name.len() as u64),
        listen: Some(SocketAddr::from(([127, 0, 0, 1], 0))),
        discovery: false,
        wake: None,
    }
}

/// A peer and the project its app shows.
struct Node {
    collab: Collab,
    project: Option<Project>,
}

impl Node {
    fn host(project: Project, name: &str) -> Self {
        let collab = Collab::host(&project, None, config(name)).expect("hosts");
        Self {
            collab,
            project: Some(project),
        }
    }

    fn join(addr: SocketAddr, name: &str) -> Self {
        let collab = Collab::join(JoinTarget::Addr(addr), config(name)).expect("joins");
        Self {
            collab,
            project: None,
        }
    }

    fn addr(&self) -> SocketAddr {
        self.collab.status().listen.expect("listens")
    }

    fn pump(&mut self) {
        if let Some(p) = self.collab.poll() {
            self.project = Some(p);
        }
    }

    fn edit(&mut self, op: Op) {
        let project = self.project.as_mut().expect("has the project");
        apply_op(project, op);
        self.collab.local_edit(project);
    }

    fn json(&self) -> String {
        self.collab.doc().to_value().to_string()
    }
}

fn pump_until(
    nodes: &mut [&mut Node],
    timeout: Duration,
    mut done: impl FnMut(&[&mut Node]) -> bool,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        for n in nodes.iter_mut() {
            n.pump();
        }
        if done(nodes) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn converged(nodes: &[&mut Node]) -> bool {
    nodes.iter().all(|n| n.project.is_some())
        && nodes.windows(2).all(|w| w[0].json() == w[1].json())
        && nodes
            .iter()
            .all(|n| n.project.as_ref().map(|p| p.to_value().to_string()) == Some(n.json()))
}

fn assert_converged(nodes: &mut [&mut Node], what: &str) {
    let ok = pump_until(nodes, Duration::from_secs(20), converged);
    if !ok {
        let a = nodes[0].collab.doc().to_value();
        for n in nodes.iter().skip(1) {
            eprintln!("diff: {:?}", first_diff(&a, &n.collab.doc().to_value()));
        }
    }
    assert!(ok, "{what}: peers did not converge");
    for n in nodes.iter() {
        let p = n.project.as_ref().expect("has the project");
        assert!(problems(p).is_empty(), "{what}: {:?}", problems(p));
    }
}

/// Deterministic pseudo-random ops.
fn ops(seed: u64, n: usize) -> Vec<Op> {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    (0..n)
        .map(|_| Op {
            kind: (next() % 256) as u8,
            a: next() as u32,
            b: next() as u32,
            x: (next() % 400) as f64 - 200.0 + 0.25,
            y: (next() % 400) as f64 - 200.0 + 0.5,
        })
        .collect()
}

#[test]
fn two_peers_converge_after_concurrent_edits() {
    let mut a = Node::host(template(Template::Tee), "ana");
    let mut b = Node::join(a.addr(), "bo");
    assert_converged(&mut [&mut a, &mut b], "initial sync");
    assert_eq!(
        b.collab.status().code,
        a.collab.status().code,
        "joiner learns the share code"
    );
    for (x, y) in ops(1, 15).into_iter().zip(ops(2, 15)) {
        a.edit(x);
        b.edit(y);
    }
    assert_converged(&mut [&mut a, &mut b], "concurrent edits");
}

#[test]
fn three_peers_converge_through_a_relay() {
    let mut a = Node::host(template(Template::Dress), "ana");
    let mut b = Node::join(a.addr(), "bo");
    assert_converged(&mut [&mut a, &mut b], "b joined");
    // c only knows b: a's and c's changes travel through b.
    let mut c = Node::join(b.addr(), "cy");
    assert_converged(&mut [&mut a, &mut b, &mut c], "c joined");
    for ((x, y), z) in ops(3, 12).into_iter().zip(ops(4, 12)).zip(ops(5, 12)) {
        a.edit(x);
        b.edit(y);
        c.edit(z);
    }
    assert_converged(&mut [&mut a, &mut b, &mut c], "concurrent edits");
}

#[test]
fn edits_made_during_a_partition_merge_on_reconnect() {
    let mut a = Node::host(template(Template::Skirt), "ana");
    let mut b = Node::join(a.addr(), "bo");
    let mut c = Node::join(a.addr(), "cy");
    assert_converged(&mut [&mut a, &mut b, &mut c], "joined");
    a.collab.set_online(false);
    assert!(
        pump_until(&mut [&mut a, &mut b, &mut c], Duration::from_secs(5), |n| n[0]
            .collab
            .status()
            .connections
            == 0),
        "a goes offline"
    );
    for op in ops(6, 10) {
        a.edit(op);
    }
    for (y, z) in ops(7, 10).into_iter().zip(ops(8, 10)) {
        b.edit(y);
        c.edit(z);
    }
    // b and c still sync with each other through... nobody: they only knew a. Give them a moment.
    pump_until(&mut [&mut a, &mut b, &mut c], Duration::from_millis(300), |_| {
        false
    });
    assert_ne!(a.json(), b.json(), "a is cut off");
    a.collab.set_online(true);
    assert_converged(&mut [&mut a, &mut b, &mut c], "after the partition");
}

#[test]
fn awareness_propagates() {
    let mut a = Node::host(template(Template::Tee), "ana");
    let mut b = Node::join(a.addr(), "bo");
    assert_converged(&mut [&mut a, &mut b], "joined");
    let piece = a
        .project
        .as_ref()
        .and_then(|p| p.pieces.keys().next().cloned())
        .expect("has pieces");
    let mut presence = a.collab.presence().clone();
    presence.piece = Some(piece.0.clone());
    presence.cursor = Some([10.0, 1200.0, 30.0]);
    a.collab.set_presence(presence.clone());
    assert!(
        pump_until(&mut [&mut a, &mut b], Duration::from_secs(5), |n| {
            n[1].collab.peers().iter().any(|p| p.presence == presence)
                && n[0].collab.peers().iter().any(|p| p.presence.name == "bo")
        }),
        "presence reaches the other peer"
    );
    assert_eq!(b.collab.peers().len(), 1);
    drop(a);
    assert!(
        pump_until(&mut [&mut b], Duration::from_secs(5), |n| n[0]
            .collab
            .peers()
            .is_empty()),
        "a leaving clears its presence"
    );
}

#[test]
fn file_log_merges_two_writers() {
    let dir = std::env::temp_dir().join(format!("azam-sync-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("tee.azam.json");
    let _ = std::fs::remove_file(azam_sync::UpdateLog::path_for(&path));
    let project = template(Template::Tee);
    let cfg = || Config {
        listen: None,
        discovery: false,
        ..config("w")
    };
    let (c1, adopted1) = Collab::open_log(&path, &project, cfg()).expect("opens");
    assert!(adopted1.is_none(), "a new log starts from the project");
    // The second instance opened a stale copy of the file: the log wins.
    let stale = template(Template::Skirt);
    let (c2, adopted2) = Collab::open_log(&path, &stale, cfg()).expect("opens");
    let adopted2 = adopted2.expect("the log holds a document");
    assert_eq!(adopted2.to_value(), project.to_value());
    let mut n1 = Node {
        collab: c1,
        project: Some(project),
    };
    let mut n2 = Node {
        collab: c2,
        project: Some(adopted2),
    };
    for (x, y) in ops(9, 12).into_iter().zip(ops(10, 12)) {
        n1.edit(x);
        n2.edit(y);
    }
    assert_converged(&mut [&mut n1, &mut n2], "two writers");
    // A third instance opening later sees everything.
    // Compact in the middle of more concurrent edits: nothing is lost.
    let before = std::fs::metadata(azam_sync::UpdateLog::path_for(&path))
        .expect("log")
        .len();
    n1.collab.compact_log();
    for (x, y) in ops(12, 8).into_iter().zip(ops(13, 8)) {
        n1.edit(x);
        n2.edit(y);
    }
    assert_converged(&mut [&mut n1, &mut n2], "two writers across a compaction");
    let log = std::fs::read(azam_sync::UpdateLog::path_for(&path)).expect("log");
    assert_eq!(&log[..4], b"AZYH");
    assert!(
        u64::from_le_bytes(log[8..16].try_into().expect("8 bytes")) >= 2,
        "the log was compacted"
    );
    eprintln!(
        "log: {before} bytes before compaction, {} after the later edits",
        log.len()
    );
    let (c3, adopted3) = Collab::open_log(&path, &stale, cfg()).expect("opens");
    assert_eq!(adopted3.map(|p| p.to_value().to_string()), Some(n1.json()));
    drop(c3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn large_projects_sync_quickly_with_small_edits() {
    let project = template(Template::Labadeh);
    let full = SyncDoc::from_project(&project).encode_state();
    let start = Instant::now();
    let mut a = Node::host(project, "ana");
    let mut b = Node::join(a.addr(), "bo");
    assert_converged(&mut [&mut a, &mut b], "initial sync");
    let initial = start.elapsed();

    let p = a.project.as_ref().expect("project").clone();
    let piece = p.pieces.keys().next().cloned().expect("pieces");
    let point = p.pieces[&piece].pts[1].id.clone();
    let seg = p.pieces[&piece].pts[0].id.clone();
    let edits: Vec<(&str, Command)> = vec![
        (
            "move one point",
            Command::MovePoints {
                piece: piece.clone(),
                points: vec![point],
                delta: Vec2::new(3.0, -2.0),
            },
        ),
        (
            "move a piece",
            Command::TransformPieces {
                pieces: vec![piece.clone()],
                transform: PieceTransform::Translate(Vec2::new(25.0, 0.0)),
            },
        ),
        (
            "split a segment",
            Command::SplitSegment {
                piece: piece.clone(),
                segment: seg.clone(),
                t: 0.5,
            },
        ),
        (
            "segment allowance",
            Command::SetSegmentAllowance {
                piece: piece.clone(),
                segment: seg,
                mm: 15.0,
            },
        ),
        ("rename", Command::Rename { name: "Robe".into() }),
        ("duplicate a piece", Command::DuplicatePiece { piece }),
    ];
    let mut report = format!(
        "labadeh: {} pieces, full state {} bytes, joined and synced in {:?}\n",
        p.pieces.len(),
        full.len(),
        initial
    );
    for (label, command) in edits {
        let project = a.project.as_mut().expect("project");
        apply(project, command).expect("edit applies");
        let t = Instant::now();
        let bytes = a.collab.local_edit(project).expect("edit changes the document");
        let target = a.json();
        assert!(
            pump_until(&mut [&mut a, &mut b], Duration::from_secs(5), |n| n[1].json()
                == target),
            "{label} reaches b"
        );
        let _ = writeln!(report, "  {label}: {bytes} bytes, round trip {:?}", t.elapsed());
        if label == "move one point" || label == "rename" {
            assert!(bytes < 200, "{label} takes {bytes} bytes");
        }
    }
    eprintln!("{report}");
    assert!(initial < Duration::from_secs(5), "initial sync took {initial:?}");
}

#[test]
fn the_app_is_woken_only_when_there_is_news() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let project = template(Template::Tee);
    let host = Collab::host(
        &project,
        None,
        Config {
            wake: Some(azam_sync::Wake::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })),
            ..config("ana")
        },
    )
    .expect("hosts");
    let mut a = Node {
        collab: host,
        project: Some(project),
    };
    let mut b = Node::join(a.addr(), "bo");
    assert_converged(&mut [&mut a, &mut b], "joined");
    assert!(wakes.load(Ordering::SeqCst) > 0, "the connection woke the app");
    // Idle: nothing happens, nobody polls, no wakes.
    pump_until(&mut [&mut a, &mut b], Duration::from_millis(300), |_| false);
    wakes.store(0, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        0,
        "idle sessions do not wake the app"
    );
    // A remote edit wakes it once, however many arrive before it polls.
    for op in ops(11, 5) {
        b.edit(op);
    }
    let start = Instant::now();
    while wakes.load(Ordering::SeqCst) == 0 && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(wakes.load(Ordering::SeqCst), 1, "one wake until the app polls");
    a.project = a.collab.poll();
    assert!(a.project.is_some(), "the wake had changes behind it");
    assert_converged(&mut [&mut a, &mut b], "after the wake");
}
