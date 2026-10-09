/// Canonical auth-method wire values, in cycle order.
pub const AUTH_METHODS: [&str; 3] = ["key", "password", "gssapi"];

/// Human label for an auth-method wire value.
pub fn auth_method_label(method: &str) -> &'static str {
    match method {
        "password" => "Password Authentication",
        "gssapi" => "Kerberos (GSSAPI)",
        _ => "SSH Cryptographic Key",
    }
}

/// An address as people paste it: `user@host:port`, `ssh -p 2222
/// user@host`, `ssh://user@host:port` or `[::1]:22`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddressParts {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<String>,
}

pub fn split_address(input: &str) -> AddressParts {
    let mut words = input.split_whitespace().peekable();
    if words.peek() == Some(&"ssh") {
        words.next();
    }
    let mut port = None;
    let mut target = None;
    while let Some(word) = words.next() {
        if word == "-p" {
            port = words.next().map(str::to_string);
        } else if let Some(p) = word.strip_prefix("-p").filter(|p| !p.is_empty()) {
            port = Some(p.to_string());
        } else if target.is_none() && !word.starts_with('-') {
            target = Some(word);
        }
    }
    let mut rest = target.unwrap_or("").trim();
    rest = rest.strip_prefix("ssh://").unwrap_or(rest);
    rest = rest.trim_end_matches('/');
    let mut user = None;
    if let Some((u, h)) = rest.rsplit_once('@') {
        if !u.is_empty() {
            user = Some(u.to_string());
        }
        rest = h;
    }
    let host = if let Some(inner) = rest.strip_prefix('[') {
        // [v6]:port
        match inner.split_once(']') {
            Some((h, tail)) => {
                if let Some(p) = tail.strip_prefix(':').filter(|p| !p.is_empty()) {
                    port = Some(p.to_string());
                }
                h
            }
            None => inner,
        }
    } else {
        match rest.split_once(':') {
            // Exactly one colon: host:port. More is a bare IPv6 address.
            Some((h, p)) if !p.contains(':') && !p.is_empty() => {
                port = Some(p.to_string());
                h
            }
            _ => rest,
        }
    };
    AddressParts {
        host: host.to_string(),
        user,
        port,
    }
}
