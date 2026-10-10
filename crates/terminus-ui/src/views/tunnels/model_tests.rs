use super::test_support::item;
use super::*;
#[test]
fn valid_host_rejects_option_like_and_malformed_addresses() {
    for ok in ["db.internal", "localhost", "10.0.0.1", "[::1]", "[fe80::1]"] {
        assert!(valid_host(ok), "{ok}");
    }
    for bad in [
        "-oProxyCommand=x",
        "-h",
        "a:b",
        "[::1",
        "::1]",
        "[]",
        "a[b]",
    ] {
        assert!(!valid_host(bad), "{bad}");
    }
}

#[test]
fn uptime_is_compact_and_grows_a_unit_at_a_time() {
    assert_eq!(format_uptime(0), "0s");
    assert_eq!(format_uptime(42), "42s");
    assert_eq!(format_uptime(60), "1m 00s");
    assert_eq!(format_uptime(192), "3m 12s");
    assert_eq!(format_uptime(3600), "1h 00m");
    assert_eq!(format_uptime(3600 + 5 * 60 + 59), "1h 05m");
    assert_eq!(format_uptime(2 * 86_400 + 3 * 3600 + 120), "2d 3h");
}

#[test]
fn stats_summary_says_n_a_when_connections_are_unknown() {
    let s = |u, c| TunnelStats {
        uptime_secs: u,
        connections: c,
    };
    assert_eq!(s(42, Some(0)).summary(), "up 42s \u{b7} 0 connections");
    assert_eq!(s(192, Some(1)).summary(), "up 3m 12s \u{b7} 1 connection");
    assert_eq!(s(192, Some(2)).summary(), "up 3m 12s \u{b7} 2 connections");
    assert_eq!(s(7, None).summary(), "up 7s \u{b7} connections n/a");
}

#[test]
fn the_detail_line_is_the_route_plus_stats_of_an_active_tunnel() {
    let mut t = item("a", TunnelKind::Local, TunnelStatus::Stopped);
    assert_eq!(t.detail(), t.route());
    t.status = TunnelStatus::Running;
    t.stats = Some(TunnelStats {
        uptime_secs: 5,
        connections: Some(3),
    });
    assert_eq!(
        t.detail(),
        format!("{}  \u{b7}  up 5s \u{b7} 3 connections", t.route())
    );
    // Stale stats never show on a tunnel that is not running.
    t.status = TunnelStatus::Stopped;
    assert_eq!(t.detail(), t.route());
    t.status = TunnelStatus::Failed;
    t.error = Some("boom".into());
    assert_eq!(t.detail(), "Failed \u{2014} boom");
}

#[test]
fn ports_are_1_to_65535() {
    assert_eq!(parse_port("5432"), Ok(5432));
    assert_eq!(parse_port(" 80 "), Ok(80));
    assert!(parse_port("").is_err());
    assert!(parse_port("0").is_err());
    assert!(parse_port("65536").is_err());
    assert!(parse_port("12a").is_err());
    assert_eq!(parse_port("65535"), Ok(65535));
}

#[test]
fn routes_read_left_to_right_per_kind() {
    let mut t = item("a", TunnelKind::Local, TunnelStatus::Stopped);
    t.dest_host = "db.internal".into();
    t.dest_port = 5433;
    assert_eq!(t.route(), "localhost:5432 \u{2192} db.internal:5433");
    t.kind = TunnelKind::Remote;
    assert_eq!(t.route(), "remote:5432 \u{2192} db.internal:5433");
    t.kind = TunnelKind::Dynamic;
    assert_eq!(t.route(), "localhost:5432 \u{2192} SOCKS5");
}

#[test]
fn kind_round_trips_through_its_stored_name() {
    for k in TunnelKind::ALL {
        assert_eq!(TunnelKind::parse(k.as_str()), k);
    }
    assert_eq!(TunnelKind::parse("???"), TunnelKind::Local);
}

#[test]
fn the_badge_counts_running_tunnels_only() {
    let items = vec![
        item("a", TunnelKind::Local, TunnelStatus::Running),
        item("b", TunnelKind::Local, TunnelStatus::Stopped),
        item("c", TunnelKind::Remote, TunnelStatus::Starting),
        item("d", TunnelKind::Local, TunnelStatus::Running),
        item("e", TunnelKind::Local, TunnelStatus::Failed),
    ];
    assert_eq!(running_count(&items), 2);
    assert_eq!(running_count(&[]), 0);
}
