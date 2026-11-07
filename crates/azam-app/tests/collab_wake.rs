//! A shared session wakes the window only when a collaborator's change (or presence) arrives:
//! an idle shared window builds no frames.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use azam_app::AzamApp;
use azam_sync::{Collab, Config, JoinTarget, Presence};
use blenderui::prelude::*;

fn options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1200.0), px(800.0)),
        })),
        ..Default::default()
    }
}

fn local(name: &str) -> Config {
    Config {
        presence: Presence::named(name, 3),
        listen: Some(SocketAddr::from(([127, 0, 0, 1], 0))),
        discovery: false,
        wake: None,
    }
}

/// Runs queued UI work (what the native loop does when woken) for `time`.
fn run_for(cx: &mut App, time: Duration) {
    let start = Instant::now();
    while start.elapsed() < time {
        cx.run_pending();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn remote_updates_wake_an_idle_window() {
    Application::headless().run(|cx| {
        let win = cx
            .open_window(options(), |_, cx| cx.new(AzamApp::new))
            .expect("window");
        for _ in 0..3 {
            cx.draw(win.id);
        }
        let addr = win
            .root()
            .update(cx, |app, cx| app.host_session(local("host"), cx))
            .expect("hosts")
            .expect("listens");
        let mut guest = Collab::join(JoinTarget::Addr(addr), local("guest")).expect("joins");
        let mut project = None;
        let start = Instant::now();
        while project.is_none() && start.elapsed() < Duration::from_secs(10) {
            cx.run_pending();
            project = guest.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut project = project.expect("the guest receives the project");

        // Settle (the guest's arrival and presence wake the window), then stay idle.
        let start = Instant::now();
        let mut last = win.root().read(cx).frame_count();
        let mut quiet_since = Instant::now();
        while quiet_since.elapsed() < Duration::from_millis(400) && start.elapsed() < Duration::from_secs(20)
        {
            run_for(cx, Duration::from_millis(20));
            let now = win.root().read(cx).frame_count();
            if now != last {
                last = now;
                quiet_since = Instant::now();
            }
        }
        let idle = win.root().read(cx).frame_count();
        run_for(cx, Duration::from_millis(500));
        assert_eq!(
            win.root().read(cx).frame_count(),
            idle,
            "an idle shared window builds no frames"
        );

        project.meta.name = "Renamed by the guest".into();
        guest.local_edit(&mut project).expect("the rename is sent");
        let start = Instant::now();
        while win.root().read(cx).session().project.meta.name != project.meta.name
            && start.elapsed() < Duration::from_secs(5)
        {
            run_for(cx, Duration::from_millis(10));
        }
        let app = win.root().read(cx);
        assert_eq!(
            app.session().project.meta.name,
            project.meta.name,
            "the wake brought the edit in"
        );
        assert!(app.frame_count() > idle, "the update triggered a frame");
    });
}
