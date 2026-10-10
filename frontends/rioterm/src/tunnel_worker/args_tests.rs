use super::args::*;
use terminus_ui::views::tunnels::{TunnelItem, TunnelKind, TunnelStatus};

fn item(kind: TunnelKind) -> TunnelItem {
    TunnelItem {
        id: "t1".into(),
        name: "db".into(),
        kind,
        bind_host: "127.0.0.1".into(),
        bind_port: 5432,
        dest_host: "db.internal".into(),
        dest_port: 5433,
        status: TunnelStatus::Stopped,
        error: None,
        stats: None,
    }
}

fn base() -> Vec<String> {
    [
        "-p",
        "2222",
        "-i",
        "/k",
        "-o",
        "IdentitiesOnly=yes",
        "--",
        "me@example.com",
    ]
    .map(String::from)
    .to_vec()
}

#[test]
fn forward_flags_per_kind() {
    assert_eq!(
        forward_args(&item(TunnelKind::Local)),
        ["-L", "127.0.0.1:5432:db.internal:5433"]
    );
    assert_eq!(
        forward_args(&item(TunnelKind::Remote)),
        ["-R", "127.0.0.1:5432:db.internal:5433"]
    );
    assert_eq!(
        forward_args(&item(TunnelKind::Dynamic)),
        ["-D", "127.0.0.1:5432"]
    );
}

#[test]
fn the_command_is_a_non_interactive_forward_only_ssh() {
    let args = tunnel_ssh_args(&base(), false, &item(TunnelKind::Local));
    assert_eq!(
        args.last().unwrap(),
        "me@example.com",
        "destination stays last"
    );
    assert_eq!(args[args.len() - 2], "--", "options end before the host");
    assert!(args.contains(&"-N".to_string()));
    let joined = args.join(" ");
    assert!(joined.contains("ExitOnForwardFailure=yes"));
    assert!(
        joined.contains("BatchMode=yes"),
        "never prompt without askpass"
    );
    assert!(joined.contains("-L 127.0.0.1:5432:db.internal:5433"));
    // The host's own options are preserved verbatim.
    assert!(joined.contains("-p 2222 -i /k -o IdentitiesOnly=yes"));
}

#[test]
fn askpass_hosts_do_not_get_batch_mode() {
    let args = tunnel_ssh_args(&base(), true, &item(TunnelKind::Dynamic));
    assert!(!args.join(" ").contains("BatchMode"));
    assert!(args.join(" ").contains("-D 127.0.0.1:5432"));
}

#[test]
fn ssh_errors_become_short_sentences() {
    assert!(
        friendly_error("bind [127.0.0.1]:80: Address already in use\n", Some(255))
            .contains("already in use")
    );
    assert!(
        friendly_error("me@h: Permission denied (publickey).", Some(255))
            .contains("Authentication")
    );
    assert!(friendly_error(
        "ssh: connect to host h port 22: Connection refused",
        Some(255)
    )
    .contains("Could not reach"));
    assert!(
        friendly_error("ssh: Could not resolve hostname x", Some(255))
            .contains("Could not reach")
    );
    assert!(
        friendly_error("Host key verification failed.", Some(255)).contains("Host key")
    );
    assert!(friendly_error(
        "Warning: remote port forwarding failed for listen port 80",
        Some(255)
    )
    .contains("refused"));
    assert_eq!(friendly_error("some\nodd line  \n", Some(7)), "odd line");
    assert_eq!(friendly_error("", Some(3)), "ssh exited with code 3");
    assert_eq!(friendly_error("", None), "ssh was terminated");
}
