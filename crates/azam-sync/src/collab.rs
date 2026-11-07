//! The app-facing collaboration session: the app's replica plus the network thread.

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use azam_doc::Project;

use crate::log::UpdateLog;
use crate::model::{SyncDoc, SyncError, random_client_id};
use crate::net::{EngineConfig, Event, NetStatus, Notice, Wake, spawn};
use crate::presence::{Peer, Presence};

/// Letters of share codes (no look-alikes).
const CODE_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// Where to join a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinTarget {
    /// A peer's address.
    Addr(SocketAddr),
    /// A share code, looked up on the LAN.
    Code(String),
}

impl JoinTarget {
    /// Reads `host:port` (resolved now) or a share code.
    ///
    /// # Errors
    /// Fails for empty text or an address that does not resolve.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("enter host:port or a share code".to_owned());
        }
        if text.contains(':') {
            return text
                .to_socket_addrs()
                .map_err(|e| format!("cannot resolve {text}: {e}"))?
                .next()
                .map(Self::Addr)
                .ok_or_else(|| format!("cannot resolve {text}"));
        }
        Ok(Self::Code(text.to_lowercase()))
    }
}

/// How a session runs.
#[derive(Clone, Debug)]
pub struct Config {
    /// What this user shares about themselves.
    pub presence: Presence,
    /// Address to accept connections on (`None`: do not listen).
    pub listen: Option<SocketAddr>,
    /// Whether to advertise and look for peers with mDNS/DNS-SD.
    pub discovery: bool,
    /// Called from the network thread when [`Collab::poll`] has something to take in, so the app
    /// can sleep until then instead of polling every frame. Called at most once per poll.
    pub wake: Option<Wake>,
}

impl Default for Config {
    fn default() -> Self {
        let name = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "Guest".to_owned());
        Self {
            presence: Presence::named(&name, random_client_id()),
            listen: Some(SocketAddr::from(([0, 0, 0, 0], 0))),
            discovery: true,
            wake: None,
        }
    }
}

/// A new random share code, like `k7m-q2x`.
#[must_use]
pub fn new_share_code() -> String {
    let mut n = random_client_id();
    let mut code = String::with_capacity(7);
    for i in 0..6 {
        if i == 3 {
            code.push('-');
        }
        code.push(char::from(
            CODE_ALPHABET[(n % CODE_ALPHABET.len() as u64) as usize],
        ));
        n /= CODE_ALPHABET.len() as u64;
    }
    code
}

/// A live collaboration session.
///
/// The app pushes every local change with [`Collab::local_edit`] and calls [`Collab::poll`]
/// between frames to pick up remote changes. Changes made while no peer is connected stay in the
/// replicas and merge when a connection (re)opens.
pub struct Collab {
    doc: SyncDoc,
    events: Sender<Event>,
    notices: Receiver<Notice>,
    thread: Option<JoinHandle<()>>,
    peers: Vec<Peer>,
    status: NetStatus,
    presence: Presence,
    wake_pending: Arc<AtomicBool>,
}

