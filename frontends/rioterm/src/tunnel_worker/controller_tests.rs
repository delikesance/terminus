use super::controller::*;
use super::stats::local_port_free;
use super::test_support::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminus_ui::views::tunnels::{TunnelAction, TunnelDraft, TunnelKind, TunnelStatus};

fn dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("terminus-tunnels-{tag}-{}", uuid::Uuid::new_v4()))
}

fn draft(name: &str, port: u16) -> TunnelDraft {
    TunnelDraft {
        id: None,
        kind: TunnelKind::Local,
        name: name.into(),
        bind_host: "127.0.0.1".into(),
        bind_port: port,
        dest_host: "localhost".into(),
        dest_port: port,
    }
}

fn pump(
    c: &mut TunnelController,
    mut done: impl FnMut(&TunnelController) -> bool,
) -> bool {
    wait_for(|| {
        c.tick();
        done(c)
    })
}

#[test]
fn the_controller_persists_and_reloads_per_machine() {
    let d = dir("persist");
    let host = uuid::Uuid::new_v4().to_string();
    {
        let mut c = TunnelController::spawn(d.clone(), None);
        c.select_machine(&host, "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        assert_eq!(c.state().items[0].name, "db");
        assert_eq!(c.state().items[0].status, TunnelStatus::Stopped);
    }
    let mut c = TunnelController::spawn(d, None);
    c.select_machine(&host, "prod");
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    c.select_machine(&uuid::Uuid::new_v4().to_string(), "other");
    assert!(pump(&mut c, |c| c.state().items.is_empty()));
}

#[test]
fn a_machine_without_ssh_shows_no_tunnels_and_saves_nothing() {
    let mut c = TunnelController::spawn(dir("nossh"), None);
    let host = uuid::Uuid::new_v4().to_string();
    c.select_machine(&host, "prod");
    c.save(&draft("db", 5432));
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    c.clear_machine("This computer");
    assert_eq!(c.host_id(), None);
    assert!(c.state().items.is_empty());
    assert_eq!(c.state().host_label, "This computer");
    c.save(&draft("ignored", 5433));
    assert!(!pump(&mut c, |c| !c.state().items.is_empty()));
}

#[test]
fn editing_updates_in_place_and_delete_removes() {
    let mut c = TunnelController::spawn(dir("edit"), None);
    let host = uuid::Uuid::new_v4().to_string();
    c.select_machine(&host, "prod");
    c.save(&draft("db", 5432));
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    let id = c.state().items[0].id.clone();
    let mut edited = draft("db2", 5433);
    edited.id = Some(id.clone());
    c.save(&edited);
    assert!(pump(&mut c, |c| c
        .state()
        .items
        .first()
        .is_some_and(|t| t.name == "db2")));
    assert_eq!(c.state().items.len(), 1);
    c.delete(&id);
    assert!(pump(&mut c, |c| c.state().items.is_empty()));
}

#[test]
fn toggle_starts_runs_and_stops_a_tunnel_with_a_fake_command() {
    let mut c =
        TunnelController::spawn(dir("run"), None).with_settle(Duration::from_millis(30));
    let host = uuid::Uuid::new_v4().to_string();
    c.select_machine(&host, "prod");
    c.save(&draft("db", 5432));
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    let id = c.state().items[0].id.clone();
    c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
    assert!(c.state().items[0].status.is_active());
    assert!(pump(&mut c, |c| c.state().running_count() == 1));
    assert_eq!(c.running_count_for(&host), 1);
    c.toggle(&id, &mut |_| unreachable!("stopping must not spawn"));
    assert_eq!(c.state().items[0].status, TunnelStatus::Stopped);
    assert_eq!(c.state().running_count(), 0);
}

#[test]
fn a_dying_tunnel_becomes_failed_with_a_notice() {
    let mut c = TunnelController::spawn(dir("fail"), None)
        .with_settle(Duration::from_millis(500));
    let host = uuid::Uuid::new_v4().to_string();
    c.select_machine(&host, "prod");
    c.save(&draft("db", 5432));
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    let id = c.state().items[0].id.clone();
    c.toggle(&id, &mut |_| {
        Ok(sh("echo 'Permission denied (publickey).' >&2; exit 255"))
    });
    assert!(pump(&mut c, |c| c.state().items[0].status
        == TunnelStatus::Failed));
    assert!(c.state().items[0]
        .error
        .as_deref()
        .unwrap()
        .contains("Authentication"));
    let notices = c.take_notices();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains("db"), "{notices:?}");
    assert!(c.take_notices().is_empty());
    // Failed tunnels can be started again.
    c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
    assert!(c.state().items[0].status.is_active());
}

