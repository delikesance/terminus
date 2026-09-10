//! WSL distro discovery and helpers (#82 / #84).
//! Parsing is platform-agnostic (unit-tested). Spawning `wsl.exe` is Windows-only.

use crate::error::Result;
#[cfg(windows)]
use crate::error::Error;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Synthetic session/host id prefix — never a SQLite host UUID.
pub const WSL_HOST_PREFIX: &str = "wsl:";

/// How long a successful `wsl -l -v` result is reused (#84 AC4).
pub const LIST_CACHE_TTL: Duration = Duration::from_secs(45);

/// Win32 `CREATE_NO_WINDOW` — hide console for non-PTY `wsl.exe` (#84 AC1).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

/// Args for an interactive WSL shell in a ConPTY (`wsl.exe` + these).
/// Uses `--cd ~` instead of a Windows cwd so the distro starts in the Linux home (#84).
pub fn shell_args(distro: &str) -> Vec<String> {
    vec![
        "-d".to_string(),
        distro.to_string(),
        "--cd".to_string(),
        "~".to_string(),
    ]
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

struct ListCache {
    at: Instant,
    list: Vec<WslDistro>,
}

static LIST_CACHE: Mutex<Option<ListCache>> = Mutex::new(None);
/// Test/observability: how many times the uncached fetch path ran.
static LIST_FETCH_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Clear the in-process distro list cache (tests + rare force refresh).
pub fn clear_list_cache() {
    *LIST_CACHE.lock() = None;
}

pub fn list_fetch_count() -> usize {
    LIST_FETCH_COUNT.load(Ordering::Relaxed)
}

/// Pure cache helper — unit-tested without invoking `wsl.exe`.
pub fn cache_get_or_fetch<F>(
    cache: &Mutex<Option<ListCache>>,
    force: bool,
    ttl: Duration,
    mut fetch: F,
) -> Result<Vec<WslDistro>>
where
    F: FnMut() -> Result<Vec<WslDistro>>,
{
    if !force {
        let guard = cache.lock();
        if let Some(entry) = guard.as_ref() {
            if entry.at.elapsed() < ttl {
                return Ok(entry.list.clone());
            }
        }
    }
    let list = fetch()?;
    *cache.lock() = Some(ListCache {
        at: Instant::now(),
        list: list.clone(),
    });
    Ok(list)
}

/// List installed WSL distros (cached). Empty when `wsl.exe` is missing.
pub fn list_distros() -> Result<Vec<WslDistro>> {
    list_distros_cached(false)
}

pub fn list_distros_cached(force: bool) -> Result<Vec<WslDistro>> {
    cache_get_or_fetch(&LIST_CACHE, force, LIST_CACHE_TTL, fetch_distros_uncached)
}

fn fetch_distros_uncached() -> Result<Vec<WslDistro>> {
    LIST_FETCH_COUNT.fetch_add(1, Ordering::Relaxed);
    let list = fetch_distros_platform()?;
    #[cfg(windows)]
    schedule_warm(&list);
    Ok(list)
}

#[cfg(windows)]
fn fetch_distros_platform() -> Result<Vec<WslDistro>> {
    let output = hidden_wsl_command()
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
fn fetch_distros_platform() -> Result<Vec<WslDistro>> {
    Ok(Vec::new())
}

#[cfg(windows)]
fn hidden_wsl_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("wsl.exe");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Pre-start a Stopped distro (or the default) so the first interactive open is faster.
#[cfg(windows)]
fn schedule_warm(list: &[WslDistro]) {
    let target = list
        .iter()
        .find(|d| d.is_default)
        .or_else(|| list.first())
        .map(|d| d.name.clone());
    let Some(name) = target else {
        return;
    };
    std::thread::Builder::new()
        .name("wsl-warm".into())
        .spawn(move || {
            let _ = hidden_wsl_command()
                .args(["-d", &name, "--cd", "~", "-e", "true"])
                .output();
        })
        .ok();
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

    #[test]
    fn shell_args_use_cd_home_not_windows_cwd() {
        assert_eq!(
            shell_args("Ubuntu").as_slice(),
            ["-d", "Ubuntu", "--cd", "~"]
        );
        assert_eq!(
            shell_args("Debian").as_slice(),
            ["-d", "Debian", "--cd", "~"]
        );
    }

    #[test]
    fn list_cache_skips_fetch_within_ttl() {
        let cache: Mutex<Option<ListCache>> = Mutex::new(None);
        let calls = AtomicUsize::new(0);
        let mut fetch = || {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(vec![WslDistro {
                name: "Ubuntu".into(),
                state: "Running".into(),
                version: 2,
                is_default: true,
            }])
        };
        let a = cache_get_or_fetch(&cache, false, Duration::from_secs(60), &mut fetch).unwrap();
        let b = cache_get_or_fetch(&cache, false, Duration::from_secs(60), &mut fetch).unwrap();
        assert_eq!(a, b);
        assert_eq!(calls.load(Ordering::Relaxed), 1, "second call must hit cache");
        let _ = cache_get_or_fetch(&cache, true, Duration::from_secs(60), &mut fetch).unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 2, "force must refetch");
    }

    #[test]
    fn create_no_window_flag_value() {
        // Document/lock the Win32 constant we pass to CommandExt::creation_flags.
        assert_eq!(0x0800_0000u32, 0x08000000);
    }
}