impl std::fmt::Debug for Collab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collab")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Collab {
    /// Shares `project` as session `code` (a new code when `None`).
    ///
    /// # Errors
    /// Fails if the listening socket cannot be opened.
    pub fn host(project: &Project, code: Option<&str>, config: Config) -> Result<Self, SyncError> {
        let doc = SyncDoc::from_project(project);
        let initial = vec![doc.encode_state()];
        let code = code.map_or_else(new_share_code, str::to_owned);
        Self::start(doc, code, Vec::new(), None, initial, config)
    }

    /// Joins a session. The project arrives with the first [`Collab::poll`] after connecting.
    ///
    /// # Errors
    /// Fails if the listening socket cannot be opened.
    pub fn join(target: JoinTarget, mut config: Config) -> Result<Self, SyncError> {
        let (code, connect) = match target {
            JoinTarget::Addr(addr) => (String::new(), vec![addr]),
            JoinTarget::Code(code) => {
                config.discovery = true;
                (code, Vec::new())
            }
        };
        Self::start(SyncDoc::new(), code, connect, None, Vec::new(), config)
    }

    /// Shares the project stored at `project_path` through an update log next to it, so app
    /// instances opening the same file merge their edits (see [`UpdateLog`]). When the log
    /// already holds a document, that document wins and is returned for the app to show;
    /// otherwise the log is started from `project`.
    ///
    /// # Errors
    /// Fails if the log cannot be read or written, or holds something that is not a project.
    pub fn open_log(
        project_path: &Path,
        project: &Project,
        config: Config,
    ) -> Result<(Self, Option<Project>), SyncError> {
        let mut log = UpdateLog::open(&UpdateLog::path_for(project_path))?;
        let records = log.read_new()?;
        let (doc, adopted, initial) = if records.is_empty() {
            let doc = SyncDoc::from_project(project);
            let state = doc.encode_state();
            log.append(&state)?;
            (doc, None, vec![state])
        } else {
            let mut doc = SyncDoc::new();
            doc.apply_updates(records.iter().map(Vec::as_slice))?;
            let adopted = doc.to_project()?;
            (doc, Some(adopted), records)
        };
        let code = project_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let collab = Self::start(doc, code, Vec::new(), Some(log), initial, config)?;
        Ok((collab, adopted))
    }

    fn start(
        doc: SyncDoc,
        code: String,
        connect: Vec<SocketAddr>,
        log: Option<UpdateLog>,
        initial: Vec<Vec<u8>>,
        config: Config,
    ) -> Result<Self, SyncError> {
        let wake_pending = Arc::new(AtomicBool::new(false));
        let handle = spawn(EngineConfig {
            code,
            listen: config.listen,
            discovery: config.discovery,
            connect,
            log,
            initial,
            presence: config.presence.clone(),
            wake: config.wake,
            wake_pending: Arc::clone(&wake_pending),
        })?;
        Ok(Self {
            doc,
            events: handle.events,
            notices: handle.notices,
            thread: Some(handle.thread),
            peers: Vec::new(),
            status: handle.status,
            presence: config.presence,
            wake_pending,
        })
    }

    /// Pushes the changes that turn the shared document into `project` (after a local edit,
    /// undo or redo). Points `project`'s id generator at this replica's own id range first.
    /// Returns the size of the update sent in bytes, or `None` when nothing changed or the
    /// session has no document yet (still joining).
    pub fn local_edit(&mut self, project: &mut Project) -> Option<usize> {
        if self.doc.is_empty() {
            return None;
        }
        self.doc.claim_ids(project);
        let update = self.doc.apply_project(project)?;
        let len = update.len();
        let _ = self.events.send(Event::Local(update));
        Some(len)
    }

    /// Takes in what arrived from the network. Returns the merged project when remote changes
    /// changed the document; the app shows it in place of its own.
    pub fn poll(&mut self) -> Option<Project> {
        // Re-arm the wake before draining, so nothing sent after this point goes unnoticed.
        self.wake_pending.store(false, Ordering::Release);
        let mut updates = Vec::new();
        while let Ok(notice) = self.notices.try_recv() {
            match notice {
                Notice::Remote(u) => updates.push(u),
                Notice::Peers(p) => self.peers = p,
                Notice::Status(s) => self.status = s,
            }
        }
        if updates.is_empty() {
            return None;
        }
        match self.doc.apply_updates(updates.iter().map(Vec::as_slice)) {
            Ok(true) => {}
            Ok(false) => return None,
            Err(e) => {
                self.status.error = Some(e.to_string());
                return None;
            }
        }
        let project = match self.doc.to_project() {
            Ok(p) => p,
            Err(e) => {
                self.status.error = Some(e.to_string());
                return None;
            }
        };
        // Share the repairs, so every replica stores what it shows.
        if let Some(fix) = self.doc.apply_project(&project) {
            let _ = self.events.send(Event::Local(fix));
        }
        Some(project)
    }

    /// Updates what this user shares (sent only when it changed).
    pub fn set_presence(&mut self, presence: Presence) {
        if presence == self.presence {
            return;
        }
        if let Ok(json) = serde_json::to_string(&presence) {
            let _ = self.events.send(Event::Presence(json));
        }
        self.presence = presence;
    }

    /// What this user shares.
    #[must_use]
    pub fn presence(&self) -> &Presence {
        &self.presence
    }

    /// The other collaborators, as of the last [`Collab::poll`].
    #[must_use]
    pub fn peers(&self) -> &[Peer] {
        &self.peers
    }

    /// Network status, as of the last [`Collab::poll`].
    #[must_use]
    pub fn status(&self) -> &NetStatus {
        &self.status
    }

    /// Whether the shared document has arrived (always true when hosting).
    #[must_use]
    pub fn has_document(&self) -> bool {
        !self.doc.is_empty()
    }

    /// The app's replica.
    #[must_use]
    pub fn doc(&self) -> &SyncDoc {
        &self.doc
    }

    /// Works offline (`false`: connections close, edits queue) or reconnects (`true`).
    pub fn set_online(&self, online: bool) {
        let _ = self.events.send(Event::SetOnline(online));
    }

    /// Rewrites the shared update log (if any) as one snapshot of the document. Also done
    /// automatically once the log grows past [`crate::COMPACT_BYTES`] or
    /// [`crate::COMPACT_RECORDS`] records.
    pub fn compact_log(&self) {
        let _ = self.events.send(Event::CompactLog);
    }

    /// Connects to another peer of the session (and keeps reconnecting to it).
    pub fn connect(&self, addr: SocketAddr) {
        let _ = self.events.send(Event::Connect(addr));
    }
}

impl Drop for Collab {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
