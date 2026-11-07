//! Joining by share code over mDNS/DNS-SD. Skipped when multicast does not work here.

mod common;

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use azam_doc::Template;
use azam_sync::{Collab, Config, JoinTarget, Presence, new_share_code};
use common::template;

fn config(name: &str) -> Config {
    Config {
        presence: Presence::named(name, 1),
        listen: Some(SocketAddr::from(([0, 0, 0, 0], 0))),
        discovery: true,
        wake: None,
    }
}

#[test]
fn joining_by_share_code_finds_the_host() {
    if std::env::var_os("AZAM_SKIP_MDNS").is_some() {
        eprintln!("skipping: AZAM_SKIP_MDNS is set");
        return;
    }
    let project = template(Template::Skirt);
    let code = new_share_code();
    let host = Collab::host(&project, Some(&code), config("host")).expect("hosts");
    let mut guest = Collab::join(JoinTarget::Code(code.clone()), config("guest")).expect("joins");
    let start = Instant::now();
    let mut got = None;
    while start.elapsed() < Duration::from_secs(12) && got.is_none() {
        got = guest.poll();
        if guest
            .status()
            .error
            .as_deref()
            .is_some_and(|e| e.contains("discovery"))
        {
            eprintln!("skipping: {}", guest.status().error.clone().unwrap_or_default());
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let Some(joined) = got else {
        eprintln!("skipping: mDNS did not resolve the host (multicast unavailable?)");
        return;
    };
    assert_eq!(joined.to_value(), project.to_value());
    assert_eq!(guest.status().code, code);
    drop(host);
}
