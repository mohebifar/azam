//! The network thread: one replica of the document that every connection, the shared update log
//! and the app talk to.
//!
//! Threads: the engine (owns the replica, the awareness states and the sockets' write halves), a
//! listener, one reader per connection and short-lived dialers. They talk to the engine over one
//! channel; the engine talks to the app over another. No async runtime.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{self, BufReader};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use yrs::sync::{Awareness, Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::{ClientID, Doc, ReadTxn, Transact, Update};

use crate::discovery::Discovery;
use crate::log::UpdateLog;
use crate::model::random_client_id;
use crate::presence::{Peer, Presence};
use crate::protocol::{Hello, decode, frame, read_frame};

/// Longest wait for an event before housekeeping runs.
const TICK: Duration = Duration::from_millis(20);
/// How often the shared update log is checked for new records.
const LOG_POLL: Duration = Duration::from_millis(150);
/// How often our awareness state is re-sent so others know we are still here.
const AWARENESS_RENEW: Duration = Duration::from_secs(10);
/// Awareness states not renewed for this long are dropped, in ms.
const AWARENESS_TIMEOUT_MS: u64 = 30_000;
/// Time allowed for a TCP connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Time allowed for a write before the connection is dropped.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// First and longest wait before re-dialling a peer.
const BACKOFF_MIN: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(5);
/// A discovered peer with a lower id is expected to dial us; dial it ourselves after this.
const DISCOVERY_GRACE: Duration = Duration::from_secs(3);
/// Shortest time between two broadcasts of our awareness state (the 3D cursor moves often).
const PRESENCE_INTERVAL: Duration = Duration::from_millis(50);
/// Update hashes remembered for de-duplicating relays.
const SEEN_LIMIT: usize = 8192;

/// State of the network side of a session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetStatus {
    /// Share code of the session (empty until learned when joining by address).
    pub code: String,
    /// Address the session accepts connections on.
    pub listen: Option<SocketAddr>,
    /// Connected peers.
    pub connections: usize,
    /// Whether the session is connecting (`false` while working offline).
    pub online: bool,
    /// Whether LAN discovery runs.
    pub discovery: bool,
    /// Shared update log, if any.
    pub log: Option<PathBuf>,
    /// Last problem worth showing.
    pub error: Option<String>,
}

/// Something for the engine to handle.
pub(crate) enum Event {
    /// A change made by the app (update, v1 encoding).
    Local(Vec<u8>),
    /// New awareness state of the app's user (JSON).
    Presence(String),
    /// Dial an address and keep reconnecting to it.
    Connect(SocketAddr),
    /// Go offline (close every connection, stop dialling) or back online.
    SetOnline(bool),
    /// Rewrite the shared update log as one snapshot (after a save, say).
    CompactLog,
    /// Stop the engine.
    Shutdown,
    Accepted(TcpStream),
    Dialed(SocketAddr, io::Result<TcpStream>),
    Frame(u64, Vec<u8>),
    Closed(u64),
}

/// Something for the app.
pub(crate) enum Notice {
    /// A change from elsewhere (update, v1 encoding).
    Remote(Vec<u8>),
    /// The other collaborators.
    Peers(Vec<Peer>),
    /// New network status.
    Status(NetStatus),
}

/// How the engine starts.
pub(crate) struct EngineConfig {
    pub code: String,
    pub listen: Option<SocketAddr>,
    pub discovery: bool,
    pub connect: Vec<SocketAddr>,
    pub log: Option<UpdateLog>,
    /// Updates the replica starts with (already known to the app).
    pub initial: Vec<Vec<u8>>,
    pub presence: Presence,
    /// Called (from the engine thread) when notices are waiting for the app.
    pub wake: Option<Wake>,
    /// Set when the app was woken and has not polled since; cleared by the app.
    pub wake_pending: Arc<AtomicBool>,
}

/// A callback that tells the app, from any thread, that the session has news (remote changes,
/// presence or connection changes), so it can redraw and poll only then.
#[derive(Clone)]
pub struct Wake(Arc<dyn Fn() + Send + Sync>);

impl Wake {
    /// Wraps `f`.
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    fn call(&self) {
        (self.0)();
    }
}

impl std::fmt::Debug for Wake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wake")
    }
}

