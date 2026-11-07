//! LAN discovery over mDNS/DNS-SD: every peer of a session advertises `_azam._tcp` with the share
//! code and its peer id in TXT records, and browses for the others.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

/// DNS-SD service type of azam sessions.
pub const SERVICE_TYPE: &str = "_azam._tcp.local.";

/// A peer found on the LAN.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Its peer id.
    pub peer: u64,
    /// Addresses it listens on.
    pub addrs: Vec<SocketAddr>,
}

/// Advertising and browsing for one session.
pub struct Discovery {
    daemon: ServiceDaemon,
    browse: mdns_sd::Receiver<ServiceEvent>,
    code: String,
    peer: u64,
    fullname: String,
    found: HashMap<String, Found>,
}

impl std::fmt::Debug for Discovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Discovery")
            .field("code", &self.code)
            .finish_non_exhaustive()
    }
}

impl Discovery {
    /// Advertises peer `peer` of session `code` listening on `port`, and starts browsing.
    ///
    /// # Errors
    /// Fails if the mDNS daemon cannot start (no multicast-capable interface, say).
    pub fn start(code: &str, peer: u64, port: u16) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let instance = format!("azam-{peer:x}");
        let host = format!("{instance}.local.");
        let peer_txt = peer.to_string();
        let props = [("code", code), ("peer", peer_txt.as_str())];
        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host, "", port, &props[..])
            .map_err(|e| e.to_string())?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        daemon.register(info).map_err(|e| e.to_string())?;
        let browse = daemon.browse(SERVICE_TYPE).map_err(|e| e.to_string())?;
        Ok(Self {
            daemon,
            browse,
            code: code.to_owned(),
            peer,
            fullname,
            found: HashMap::new(),
        })
    }

    /// Peers of the session resolved since the last call (other than this one).
    pub fn poll(&mut self) -> Vec<Found> {
        let mut out = Vec::new();
        while let Ok(event) = self.browse.try_recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    if info.get_property_val_str("code") != Some(self.code.as_str()) {
                        continue;
                    }
                    let Some(peer) = info
                        .get_property_val_str("peer")
                        .and_then(|p| p.parse::<u64>().ok())
                    else {
                        continue;
                    };
                    if peer == self.peer {
                        continue;
                    }
                    let port = info.get_port();
                    let mut addrs: Vec<SocketAddr> = info
                        .get_addresses_v4()
                        .into_iter()
                        .map(|ip| SocketAddr::new(IpAddr::V4(ip), port))
                        .collect();
                    addrs.sort();
                    let found = Found { peer, addrs };
                    self.found.insert(info.get_fullname().to_owned(), found.clone());
                    out.push(found);
                }
                ServiceEvent::ServiceRemoved(_, name) => {
                    self.found.remove(&name);
                }
                _ => {}
            }
        }
        out
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}
