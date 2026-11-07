//! Live collaboration for azam projects, without a central service.
//!
//! - [`SyncDoc`] mirrors a [`azam_doc::Project`] into a CRDT document (yrs): local edits are
//!   diffed in as minimal changes, remote changes merge and read back as a project.
//! - [`Collab`] runs the network on a background thread: peers on the LAN connect over TCP and
//!   speak the y-sync protocol v1 (sync steps 1 and 2, updates, awareness for presence), find
//!   each other by share code over mDNS/DNS-SD or by `host:port`, and can instead (or as well)
//!   share an append-only [`UpdateLog`] next to the project file. Edits made while disconnected
//!   stay in the replicas and merge on reconnect.
//! - [`rebase`] reverts one collaborator's undo step on top of everyone else's edits.

mod collab;
mod discovery;
mod log;
mod merge;
mod model;
mod net;
mod presence;
mod protocol;
mod repair;

pub use collab::{Collab, Config, JoinTarget, new_share_code};
pub use discovery::{Discovery, Found, SERVICE_TYPE};
pub use log::{COMPACT_BYTES, COMPACT_RECORDS, STALE_LOCK, UpdateLog};
pub use merge::{merge3, rebase};
pub use model::{ORDER_KEY, SyncDoc, SyncError, random_client_id};
pub use net::{NetStatus, Wake};
pub use presence::{PRESENCE_COLORS, Peer, Presence};
pub use protocol::{Hello, MAX_FRAME, MSG_HELLO, decode, frame, read_frame, write_message};
pub use repair::{problems, repair};