#[test]
fn a_spawn_error_is_a_notice_and_a_failed_card() {
    let mut c = TunnelController::spawn(dir("spawnerr"), None);
    let host = uuid::Uuid::new_v4().to_string();
    c.select_machine(&host, "prod");
    c.save(&draft("db", 5432));
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    let id = c.state().items[0].id.clone();
    c.toggle(&id, &mut |_| Err("No saved password".into()));
    assert_eq!(c.state().items[0].status, TunnelStatus::Failed);
    assert_eq!(c.take_notices().len(), 1);
}

#[test]
fn host_deletion_and_shutdown_stop_processes() {
    let mut c =
        TunnelController::spawn(dir("kill"), None).with_settle(Duration::from_millis(10));
    let host = uuid::Uuid::new_v4().to_string();
    c.select_machine(&host, "prod");
    c.save(&draft("db", 5432));
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    let id = c.state().items[0].id.clone();
    c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
    assert!(pump(&mut c, |c| c.running_count_for(&host) == 1));
    c.host_deleted(&host);
    assert_eq!(c.running_count_for(&host), 0);
    c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
    c.shutdown();
    assert_eq!(c.running_count_for(&host), 0);
}

#[test]
fn a_new_tunnel_saved_from_the_form_starts_once_listed() {
    let mut c = TunnelController::spawn(dir("pending"), None)
        .with_settle(Duration::from_millis(10));
    let host = uuid::Uuid::new_v4().to_string();
    c.select_machine(&host, "prod");
    c.run(
        TunnelAction::Save(draft("db", 5432)),
        &mut |_| unreachable!(),
    );
    let mut started = false;
    assert!(pump(&mut c, |c| {
        let _ = c;
        true
    }));
    assert!(wait_for(|| {
        c.tick();
        started = started || c.start_pending(&mut |_| Ok(sh("sleep 30")));
        started
    }));
    assert!(c.state().items[0].status.is_active());
}

#[test]
fn the_free_port_probe_sees_a_bound_port() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    assert!(!local_port_free(port));
    drop(l);
    assert!(local_port_free(port));
}

/// A controller with two saved tunnels ("db" and "web") on one machine.
fn two_tunnels(tag: &str) -> (TunnelController, String, String) {
    let mut c =
        TunnelController::spawn(dir(tag), None).with_settle(Duration::from_millis(10));
    c.select_machine(&uuid::Uuid::new_v4().to_string(), "prod");
    c.save(&draft("db", 5432));
    c.save(&draft("web", 8080));
    assert!(pump(&mut c, |c| c.state().items.len() == 2));
    let id = |name: &str| {
        c.state()
            .items
            .iter()
            .find(|t| t.name == name)
            .unwrap()
            .id
            .clone()
    };
    let (db, web) = (id("db"), id("web"));
    (c, db, web)
}

#[test]
fn a_running_tunnel_carries_uptime_and_a_connection_sample() {
    let (mut c, db, web) = two_tunnels("stats");
    assert!(c.state().items.iter().all(|t| t.stats.is_none()));
    c.start_tunnel("db", &mut |_| Ok(sh("sleep 30"))).unwrap();
    assert!(pump(&mut c, |c| c
        .state()
        .items
        .iter()
        .any(|t| t.stats.is_some())));
    let by = |c: &TunnelController, id: &str| {
        c.state().items.iter().find(|t| t.id == id).unwrap().clone()
    };
    let stats = by(&c, &db).stats.unwrap();
    assert!(stats.uptime_secs < 5);
    // `sh` owns no socket: 0 on Linux, unknown elsewhere.
    let expected = if cfg!(target_os = "linux") {
        Some(0)
    } else {
        None
    };
    assert_eq!(stats.connections, expected);
    assert!(by(&c, &web).stats.is_none(), "stopped tunnels have none");
    c.stop_tunnel("db").unwrap();
    assert!(by(&c, &db).stats.is_none(), "stats end with the process");
}

