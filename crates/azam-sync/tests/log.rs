//! Compaction of the shared update log.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use azam_doc::{Command, Template, apply};
use azam_geom::Vec2;
use azam_sync::{SyncDoc, UpdateLog};
use common::template;
use yrs::updates::decoder::Decode;
use yrs::{Array, Doc, Transact, Update};

fn temp_log(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("azam-sync-compact-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join("garment.azam.json.sync")
}

/// A replica that reads everything in the log.
fn replay(path: &std::path::Path) -> SyncDoc {
    let mut log = UpdateLog::open(path).expect("opens");
    let mut doc = SyncDoc::new();
    let records = log.read_new().expect("reads");
    doc.apply_updates(records.iter().map(Vec::as_slice))
        .expect("applies");
    doc
}

#[test]
fn compaction_shrinks_the_log_and_keeps_the_document() {
    let path = temp_log("shrink");
    let mut project = template(Template::Tee);
    let mut doc = SyncDoc::from_project(&project);
    let mut log = UpdateLog::open(&path).expect("opens");
    let initial = doc.encode_state().len();
    log.append(&doc.encode_state()).expect("appends");
    let piece = project.pieces.keys().next().cloned().expect("pieces");
    let point = project.pieces[&piece].pts[0].id.clone();
    for i in 0..1500 {
        let command = Command::MovePoints {
            piece: piece.clone(),
            points: vec![point.clone()],
            delta: Vec2::new(0.5, if i % 2 == 0 { 0.25 } else { -0.25 }),
        };
        apply(&mut project, command).expect("moves");
        log.append(&doc.apply_project(&project).expect("changed"))
            .expect("appends");
    }
    let before = std::fs::metadata(&path).expect("log").len();
    assert!(log.read_new().expect("reads").len() > 1500);
    let snapshot = log
        .compact(&doc.encode_state())
        .expect("compacts")
        .expect("not locked");
    let after = std::fs::metadata(&path).expect("log").len();
    eprintln!(
        "log: initial state {initial} bytes; after 1500 edits {before} bytes; compacted {after} (snapshot {})",
        snapshot.len()
    );
    // Overwritten values leave small tombstones in the snapshot (the CRDT needs them to order
    // later concurrent edits); their content is gone.
    assert!(after * 2 < before, "{after} is not much smaller than {before}");
    let initial = initial as u64;
    assert!(
        after - initial < (before - initial) / 3,
        "history shrinks to under a third"
    );
    assert_eq!(
        replay(&path).to_value(),
        doc.to_value(),
        "the snapshot holds the whole document"
    );
    // The compacting reader sees the new generation and just the snapshot.
    assert_eq!(log.read_new().expect("reads").len(), 1);
    assert_eq!(log.generation(), Some(2));
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn a_tailing_reader_survives_the_rewrite() {
    let path = temp_log("tail");
    let mut project = template(Template::Skirt);
    let mut writer_doc = SyncDoc::from_project(&project);
    let mut writer = UpdateLog::open(&path).expect("opens");
    writer.append(&writer_doc.encode_state()).expect("appends");
    let mut reader = UpdateLog::open(&path).expect("opens");
    let mut reader_doc = SyncDoc::new();
    let mut edit = |name: &str, log: &mut UpdateLog, doc: &mut SyncDoc| {
        project.meta.name = name.to_owned();
        log.append(&doc.apply_project(&project).expect("changed"))
            .expect("appends");
    };
    edit("one", &mut writer, &mut writer_doc);
    let r = reader.read_new().expect("reads");
    reader_doc
        .apply_updates(r.iter().map(Vec::as_slice))
        .expect("applies");
    edit("two", &mut writer, &mut writer_doc);
    // Another instance compacts, then the writer keeps going in the new file.
    let mut compactor = UpdateLog::open(&path).expect("opens");
    compactor
        .compact(&SyncDoc::new().encode_state())
        .expect("compacts")
        .expect("not locked");
    edit("three", &mut writer, &mut writer_doc);
    let r = reader.read_new().expect("reads");
    reader_doc
        .apply_updates(r.iter().map(Vec::as_slice))
        .expect("applies");
    assert_eq!(
        reader_doc.to_value(),
        writer_doc.to_value(),
        "nothing skipped across the rewrite"
    );
    assert_eq!(reader.generation(), Some(2));
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn concurrent_writers_lose_nothing_while_the_log_is_compacted() {
    let path = temp_log("stress");
    UpdateLog::open(&path).expect("creates");
    let stop = Arc::new(AtomicBool::new(false));
    let compactions = Arc::new(AtomicUsize::new(0));
    let writers: Vec<_> = (0..2)
        .map(|w| {
            let path = path.clone();
            let compactions = Arc::clone(&compactions);
            std::thread::spawn(move || {
                let mut log = UpdateLog::open(&path).expect("opens");
                let doc = Doc::new();
                let list = doc.get_or_insert_array("list");
                for i in 0..300 {
                    let mut txn = doc.transact_mut();
                    list.push_back(&mut txn, i64::from(w) * 1000 + i);
                    let update = txn.encode_update_v1();
                    drop(txn);
                    log.append(&update).expect("appends");
                    if i % 25 == 0 {
                        // Leave the compactor room to run between bursts of appends.
                        std::thread::sleep(Duration::from_millis(3));
                    }
                    // Halfway, make sure some compactions really happen in the middle.
                    let start = std::time::Instant::now();
                    while i == 150
                        && compactions.load(Ordering::SeqCst) < 2
                        && start.elapsed() < Duration::from_secs(5)
                    {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            })
        })
        .collect();
    let compactor = {
        let (path, stop, count) = (path.clone(), Arc::clone(&stop), Arc::clone(&compactions));
        std::thread::spawn(move || {
            let mut log = UpdateLog::open(&path).expect("opens");
            while !stop.load(Ordering::SeqCst) {
                if log.compact(&[0, 0]).expect("compacts").is_some() {
                    count.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    };
    for w in writers {
        w.join().expect("writer");
    }
    stop.store(true, Ordering::SeqCst);
    compactor.join().expect("compactor");
    let compactions = compactions.load(Ordering::SeqCst);
    assert!(compactions > 1, "compacted during the writes ({compactions}x)");
    let mut log = UpdateLog::open(&path).expect("opens");
    let doc = Doc::new();
    let list = doc.get_or_insert_array("list");
    {
        let mut txn = doc.transact_mut();
        for r in log.read_new().expect("reads") {
            txn.apply_update(Update::decode_v1(&r).expect("decodes"))
                .expect("applies");
        }
    }
    assert_eq!(
        list.len(&doc.transact()),
        600,
        "every update survived {compactions} compactions"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn a_stale_lock_is_broken() {
    let path = temp_log("lock");
    let mut log = UpdateLog::open(&path).expect("opens");
    log.append(&SyncDoc::from_project(&template(Template::Tee)).encode_state())
        .expect("appends");
    let lock = path.with_file_name("garment.azam.json.sync.lock");
    std::fs::write(&lock, "12345\n").expect("lock");
    assert!(
        log.compact(&[0, 0]).expect("runs").is_none(),
        "a fresh lock is respected"
    );
    let file = std::fs::File::options().write(true).open(&lock).expect("lock");
    file.set_modified(SystemTime::now() - Duration::from_secs(120))
        .expect("backdates");
    drop(file);
    assert!(
        log.compact(&[0, 0]).expect("runs").is_some(),
        "a stale lock is broken"
    );
    assert!(!lock.exists(), "the lock is released");
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}
