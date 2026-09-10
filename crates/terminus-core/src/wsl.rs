//! WSL distro discovery and helpers (#82).
//! Parsing is platform-agnostic (unit-tested). Spawning `wsl.exe` is Windows-only.

use crate::error::Result;
#[cfg(windows)]
use crate::error::Error;
use serde::{Deserialize, Serialize};

/// Synthetic session/host id prefix — never a SQLite host UUID.
pub const WSL_HOST_PREFIX: &str = "wsl:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WslDistro {
    pub name: String,
    pub state: String,
    pub version: u8,
    pub is_default: bool,
}

pub fn wsl_host_id(distro: &str) -> String {
    format!("{WSL_HOST_PREFIX}{distro}")
}

pub fn distro_from_host_id(host_id: &str) -> Option<&str> {
    host_id.strip_prefix(WSL_HOST_PREFIX)
}

pub fn is_wsl_host_id(host_id: &str) -> bool {
    host_id.starts_with(WSL_HOST_PREFIX)
}

/// Parse `wsl.exe -l -v` output (UTF-16 LE bytes or UTF-8 fixtures).
pub fn parse_wsl_list_output(raw: &[u8]) -> Result<Vec<WslDistro>> {
    let text = decode_wsl_list_bytes(raw);
    parse_wsl_list_text(&text)
}

fn decode_wsl_list_bytes(raw: &[u8]) -> String {
    if raw.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_le(&raw[2..]);
    }
    // Dense NULs → UTF-16LE without BOM
    let nul_ratio = if raw.is_empty() {
        0.0
    } else {
        raw.iter().filter(|&&b| b == 0).count() as f64 / raw.len() as f64
    };
    if nul_ratio > 0.2 {
        return decode_utf16_le(raw);
    }
    String::from_utf8_lossy(raw).into_owned()
}

fn decode_utf16_le(raw: &[u8]) -> String {
    let mut u16s = Vec::with_capacity(raw.len() / 2);
    let mut i = 0;
    while i + 1 < raw.len() {
        u16s.push(u16::from_le_bytes([raw[i], raw[i + 1]]));
        i += 2;
    }
    String::from_utf16_lossy(&u16s)
}

fn parse_wsl_list_text(text: &str) -> Result<Vec<WslDistro>> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("NAME") && upper.contains("STATE") {
            continue;
        }
        let mut is_default = false;
        let mut rest = line;
        if let Some(stripped) = rest.strip_prefix('*') {
            is_default = true;
            rest = stripped.trim_start();
        }
        let cols: Vec<&str> = rest.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }
        // NAME may be multi-token rarely; prefer last two cols as STATE VERSION when numeric version.
        let (name, state, version) = if cols.len() >= 3 {
            let version_str = cols[cols.len() - 1];
            let state = cols[cols.len() - 2];
            if version_str.chars().all(|c| c.is_ascii_digit()) {
                let name = cols[..cols.len() - 2].join(" ");
                let version = version_str.parse::<u8>().unwrap_or(2);
                (name, state.to_string(), version)
            } else {
                (cols[0].to_string(), cols[1].to_string(), 2)
            }
        } else {
            (cols[0].to_string(), cols[1].to_string(), 2)
        };
        if name.is_empty() {
            continue;
        }
        out.push(WslDistro {
            name,
            state,
            version,
            is_default,
        });
    }
    Ok(out)
}

/// List installed WSL distros. Empty when `wsl.exe` is missing.
#[cfg(windows)]
pub fn list_distros() -> Result<Vec<WslDistro>> {
    use std::process::Command;
    let output = Command::new("wsl.exe")
        .args(["-l", "-v"])
        .output()
        .map_err(|err| Error::msg(format!("WSL not available ({err})")))?;
    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::msg(format!(
            "wsl -l -v failed: {}",
            stderr.trim()
        )));
    }
    parse_wsl_list_output(&output.stdout)
}

#[cfg(not(windows))]
pub fn list_distros() -> Result<Vec<WslDistro>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16_le(s: &str) -> Vec<u8> {
        let mut out = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out
    }

    #[test]
    fn ac1_parse_utf16_default_and_states() {
        let raw = utf16_le(
            "  NAME              STATE           VERSION\r\n\
             * Ubuntu            Running         2\r\n\
               Debian            Stopped         2\r\n\
               Alpine            Running         1\r\n",
        );
        let list = parse_wsl_list_output(&raw).expect("parse");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "Ubuntu");
        assert!(list[0].is_default);
        assert_eq!(list[0].state, "Running");
        assert_eq!(list[0].version, 2);
        assert_eq!(list[1].name, "Debian");
        assert!(!list[1].is_default);
        assert_eq!(list[1].state, "Stopped");
        assert_eq!(list[2].version, 1);
    }

    #[test]
    fn ac1_parse_utf8_fixture() {
        let text = "  NAME    STATE    VERSION\n* Ubuntu-22.04  Running  2\n";
        let list = parse_wsl_list_output(text.as_bytes()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Ubuntu-22.04");
        assert!(list[0].is_default);
    }

    #[test]
    fn host_id_roundtrip() {
        assert_eq!(wsl_host_id("Ubuntu"), "wsl:Ubuntu");
        assert_eq!(distro_from_host_id("wsl:Ubuntu"), Some("Ubuntu"));
        assert!(is_wsl_host_id("wsl:Debian"));
        assert!(!is_wsl_host_id("local"));
        assert!(!is_wsl_host_id("uuid-here"));
    }
}
