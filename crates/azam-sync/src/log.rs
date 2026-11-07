//! Shared update log: an append-only file of updates next to the project file, so app instances
//! on one machine (or on a shared folder) editing the same project merge their edits.
//!
//! The file starts with a header: `"AZYH"`, the format version (u32, little endian) and a
//! generation number (u64, little endian). Records follow: `"AZYL"`, the update's length (u32,
//! little endian), the update (v1 encoding) and an FNV-1a checksum of the update (u32, little
//! endian). Every record is written with one append, so writers do not interleave; a reader skips
//! anything that does not check out (a record torn by a crash) and picks up at the next record.
//! Files without a header (written by the first version) read as generation 0.
//!
//! Compaction ([`UpdateLog::compact`]) replaces the file with one holding a single snapshot of the
//! merged document under the next generation: written to a temporary file and renamed over the
//! log, under a lock file (broken when older than [`STALE_LOCK`]). No update is lost:
//! - the snapshot merges every record of the old file with the compacting replica's state;
//! - after the rename, the compactor reads the old file to its end again and re-appends records
//!   that other writers added meanwhile;
//! - a writer checks after each append that the log still has the generation it appended to, and
//!   appends again to the new file if it was replaced in between.
//!
//! Readers notice the new generation and read the new file from its start. They may see an update
//! twice (applying an update twice changes nothing) but never miss one.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

const MAGIC: [u8; 4] = *b"AZYL";
const HEADER: usize = 8;
const TRAILER: usize = 4;
const FILE_MAGIC: [u8; 4] = *b"AZYH";
const FILE_HEADER: usize = 16;
const FORMAT: u32 = 1;
/// A log larger than this many bytes is due for compaction.
pub const COMPACT_BYTES: u64 = 4 << 20;
/// A log with more than this many records is due for compaction.
pub const COMPACT_RECORDS: usize = 2000;
/// A compaction lock older than this is taken to belong to a crashed writer and broken.
pub const STALE_LOCK: Duration = Duration::from_secs(30);
/// How many times an append is retried when compactions keep replacing the file under it.
const APPEND_ATTEMPTS: usize = 16;

/// The update log of one project.
#[derive(Debug)]
pub struct UpdateLog {
    path: PathBuf,
    /// Generation of the file being read, once it has been opened.
    generation: Option<u64>,
    /// Bytes of the file read so far.
    offset: u64,
    /// Read bytes not yet forming a whole record.
    pending: Vec<u8>,
    /// Records read from the current generation.
    records: usize,
    stale_lock: Duration,
}

impl UpdateLog {
    /// Where the log of the project stored at `project` lives: next to it, with `.sync` appended.
    #[must_use]
    pub fn path_for(project: &Path) -> PathBuf {
        with_suffix(project, ".sync")
    }

