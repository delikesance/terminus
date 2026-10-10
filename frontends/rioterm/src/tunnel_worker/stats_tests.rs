use super::stats::*;
use std::collections::HashSet;
use terminus_ui::views::tunnels::TunnelKind;

// ------------------------------------------------------------ stats

const PROC_TCP: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 11111 1 0000000000000000 100 0 0 10 0
   1: 0100007F:1538 0100007F:C350 01 00000000:00000000 00:00000000 00000000  1000        0 22222 1 0000000000000000 20 4 30 10 -1
   2: 0100007F:C350 0100007F:1538 01 00000000:00000000 00:00000000 00000000  1000        0 33333 1 0000000000000000 20 4 30 10 -1
   3: garbage
";

const PROC_TCP6: &str = "\
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:1538 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 44444 1 0000000000000000 100 0 0 10 0
   1: 00000000000000000000000001000000:1538 00000000000000000000000001000000:D431 01 00000000:00000000 00:00000000 00000000  1000        0 55555 1 0000000000000000 20 4 30 10 -1
";

fn owned(inodes: &[u64]) -> HashSet<u64> {
    inodes.iter().copied().collect()
}

#[test]
fn proc_net_tcp_rows_are_parsed_and_junk_is_skipped() {
    let rows = parse_proc_net_tcp(PROC_TCP);
    assert_eq!(
        rows,
        vec![
            TcpRow {
                local_port: 5432,
                remote_port: 0,
                established: false,
                inode: 11111
            },
            TcpRow {
                local_port: 5432,
                remote_port: 50000,
                established: true,
                inode: 22222
            },
            TcpRow {
                local_port: 50000,
                remote_port: 5432,
                established: true,
                inode: 33333
            },
        ]
    );
    let rows6 = parse_proc_net_tcp(PROC_TCP6);
    assert_eq!(rows6.len(), 2);
    assert_eq!(rows6[1].local_port, 5432);
    assert_eq!(rows6[1].remote_port, 0xD431);
    assert!(rows6[1].established && !rows6[0].established);
    assert!(parse_proc_net_tcp("").is_empty());
}

#[test]
fn local_and_dynamic_count_accepted_connections_only() {
    let rows = parse_proc_net_tcp(PROC_TCP);
    let all = owned(&[11111, 22222, 33333]);
    for kind in [TunnelKind::Local, TunnelKind::Dynamic] {
        // LISTEN is not a connection; the client end (remote port ==
        // bind port) is not ssh's accepted socket.
        assert_eq!(count_connections(&rows, kind, 5432, 0, &all), 1, "{kind:?}");
    }
    assert_eq!(
        count_connections(&rows, TunnelKind::Local, 5432, 0, &owned(&[])),
        0
    );
    assert_eq!(
        count_connections(&rows, TunnelKind::Local, 9999, 0, &all),
        0
    );
}

#[test]
fn remote_counts_connections_opened_to_the_destination() {
    let rows = parse_proc_net_tcp(PROC_TCP);
    assert_eq!(
        count_connections(&rows, TunnelKind::Remote, 7000, 5432, &owned(&[33333])),
        1
    );
    // Sockets of other processes are not ours.
    assert_eq!(
        count_connections(&rows, TunnelKind::Remote, 7000, 5432, &owned(&[22222])),
        0
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_live_accepted_connection_is_sampled_from_proc() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let me = std::process::id();
    assert_eq!(sample_connections(me, TunnelKind::Local, port, 0), Some(0));
    let _client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (_server, _) = l.accept().unwrap();
    assert_eq!(sample_connections(me, TunnelKind::Local, port, 0), Some(1));
    assert_eq!(
        sample_connections(0x7fff_fff0, TunnelKind::Local, port, 0),
        None
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn connections_are_unavailable_off_linux() {
    assert_eq!(sample_connections(1, TunnelKind::Local, 1, 0), None);
}
