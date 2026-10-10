#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelKind {
    Local,
    Remote,
    Dynamic,
}

impl TunnelKind {
    pub const ALL: [TunnelKind; 3] =
        [TunnelKind::Local, TunnelKind::Remote, TunnelKind::Dynamic];

    pub fn label(self) -> &'static str {
        match self {
            TunnelKind::Local => "Local",
            TunnelKind::Remote => "Remote",
            TunnelKind::Dynamic => "Dynamic",
        }
    }

    /// Value stored in `PortForward::kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            TunnelKind::Local => "local",
            TunnelKind::Remote => "remote",
            TunnelKind::Dynamic => "dynamic",
        }
    }

    /// Unknown stored values read as `Local`.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "remote" => TunnelKind::Remote,
            "dynamic" => TunnelKind::Dynamic,
            _ => TunnelKind::Local,
        }
    }

    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelStatus {
    Stopped,
    /// Process spawned, not yet known to be healthy.
    Starting,
    Running,
    /// Exited on its own; see [`TunnelItem::error`].
    Failed,
}

impl TunnelStatus {
    /// A process exists (Stop is offered instead of Start).
    pub fn is_active(self) -> bool {
        matches!(self, TunnelStatus::Starting | TunnelStatus::Running)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TunnelItem {
    pub id: String,
    pub name: String,
    pub kind: TunnelKind,
    pub bind_host: String,
    /// Listening port: local for Local/Dynamic, remote for Remote.
    pub bind_port: u16,
    pub dest_host: String,
    pub dest_port: u16,
    pub status: TunnelStatus,
    pub error: Option<String>,
    /// Live numbers of the running process; `None` unless the tunnel is
    /// active (the backend fills it on every tick).
    pub stats: Option<TunnelStats>,
}

impl TunnelItem {
    /// Mono route line of the card.
    pub fn route(&self) -> String {
        route_text(self.kind, self.bind_port, &self.dest_host, self.dest_port)
    }

    /// Mono detail line of the card: the failure, or the route followed by
    /// the live stats of an active tunnel.
    pub fn detail(&self) -> String {
        match (self.status, &self.error, self.stats) {
            (TunnelStatus::Failed, Some(err), _) => format!("Failed \u{2014} {err}"),
            (st, _, Some(stats)) if st.is_active() => {
                format!("{}  \u{b7}  {}", self.route(), stats.summary())
            }
            _ => self.route(),
        }
    }
}

/// What can honestly be observed about an `ssh -N` process.
///
/// `connections` is a sample of the established forwarded connections and is
/// `None` where the platform (or the forward kind) cannot provide it. Bytes
/// are not tracked: nothing cheap and truthful exposes them for a plain
/// `ssh -N`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TunnelStats {
    pub uptime_secs: u64,
    pub connections: Option<u32>,
}

impl TunnelStats {
    /// `up 3m 12s \u{b7} 2 connections`, `up 42s \u{b7} connections n/a`.
    pub fn summary(&self) -> String {
        let conns = match self.connections {
            Some(1) => "1 connection".to_string(),
            Some(n) => format!("{n} connections"),
            None => "connections n/a".to_string(),
        };
        format!("up {} \u{b7} {conns}", format_uptime(self.uptime_secs))
    }
}

/// `42s`, `3m 12s`, `1h 05m`, `2d 3h`.
pub fn format_uptime(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m {:02}s", secs / 60, secs % 60),
        3600..=86_399 => format!("{}h {:02}m", secs / 3600, secs % 3600 / 60),
        _ => format!("{}d {}h", secs / 86_400, secs % 86_400 / 3600),
    }
}

pub(super) fn route_text(
    kind: TunnelKind,
    bind: u16,
    dest_host: &str,
    dest_port: u16,
) -> String {
    match kind {
        TunnelKind::Local => format!("localhost:{bind} \u{2192} {dest_host}:{dest_port}"),
        TunnelKind::Remote => format!("remote:{bind} \u{2192} {dest_host}:{dest_port}"),
        TunnelKind::Dynamic => format!("localhost:{bind} \u{2192} SOCKS5"),
    }
}

/// Number of running tunnels: the header badge of the Tunnels tab.
pub fn running_count(items: &[TunnelItem]) -> usize {
    items
        .iter()
        .filter(|t| t.status == TunnelStatus::Running)
        .count()
}

/// Parse a port field: 1-65535.
pub fn parse_port(s: &str) -> Result<u16, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Enter a port".into());
    }
    match s.parse::<u32>() {
        Ok(p) if (1..=65535).contains(&p) => Ok(p as u16),
        Ok(_) => Err("Port must be 1\u{2013}65535".into()),
        Err(_) => Err("Port must be a number".into()),
    }
}

pub(super) fn valid_host(s: &str) -> bool {
    let name = match s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        Some(literal) => {
            return !literal.is_empty() && literal.parse::<std::net::Ipv6Addr>().is_ok()
        }
        None => s,
    };
    !name.is_empty()
        && name.len() <= 253
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}