    /// Opens (creating if needed) the log at `path`, positioned at its start.
    ///
    /// # Errors
    /// Fails if the file cannot be created.
    pub fn open(path: &Path) -> io::Result<Self> {
        match OpenOptions::new().append(true).create_new(true).open(path) {
            Ok(mut file) => file.write_all(&file_header(1))?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        Ok(Self {
            path: path.to_owned(),
            generation: None,
            offset: 0,
            pending: Vec::new(),
            records: 0,
            stale_lock: STALE_LOCK,
        })
    }

    /// The log file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Generation of the file as last read (`None` before the first read).
    #[must_use]
    pub fn generation(&self) -> Option<u64> {
        self.generation
    }

    /// Sets how old a compaction lock must be before it is broken (default [`STALE_LOCK`]).
    pub fn set_stale_lock(&mut self, after: Duration) {
        self.stale_lock = after;
    }

    /// Whether the log has grown past [`COMPACT_BYTES`] or [`COMPACT_RECORDS`] (as of the last
    /// read).
    #[must_use]
    pub fn needs_compaction(&self) -> bool {
        self.offset > COMPACT_BYTES || self.records > COMPACT_RECORDS
    }

    /// Updates appended since the last read (by anyone, this writer included). After a
    /// compaction replaced the file, the new file is read from its start.
    ///
    /// # Errors
    /// Fails if the file cannot be read.
    pub fn read_new(&mut self) -> io::Result<Vec<Vec<u8>>> {
        let mut file = File::open(&self.path)?;
        let (generation, header) = read_generation(&mut file)?;
        let len = file.metadata()?.len();
        if self.generation != Some(generation) || len < self.offset {
            self.generation = Some(generation);
            self.offset = header;
            self.pending.clear();
            self.records = 0;
        }
        if len <= self.offset {
            return Ok(Vec::new());
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut fresh = Vec::new();
        file.read_to_end(&mut fresh)?;
        self.offset += fresh.len() as u64;
        self.pending.extend_from_slice(&fresh);
        let (records, used) = parse(&self.pending);
        self.pending.drain(..used);
        self.records += records.len();
        Ok(records)
    }

    /// Appends `update` as one record (again to the new file, if a compaction replaced the log
    /// while it was written).
    ///
    /// # Errors
    /// Fails if the file cannot be written.
    pub fn append(&mut self, update: &[u8]) -> io::Result<()> {
        let record = encode_record(update)?;
        for _ in 0..APPEND_ATTEMPTS {
            let mut file = OpenOptions::new().read(true).append(true).open(&self.path)?;
            let (generation, _) = read_generation(&mut file)?;
            file.write_all(&record)?;
            file.flush()?;
            if current_generation(&self.path)? == generation {
                return Ok(());
            }
        }
        Err(io::Error::other(
            "the update log kept being replaced while appending",
        ))
    }

    /// Rewrites the log as one snapshot of the merged document: every record in the file merged
    /// with `state` (an update holding the compacting replica's whole document). Returns the
    /// snapshot, or `None` when another writer is compacting right now.
    ///
    /// # Errors
    /// Fails if the files cannot be read, written or renamed.
    pub fn compact(&mut self, state: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let Some(_lock) = Lock::acquire(&with_suffix(&self.path, ".lock"), self.stale_lock)? else {
            return Ok(None);
        };
        let mut old = File::open(&self.path)?;
        let (generation, header) = read_generation(&mut old)?;
        old.seek(SeekFrom::Start(header))?;
        let mut buf = Vec::new();
        old.read_to_end(&mut buf)?;
        let (records, used) = parse(&buf);
        let snapshot = merged_state(records.iter().map(Vec::as_slice).chain([state]));

        let tmp = with_suffix(&self.path, &format!(".tmp{}", std::process::id()));
        {
            let mut file = File::create(&tmp)?;
            file.write_all(&file_header(generation + 1))?;
            file.write_all(&encode_record(&snapshot)?)?;
            file.sync_all()?;
        }
        if let Err(e) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        // Records other writers appended to the old file between our read and the rename.
        let mut rest = buf.split_off(used);
        old.read_to_end(&mut rest)?;
        let (late, _) = parse(&rest);
        for update in late {
            self.append(&update)?;
        }
        Ok(Some(snapshot))
    }
}

/// A compaction lock file, removed when dropped.
struct Lock(PathBuf);

impl Lock {
    fn acquire(path: &Path, stale: Duration) -> io::Result<Option<Self>> {
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    return Ok(Some(Self(path.to_owned())));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let age = fs::metadata(path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok());
                    if age.is_some_and(|a| a >= stale) {
                        let _ = fs::remove_file(path);
                    } else {
                        return Ok(None);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

fn file_header(generation: u64) -> [u8; FILE_HEADER] {
    let mut h = [0u8; FILE_HEADER];
    h[..4].copy_from_slice(&FILE_MAGIC);
    h[4..8].copy_from_slice(&FORMAT.to_le_bytes());
    h[8..].copy_from_slice(&generation.to_le_bytes());
    h
}

/// The generation of an open log and the length of its header (0 and 0 without one).
fn read_generation(file: &mut File) -> io::Result<(u64, u64)> {
    file.seek(SeekFrom::Start(0))?;
    let mut h = [0u8; FILE_HEADER];
    let mut got = 0;
    while got < FILE_HEADER {
        match file.read(&mut h[got..])? {
            0 => break,
            n => got += n,
        }
    }
    if got == FILE_HEADER && h[..4] == FILE_MAGIC {
        let mut g = [0u8; 8];
        g.copy_from_slice(&h[8..]);
        Ok((u64::from_le_bytes(g), FILE_HEADER as u64))
    } else {
        Ok((0, 0))
    }
}

fn current_generation(path: &Path) -> io::Result<u64> {
    Ok(read_generation(&mut File::open(path)?)?.0)
}

fn encode_record(update: &[u8]) -> io::Result<Vec<u8>> {
    let len = u32::try_from(update.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "update too large"))?;
    let mut record = Vec::with_capacity(HEADER + update.len() + TRAILER);
    record.extend_from_slice(&MAGIC);
    record.extend_from_slice(&len.to_le_bytes());
    record.extend_from_slice(update);
    record.extend_from_slice(&fnv1a(update).to_le_bytes());
    Ok(record)
}

/// The whole document the updates add up to, as one update. Updates that do not decode are
/// skipped.
fn merged_state<'a>(updates: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let doc = Doc::new();
    {
        let mut txn = doc.transact_mut();
        for bytes in updates {
            if let Ok(update) = Update::decode_v1(bytes) {
                let _ = txn.apply_update(update);
            }
        }
    }
    doc.transact().encode_state_as_update_v1(&StateVector::default())
}

/// The whole records at the start of `buf` and how many bytes they (and skipped garbage) use.
fn parse(buf: &[u8]) -> (Vec<Vec<u8>>, usize) {
    let mut records = Vec::new();
    let mut at = 0;
    while buf.len() - at >= HEADER {
        if buf[at..at + 4] != MAGIC {
            at += 1;
            continue;
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&buf[at + 4..at + 8]);
        let len = u32::from_le_bytes(len_bytes) as usize;
        let end = at + HEADER + len + TRAILER;
        if end > buf.len() {
            break;
        }
        let body = &buf[at + HEADER..at + HEADER + len];
        let mut sum = [0u8; 4];
        sum.copy_from_slice(&buf[end - TRAILER..end]);
        if u32::from_le_bytes(sum) == fnv1a(body) {
            records.push(body.to_vec());
            at = end;
        } else {
            at += 1;
        }
    }
    (records, at)
}

fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5_u32, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torn_records_are_skipped() {
        let mut buf = Vec::new();
        for (i, body) in [b"one".as_slice(), b"two", b"three"].iter().enumerate() {
            buf.extend_from_slice(&MAGIC);
            buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
            buf.extend_from_slice(body);
            let sum = if i == 1 { 0 } else { fnv1a(body) };
            buf.extend_from_slice(&sum.to_le_bytes());
        }
        let (records, used) = parse(&buf);
        assert_eq!(records, vec![b"one".to_vec(), b"three".to_vec()]);
        assert_eq!(used, buf.len());
        let (records, used) = parse(&buf[..buf.len() - 2]);
        assert_eq!(records.len(), 1);
        assert!(used < buf.len() - 2);
    }
}