/// A running engine.
pub(crate) struct EngineHandle {
    pub events: Sender<Event>,
    pub notices: Receiver<Notice>,
    pub thread: JoinHandle<()>,
    pub status: NetStatus,
}

/// Starts the engine thread (and the listener, if any).
pub(crate) fn spawn(cfg: EngineConfig) -> io::Result<EngineHandle> {
    let (events, rx) = mpsc::channel();
    let (notices_tx, notices) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let listen = match cfg.listen {
        Some(addr) => {
            let listener = TcpListener::bind(addr)?;
            let local = listener.local_addr()?;
            spawn_listener(listener, events.clone(), Arc::clone(&stop))?;
            Some(local)
        }
        None => None,
    };
    let doc = Doc::with_client_id(random_client_id());
    let awareness = Awareness::new(doc);
    let peer_id = awareness.client_id().get();
    let mut engine = Engine {
        awareness,
        peer_id,
        code: cfg.code,
        events: events.clone(),
        notices: notices_tx,
        conns: HashMap::new(),
        next_conn: 1,
        targets: Vec::new(),
        seen: HashSet::new(),
        seen_order: VecDeque::new(),
        online: true,
        listen,
        stop,
        log: cfg.log,
        next_log_poll: Instant::now(),
        want_discovery: cfg.discovery,
        discovery: None,
        discovery_failed: false,
        last_renew: Instant::now(),
        error: None,
        sent_status: None,
        sent_peers: Vec::new(),
        wake: cfg.wake,
        wake_pending: cfg.wake_pending,
        noticed: false,
        presence_dirty: false,
        presence_sent: Instant::now(),
    };
    for update in cfg.initial {
        engine.integrate(update, Source::Initial);
    }
    if let Ok(json) = serde_json::to_string(&cfg.presence) {
        engine.awareness.set_local_state_raw(json);
    }
    for addr in cfg.connect {
        engine.add_target(addr, None, Instant::now());
    }
    let status = engine.status();
    let thread = thread::Builder::new()
        .name("azam-sync".to_owned())
        .spawn(move || engine.run(&rx))?;
    Ok(EngineHandle {
        events,
        notices,
        thread,
        status,
    })
}

fn spawn_listener(listener: TcpListener, events: Sender<Event>, stop: Arc<AtomicBool>) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    thread::Builder::new()
        .name("azam-sync-listen".to_owned())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        if events.send(Event::Accepted(stream)).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(15));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(100)),
                }
            }
        })?;
    Ok(())
}

/// Where an update came from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Initial,
    App,
    Log,
    Conn(u64),
}

struct Conn {
    writer: TcpStream,
    /// The peer's hello, once received and accepted.
    hello: Option<Hello>,
    /// The address we dialled, for connections we opened.
    dialed: Option<SocketAddr>,
    /// Nonce we sent in our hello.
    nonce: u64,
}

