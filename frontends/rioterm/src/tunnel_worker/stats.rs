use std::collections::HashSet;
use std::time::Duration;

use terminus_ui::views::tunnels::TunnelKind;

/// True when `127.0.0.1:port` can be bound right now.
pub fn local_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

// ---------------------------------------------------------------- stats

/// One socket row of `/proc/net/tcp{,6}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpRow {
    pub local_port: u16,
    pub remote_port: u16,
    pub established: bool,
    pub inode: u64,
}

/// Parse the text of `/proc/net/tcp` or `/proc/net/tcp6`; malformed lines
/// (and the header) are skipped.
pub fn parse_proc_net_tcp(text: &str) -> Vec<TcpRow> {
    // `sl local rem st tx:rx tr:tm retrnsmt uid timeout inode …`; addresses
    // are `HEXIP:HEXPORT` (8 or 32 hex digits of IP), state `01` is
    // ESTABLISHED.
    fn port(addr: &str) -> Option<u16> {
        u16::from_str_radix(addr.rsplit_once(':')?.1, 16).ok()
    }
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || !f[0].ends_with(':') {
                return None;
            }
            Some(TcpRow {
                local_port: port(f[1])?,
                remote_port: port(f[2])?,
                established: f[3] == "01",
                inode: f[9].parse().ok()?,
            })
        })
        .collect()
}

/// Established forwarded connections of one tunnel among `rows`, counting
/// only sockets owned by its ssh process (`owned` = socket inodes of
/// `/proc/<pid>/fd`):
///
/// * `-L` / `-D`: connections ssh accepted, i.e. local port == `bind_port`.
/// * `-R`: connections ssh opened to the destination, i.e. remote port ==
///   `dest_port`.
pub fn count_connections(
    rows: &[TcpRow],
    kind: TunnelKind,
    bind_port: u16,
    dest_port: u16,
    owned: &HashSet<u64>,
) -> u32 {
    rows.iter()
        .filter(|r| r.established && owned.contains(&r.inode))
        .filter(|r| match kind {
            TunnelKind::Local | TunnelKind::Dynamic => r.local_port == bind_port,
            TunnelKind::Remote => r.remote_port == dest_port,
        })
        .count() as u32
}

/// Sample the established connections of the tunnel run by ssh process
/// `pid`. `None` when the platform cannot tell (anything but Linux) or the
/// process is gone.
pub fn sample_connections(
    pid: u32,
    kind: TunnelKind,
    bind_port: u16,
    dest_port: u16,
) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let owned: HashSet<u64> = std::fs::read_dir(format!("/proc/{pid}/fd"))
            .ok()?
            .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
            .filter_map(|l| {
                let l = l.to_str()?;
                l.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
            })
            .collect();
        let mut rows =
            parse_proc_net_tcp(&std::fs::read_to_string("/proc/net/tcp").ok()?);
        if let Ok(v6) = std::fs::read_to_string("/proc/net/tcp6") {
            rows.extend(parse_proc_net_tcp(&v6));
        }
        Some(count_connections(&rows, kind, bind_port, dest_port, &owned))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, kind, bind_port, dest_port);
        None
    }
}

/// Connections are re-sampled at most this often (`/proc` reads).
pub(super) const SAMPLE_EVERY: Duration = Duration::from_secs(1);
