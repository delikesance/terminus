use terminus_ui::views::tunnels::{TunnelItem, TunnelKind};

/// The `-L`/`-R`/`-D` flag pair for a tunnel.
pub fn forward_args(item: &TunnelItem) -> Vec<String> {
    match item.kind {
        TunnelKind::Local => vec![
            "-L".into(),
            format!(
                "{}:{}:{}:{}",
                item.bind_host, item.bind_port, item.dest_host, item.dest_port
            ),
        ],
        TunnelKind::Remote => vec![
            "-R".into(),
            format!(
                "{}:{}:{}:{}",
                item.bind_host, item.bind_port, item.dest_host, item.dest_port
            ),
        ],
        TunnelKind::Dynamic => {
            vec![
                "-D".into(),
                format!("{}:{}", item.bind_host, item.bind_port),
            ]
        }
    }
}

/// Full `ssh` argument list for a tunnel.
///
/// `base` is what the interactive shell would run (options, then the
/// destination as the last element). `has_askpass` says the base command
/// authenticates through `SSH_ASKPASS` (password hosts, passphrase keys): only
/// then may ssh ask anything. In every other case `BatchMode=yes` makes a login
/// that would need a prompt fail immediately with "Permission denied" instead
/// of hanging on a terminal nobody sees.
pub fn tunnel_ssh_args(
    base: &[String],
    has_askpass: bool,
    item: &TunnelItem,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-N".into(),
        "-T".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
        "-o".into(),
        "ServerAliveCountMax=3".into(),
    ];
    if !has_askpass {
        args.extend(["-o".into(), "BatchMode=yes".into()]);
    }
    args.extend(forward_args(item));
    args.extend(base.iter().cloned());
    args
}

/// Turn what ssh printed before dying into one short sentence.
pub fn friendly_error(stderr: &str, code: Option<i32>) -> String {
    let lower = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if has(&[
        "address already in use",
        "cannot listen to port",
        "cannot assign requested address",
    ]) {
        return "Port is already in use".into();
    }
    if has(&["remote port forwarding failed"]) {
        return "The server refused the remote port forward".into();
    }
    if has(&["permission denied", "too many authentication failures"]) {
        return "Authentication failed \u{2014} check the key or the saved password"
            .into();
    }
    if has(&["host key verification failed", "identification has changed"]) {
        return "Host key not trusted \u{2014} connect once from a terminal tab".into();
    }
    if has(&[
        "connection refused",
        "timed out",
        "could not resolve",
        "no route to host",
        "network is unreachable",
        "connection closed by",
    ]) {
        return "Could not reach the server".into();
    }
    if has(&[
        "administratively prohibited",
        "open failed",
        "forwarding is disabled",
    ]) {
        return "The server does not allow this forward".into();
    }
    if let Some(line) = stderr.lines().map(str::trim).rev().find(|l| !l.is_empty()) {
        let mut s: String = line.chars().take(140).collect();
        if line.chars().count() > 140 {
            s.push('\u{2026}');
        }
        return s;
    }
    match code {
        Some(c) => format!("ssh exited with code {c}"),
        None => "ssh was terminated".into(),
    }
}