struct Target {
    addr: SocketAddr,
    /// Peer known to listen there.
    peer: Option<u64>,
    next: Instant,
    backoff: Duration,
    dialing: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent state flags of the engine loop"
)]
struct Engine {
    awareness: Awareness,
    peer_id: u64,
    code: String,
    events: Sender<Event>,
    notices: Sender<Notice>,
    conns: HashMap<u64, Conn>,
    next_conn: u64,
    targets: Vec<Target>,
    seen: HashSet<u64>,
    seen_order: VecDeque<u64>,
    online: bool,
    listen: Option<SocketAddr>,
    stop: Arc<AtomicBool>,
    log: Option<UpdateLog>,
    next_log_poll: Instant,
    want_discovery: bool,
    discovery: Option<Discovery>,
    discovery_failed: bool,
    last_renew: Instant,
    error: Option<String>,
    sent_status: Option<NetStatus>,
    sent_peers: Vec<Peer>,
    wake: Option<Wake>,
    wake_pending: Arc<AtomicBool>,
    /// Whether notices were sent since the app was last woken.
    noticed: bool,
    /// Whether our awareness state changed since it was last broadcast.
    presence_dirty: bool,
    presence_sent: Instant,
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl Engine {
    fn run(mut self, rx: &Receiver<Event>) {
        self.publish();
        loop {
            match rx.recv_timeout(TICK) {
                Ok(Event::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(event) => self.handle(event),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.housekeeping();
            self.publish();
            self.wake_app();
        }
        self.shutdown();
    }

    /// Sends `notice` to the app.
    fn notify(&mut self, notice: Notice) {
        if self.notices.send(notice).is_ok() {
            self.noticed = true;
        }
    }

    /// Wakes the app if notices are waiting and it has not been woken for them yet (at most one
    /// wake is outstanding; [`crate::Collab::poll`] re-arms it).
    fn wake_app(&mut self) {
        if std::mem::take(&mut self.noticed)
            && !self.wake_pending.swap(true, Ordering::AcqRel)
            && let Some(wake) = &self.wake
        {
            wake.call();
        }
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Local(update) => self.integrate(update, Source::App),
            Event::Presence(json) => {
                // Broadcast from housekeeping, at most every PRESENCE_INTERVAL.
                self.awareness.set_local_state_raw(json);
                self.presence_dirty = true;
            }
            Event::Connect(addr) => self.add_target(addr, None, Instant::now()),
            Event::SetOnline(online) => self.set_online(online),
            Event::CompactLog => self.compact_log(),
            Event::Shutdown => {}
            Event::Accepted(stream) => {
                if self.online {
                    self.add_conn(stream, None);
                } else {
                    let _ = stream.shutdown(Shutdown::Both);
                }
            }
            Event::Dialed(addr, result) => self.on_dialed(addr, result),
            Event::Frame(id, body) => self.on_frame(id, &body),
            Event::Closed(id) => self.close(id),
        }
    }

    fn status(&self) -> NetStatus {
        NetStatus {
            code: self.code.clone(),
            listen: self.listen,
            connections: self.conns.values().filter(|c| c.hello.is_some()).count(),
            online: self.online,
            discovery: self.discovery.is_some(),
            log: self.log.as_ref().map(|l| l.path().to_owned()),
            error: self.error.clone(),
        }
    }

    /// Sends the status and the peer list to the app when they changed.
    fn publish(&mut self) {
        let status = self.status();
        if self.sent_status.as_ref() != Some(&status) {
            self.notify(Notice::Status(status.clone()));
            self.sent_status = Some(status);
        }
        let me = self.awareness.client_id();
        let mut peers: Vec<Peer> = self
            .awareness
            .iter()
            .filter(|(id, _)| *id != me)
            .filter_map(|(id, state)| {
                let presence = serde_json::from_str::<Presence>(state.data.as_deref()?).ok()?;
                Some(Peer {
                    client: id.get(),
                    presence,
                })
            })
            .collect();
        peers.sort_by_key(|p| p.client);
        if peers != self.sent_peers {
            self.notify(Notice::Peers(peers.clone()));
            self.sent_peers = peers;
        }
    }

    /// Remembers an update hash; returns `false` if it was already seen.
    fn mark_seen(&mut self, h: u64) -> bool {
        if !self.seen.insert(h) {
            return false;
        }
        self.seen_order.push_back(h);
        if self.seen_order.len() > SEEN_LIMIT
            && let Some(old) = self.seen_order.pop_front()
        {
            self.seen.remove(&old);
        }
        true
    }

    /// Applies an update to the replica and passes it on to everyone who has not seen it.
    fn integrate(&mut self, update: Vec<u8>, from: Source) {
        if !self.mark_seen(hash_bytes(&update)) {
            return;
        }
        let applied = Update::decode_v1(&update)
            .map_err(|e| e.to_string())
            .and_then(|u| {
                self.awareness
                    .doc()
                    .transact_mut()
                    .apply_update(u)
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = applied {
            self.error = Some(format!("dropped an invalid update: {e}"));
            return;
        }
        if matches!(from, Source::Log | Source::Conn(_)) {
            self.notify(Notice::Remote(update.clone()));
        }
        if matches!(from, Source::App | Source::Conn(_))
            && let Some(log) = &mut self.log
            && let Err(e) = log.append(&update)
        {
            self.error = Some(format!("could not write the update log: {e}"));
        }
        if from != Source::Initial {
            let msg = Message::Sync(SyncMessage::Update(update));
            let skip = match from {
                Source::Conn(id) => Some(id),
                _ => None,
            };
            self.broadcast(&msg, skip);
        }
    }

    /// Sends `msg` to every connected peer except `skip`.
    fn broadcast(&mut self, msg: &Message, skip: Option<u64>) {
        let bytes = frame(msg);
        let ids: Vec<u64> = self
            .conns
            .iter()
            .filter(|(id, c)| c.hello.is_some() && Some(**id) != skip)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.send_bytes(id, &bytes);
        }
    }

    fn send(&mut self, id: u64, msg: &Message) {
        self.send_bytes(id, &frame(msg));
    }

    fn send_bytes(&mut self, id: u64, bytes: &[u8]) {
        let failed = match self.conns.get_mut(&id) {
            Some(conn) => io::Write::write_all(&mut conn.writer, bytes).is_err(),
            None => false,
        };
        if failed {
            self.close(id);
        }
    }

    fn broadcast_own_awareness(&mut self) {
        if let Ok(update) = self.awareness.update_with_clients([self.awareness.client_id()]) {
            self.broadcast(&Message::Awareness(update), None);
        }
    }

    fn add_conn(&mut self, stream: TcpStream, dialed: Option<SocketAddr>) {
        let _ = stream.set_nodelay(true);
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let Ok(reader) = stream.try_clone() else {
            return;
        };
        let id = self.next_conn;
        self.next_conn += 1;
        let events = self.events.clone();
        let spawned = thread::Builder::new()
            .name(format!("azam-sync-conn-{id}"))
            .spawn(move || {
                let mut r = BufReader::new(reader);
                loop {
                    let Ok(body) = read_frame(&mut r) else {
                        let _ = events.send(Event::Closed(id));
                        break;
                    };
                    if events.send(Event::Frame(id, body)).is_err() {
                        break;
                    }
                }
            });
        if spawned.is_err() {
            return;
        }
        let nonce = random_client_id();
        self.conns.insert(
            id,
            Conn {
                writer: stream,
                hello: None,
                dialed,
                nonce,
            },
        );
        let mut hello = Hello::new(&self.code, self.peer_id, self.awareness.client_id().get());
        hello.nonce = nonce;
        self.send(id, &hello.to_message());
    }

    fn close(&mut self, id: u64) {
        let Some(conn) = self.conns.remove(&id) else {
            return;
        };
        let _ = conn.writer.shutdown(Shutdown::Both);
        if let Some(hello) = &conn.hello {
            let still_connected = self
                .conns
                .values()
                .any(|c| c.hello.as_ref().is_some_and(|h| h.peer == hello.peer));
            if !still_connected {
                self.awareness.remove_state(ClientID::new(hello.awareness));
            }
        }
        if let Some(addr) = conn.dialed
            && let Some(t) = self.targets.iter_mut().find(|t| t.addr == addr)
        {
            t.next = Instant::now() + t.backoff;
            t.backoff = (t.backoff * 2).min(BACKOFF_MAX);
        }
    }

    fn set_online(&mut self, online: bool) {
        self.online = online;
        if online {
            let now = Instant::now();
            for t in &mut self.targets {
                t.next = now;
                t.backoff = BACKOFF_MIN;
            }
        } else {
            let ids: Vec<u64> = self.conns.keys().copied().collect();
            for id in ids {
                self.close(id);
            }
        }
    }

    fn add_target(&mut self, addr: SocketAddr, peer: Option<u64>, next: Instant) {
        if Some(addr) == self.listen {
            return;
        }
        if let Some(t) = self.targets.iter_mut().find(|t| t.addr == addr) {
            if peer.is_some() {
                t.peer = peer;
            }
            return;
        }
        self.targets.push(Target {
            addr,
            peer,
            next,
            backoff: BACKOFF_MIN,
            dialing: false,
        });
    }

    fn on_dialed(&mut self, addr: SocketAddr, result: io::Result<TcpStream>) {
        let Some(t) = self.targets.iter_mut().find(|t| t.addr == addr) else {
            return;
        };
        t.dialing = false;
        match result {
            Ok(stream) if self.online => {
                t.backoff = BACKOFF_MIN;
                self.add_conn(stream, Some(addr));
            }
            Ok(stream) => {
                let _ = stream.shutdown(Shutdown::Both);
            }
            Err(_) => {
                t.next = Instant::now() + t.backoff;
                t.backoff = (t.backoff * 2).min(BACKOFF_MAX);
            }
        }
    }

    fn connected_to(&self, peer: u64) -> bool {
        self.conns
            .values()
            .any(|c| c.hello.as_ref().is_some_and(|h| h.peer == peer))
    }

    fn on_frame(&mut self, id: u64, body: &[u8]) {
        let Ok(msg) = decode(body) else {
            self.close(id);
            return;
        };
        let Some(conn) = self.conns.get(&id) else {
            return;
        };
        if conn.hello.is_none() {
            match Hello::from_message(&msg) {
                Some(hello) => self.on_hello(id, hello),
                None => self.close(id),
            }
            return;
        }
        match msg {
            Message::Sync(SyncMessage::SyncStep1(sv)) => {
                let diff = self.awareness.doc().transact().encode_diff_v1(&sv);
                self.send(id, &Message::Sync(SyncMessage::SyncStep2(diff)));
            }
            Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => {
                self.integrate(update, Source::Conn(id));
            }
            Message::Awareness(update) => {
                let changed = self
                    .awareness
                    .apply_update_summary(update.clone())
                    .ok()
                    .flatten()
                    .is_some_and(|s| !s.all_changes().is_empty());
                if changed {
                    self.broadcast(&Message::Awareness(update), Some(id));
                }
            }
            Message::AwarenessQuery => {
                if let Ok(update) = self.awareness.update() {
                    self.send(id, &Message::Awareness(update));
                }
            }
            _ => {}
        }
    }

    /// Rank of a connection for choosing between two to the same peer: the one dialled by the
    /// lower peer id wins, then the lower nonce of its dialler. Both ends compute the same rank.
    fn rank(&self, conn: &Conn, hello: &Hello) -> (bool, u64) {
        let (dialer, nonce) = if conn.dialed.is_some() {
            (self.peer_id, conn.nonce)
        } else {
            (hello.peer, hello.nonce)
        };
        (dialer != self.peer_id.min(hello.peer), nonce)
    }

    fn on_hello(&mut self, id: u64, hello: Hello) {
        let code_mismatch = !self.code.is_empty() && !hello.code.is_empty() && hello.code != self.code;
        if hello.peer == self.peer_id || code_mismatch {
            if code_mismatch {
                self.error = Some(format!("a peer is in another session ({})", hello.code));
            }
            self.close(id);
            return;
        }
        if self.code.is_empty() {
            self.code.clone_from(&hello.code);
        }
        // Keep one connection per peer.
        let duplicate = self
            .conns
            .iter()
            .find(|(cid, c)| **cid != id && c.hello.as_ref().is_some_and(|h| h.peer == hello.peer))
            .map(|(cid, _)| *cid);
        if let (Some(other), Some(conn)) = (duplicate, self.conns.get(&id)) {
            let new_rank = self.rank(conn, &hello);
            let old_rank = self
                .conns
                .get(&other)
                .and_then(|c| c.hello.as_ref().map(|h| self.rank(c, h)));
            if old_rank.is_some_and(|r| r <= new_rank) {
                self.mark_target_peer(id, hello.peer);
                self.close(id);
                return;
            }
            self.close(other);
        }
        self.mark_target_peer(id, hello.peer);
        if let Some(conn) = self.conns.get_mut(&id) {
            conn.hello = Some(hello);
        }
        let sv = self.awareness.doc().transact().state_vector();
        self.send(id, &Message::Sync(SyncMessage::SyncStep1(sv)));
        if let Ok(update) = self.awareness.update() {
            self.send(id, &Message::Awareness(update));
        }
    }

    fn mark_target_peer(&mut self, id: u64, peer: u64) {
        if let Some(addr) = self.conns.get(&id).and_then(|c| c.dialed)
            && let Some(t) = self.targets.iter_mut().find(|t| t.addr == addr)
        {
            t.peer = Some(peer);
        }
    }

    fn housekeeping(&mut self) {
        let now = Instant::now();
        self.dial_due(now);
        self.poll_log(now);
        self.poll_discovery(now);
        self.awareness_timers(now);
    }

    fn dial_due(&mut self, now: Instant) {
        if !self.online {
            return;
        }
        let busy: Vec<bool> = self
            .targets
            .iter()
            .map(|t| {
                t.dialing
                    || t.next > now
                    || t.peer.is_some_and(|p| self.connected_to(p))
                    || self.conns.values().any(|c| c.dialed == Some(t.addr))
            })
            .collect();
        for (t, busy) in self.targets.iter_mut().zip(busy) {
            if busy {
                continue;
            }
            t.dialing = true;
            let addr = t.addr;
            let events = self.events.clone();
            let spawned = thread::Builder::new()
                .name("azam-sync-dial".to_owned())
                .spawn(move || {
                    let result = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT);
                    let _ = events.send(Event::Dialed(addr, result));
                });
            if spawned.is_err() {
                t.dialing = false;
                t.next = now + BACKOFF_MAX;
            }
        }
    }

    fn poll_log(&mut self, now: Instant) {
        if now < self.next_log_poll {
            return;
        }
        self.next_log_poll = now + LOG_POLL;
        let Some(log) = &mut self.log else {
            return;
        };
        match log.read_new() {
            Ok(records) => {
                for update in records {
                    self.integrate(update, Source::Log);
                }
            }
            Err(e) => self.error = Some(format!("could not read the update log: {e}")),
        }
        if self.log.as_ref().is_some_and(UpdateLog::needs_compaction) {
            self.compact_log();
        }
    }

    /// Rewrites the shared update log as one snapshot of the document, unless another writer is
    /// compacting it right now.
    fn compact_log(&mut self) {
        let Some(log) = &mut self.log else {
            return;
        };
        let state = self
            .awareness
            .doc()
            .transact()
            .encode_state_as_update_v1(&yrs::StateVector::default());
        match log.compact(&state) {
            // Our own reader will meet the snapshot in the new file: nothing new for anyone.
            Ok(Some(snapshot)) => {
                self.mark_seen(hash_bytes(&snapshot));
            }
            Ok(None) => {}
            Err(e) => self.error = Some(format!("could not compact the update log: {e}")),
        }
    }

    fn poll_discovery(&mut self, now: Instant) {
        if self.want_discovery
            && self.discovery.is_none()
            && !self.discovery_failed
            && !self.code.is_empty()
            && let Some(listen) = self.listen
        {
            match Discovery::start(&self.code, self.peer_id, listen.port()) {
                Ok(d) => self.discovery = Some(d),
                Err(e) => {
                    self.discovery_failed = true;
                    self.error = Some(format!("LAN discovery is unavailable: {e}"));
                }
            }
        }
        let found = match &mut self.discovery {
            Some(d) => d.poll(),
            None => return,
        };
        for f in found {
            let next = if self.peer_id < f.peer {
                now
            } else {
                now + DISCOVERY_GRACE
            };
            for addr in f.addrs {
                self.add_target(addr, Some(f.peer), next);
            }
        }
    }

    fn awareness_timers(&mut self, now: Instant) {
        if self.presence_dirty && now.duration_since(self.presence_sent) >= PRESENCE_INTERVAL {
            self.presence_dirty = false;
            self.presence_sent = now;
            self.last_renew = now;
            self.broadcast_own_awareness();
        }
        if now.duration_since(self.last_renew) >= AWARENESS_RENEW {
            self.last_renew = now;
            if let Some(state) = self.awareness.local_state_raw() {
                self.awareness.set_local_state_raw(state);
                self.broadcast_own_awareness();
            }
        }
        let me = self.awareness.client_id();
        let cutoff = now_ms().saturating_sub(AWARENESS_TIMEOUT_MS);
        let stale: Vec<ClientID> = self
            .awareness
            .iter()
            .filter(|(id, s)| *id != me && s.data.is_some() && s.last_updated < cutoff)
            .map(|(id, _)| id)
            .collect();
        for id in stale {
            self.awareness.remove_state(id);
        }
    }

    fn shutdown(&mut self) {
        self.awareness.clean_local_state();
        self.broadcast_own_awareness();
        let ids: Vec<u64> = self.conns.keys().copied().collect();
        for id in ids {
            self.close(id);
        }
        self.stop.store(true, Ordering::Relaxed);
        self.discovery = None;
    }
}