#[test]
fn start_and_stop_resolve_ids_and_names() {
    let (mut c, db, _web) = two_tunnels("api");
    c.start_tunnel(&db, &mut |_| Ok(sh("sleep 30"))).unwrap();
    assert!(c.registry.is_active(&db));
    let pid = c.registry.pid(&db);
    // Starting a running tunnel keeps the same process.
    c.start_tunnel("DB", &mut |_| unreachable!("already running"))
        .unwrap();
    assert_eq!(c.registry.pid(&db), pid);
    c.stop_tunnel("db").unwrap();
    assert!(!c.registry.is_active(&db));
    assert_eq!(c.state().items[0].status, TunnelStatus::Stopped);
    // Stopping a stopped tunnel is fine; unknown keys are not.
    c.stop_tunnel(&db).unwrap();
    assert!(c.stop_tunnel("nope").is_err());
    assert!(c.start_tunnel("nope", &mut |_| unreachable!()).is_err());
}

#[test]
fn a_failed_start_reports_the_error_and_marks_the_card() {
    let (mut c, db, _web) = two_tunnels("fail");
    let err = c
        .start_tunnel("db", &mut |_| Err("no ssh".to_string()))
        .unwrap_err();
    assert_eq!(err, "no ssh");
    let t = c.state().items.iter().find(|t| t.id == db).unwrap();
    assert_eq!(t.status, TunnelStatus::Failed);
    assert!(c.take_notices().is_empty(), "the caller shows the Err");
}

#[test]
fn an_ambiguous_name_is_refused() {
    let mut c = TunnelController::spawn(dir("ambig"), None)
        .with_settle(Duration::from_millis(10));
    c.select_machine(&uuid::Uuid::new_v4().to_string(), "prod");
    c.save(&draft("db", 5432));
    c.save(&draft("db", 5433));
    assert!(pump(&mut c, |c| c.state().items.len() == 2));
    let err = c.start_tunnel("db", &mut |_| unreachable!()).unwrap_err();
    assert!(err.contains("More than one"), "{err}");
    let id = c.state().items[0].id.clone();
    assert_eq!(c.resolve_current(&id).as_deref(), Ok(id.as_str()));
}

#[test]
fn an_active_tunnel_wakes_the_ui_about_once_a_second() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let wakes = Arc::new(AtomicUsize::new(0));
    let w = wakes.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    let mut c = TunnelController::spawn(dir("wake"), Some(wake))
        .with_settle(Duration::from_millis(10));
    c.select_machine(&uuid::Uuid::new_v4().to_string(), "prod");
    c.save(&draft("db", 5432));
    assert!(pump(&mut c, |c| c.state().items.len() == 1));
    std::thread::sleep(Duration::from_millis(100));
    let before = wakes.load(Ordering::SeqCst);
    c.start_tunnel("db", &mut |_| Ok(sh("sleep 30"))).unwrap();
    c.tick();
    let end = Instant::now() + Duration::from_millis(2500);
    while Instant::now() < end && wakes.load(Ordering::SeqCst) == before {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        wakes.load(Ordering::SeqCst) > before,
        "no tick while running"
    );
    // Idle again once nothing runs: the ticker stops waking.
    c.stop_tunnel("db").unwrap();
    c.tick();
    std::thread::sleep(Duration::from_millis(1300));
    let settled = wakes.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(1300));
    assert_eq!(wakes.load(Ordering::SeqCst), settled, "woke with no tunnel");
}

#[test]
fn stopping_a_name_shared_by_idle_tunnels_of_two_machines_is_a_no_op() {
    let mut c = TunnelController::spawn(dir("shared"), None)
        .with_settle(Duration::from_millis(10));
    for label in ["a", "b"] {
        c.select_machine(&uuid::Uuid::new_v4().to_string(), label);
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
    }
    assert_eq!(c.stop_tunnel("db"), Ok(()));
}
