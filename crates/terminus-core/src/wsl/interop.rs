use std::path::Path;
use std::sync::OnceLock;

use super::*;

/// A `wsl.exe` invocation for a probe.
///
/// Every probe goes through this. On Windows a console child started from a
/// GUI process is given its own console window for as long as it runs, and
/// `wsl.exe` cold-starts slowly enough for that window to be visible — the
/// distro list would come up with a black rectangle flashing over the
/// terminal. There is nothing to hide on the WSL side, so the flag is
/// Windows-only.
pub(super) fn wsl_probe(exe: &Path) -> std::process::Command {
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut command = std::process::Command::new(exe);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        /// `CREATE_NO_WINDOW`, from `Win32_System_Threading`.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

/// Runs `wsl.exe --list --verbose` and parses it, or `None` when interop is
/// unavailable or `wsl.exe` refused.
pub(super) fn interop_list(exe: &Path) -> Option<Vec<WslDistro>> {
    let output = wsl_probe(exe).args(["--list", "--verbose"]).output().ok()?;

    let text = decode_windows_output(&output.stdout);

    if !output.status.success() {
        // WSL 1-era builds reject `--verbose` but know `--list`.
        let plain = wsl_probe(exe).arg("--list").output().ok()?;
        return Some(parse_interop_list(&decode_windows_output(&plain.stdout)));
    }

    let distros = parse_interop_list(&text);

    if distros.is_empty() {
        return None;
    }

    Some(distros)
}

/// Checks that a Windows binary can actually be launched from here.
///
/// The failure this exists for is `Exec format error`: the kernel hands
/// Windows binaries to `/init` through `binfmt_misc`, so anything that replaces
/// `/init` — a container, a sandbox, an FHS wrapper — breaks interop for every
/// process inside it. The error chain is reported verbatim, because "wsl.exe is
/// missing" and "WSL interop is off" need different fixes.
pub fn probe_interop(exe: &Path) -> Result<(), String> {
    match wsl_probe(exe).arg("--status").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = decode_windows_output(&output.stderr);
            let stderr = stderr.trim();

            Err(if stderr.is_empty() {
                format!("{} exited with {}", exe.display(), output.status)
            } else {
                format!("{} failed: {stderr}", exe.display())
            })
        }
        Err(error) => Err(format!(
            "could not run {}: {error}. {}",
            exe.display(),
            interop_hint()
        )),
    }
}

/// [`probe_interop`], asked once per process.
///
/// `wsl.exe --status` cold-starts the WSL service, which is slow enough to be
/// felt as a pause when opening a distro — and it was paid on *every* session
/// open, because the answer is only about `/init` and `wsl.exe`, neither of
/// which changes while the app runs. The first answer is kept; later opens
/// return it without spawning anything.
pub fn interop_ready(exe: &Path) -> Result<(), String> {
    static PROBE: OnceLock<Result<(), String>> = OnceLock::new();

    PROBE.get_or_init(|| probe_interop(exe)).clone()
}

/// Why interop is off, as far as `/init` can say.
///
/// `binfmt_misc` runs `/init` for every Windows binary, so its identity decides
/// whether interop works at all. A `/init` that is a text file is a stand-in
/// for the real one, and the message says which stand-in it is.
pub fn interop_hint() -> String {
    const DEFAULT: &str = "WSL interop is unavailable, so Windows binaries cannot be started from this session.";

    match impostor_init() {
        Some(interpreter) => format!(
            "{DEFAULT} /init is a shell script ({interpreter}), not the WSL interop binary: something replaced it, which is what a sandboxed or containerised session looks like."
        ),
        None if std::fs::read("/init").is_err() => {
            format!("{DEFAULT} /init is not readable from here.")
        }
        None => DEFAULT.to_string(),
    }
}

/// The same reason in one line, for a panel that has one line to give it.
pub fn interop_short_hint() -> String {
    match impostor_init() {
        Some(_) => "/init is a shell script, not the WSL interop binary".to_string(),
        None => "Windows binaries cannot be started from this session".to_string(),
    }
}

/// The interpreter line of the script standing in for `/init`, if one is.
pub(super) fn impostor_init() -> Option<String> {
    let bytes = std::fs::read("/init").ok()?;
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
    let shebang = head.strip_prefix("#!")?;

    Some(
        shebang
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string(),
    )
}

/// Decodes `wsl.exe` output, which is UTF-16LE on Windows and plain UTF-8 when
/// it has been through a pipe on this side.
pub fn decode_windows_output(bytes: &[u8]) -> String {
    // Real UTF-16 text is full of NUL bytes where UTF-8 never has any.
    if bytes.iter().skip(1).step_by(2).any(|byte| *byte == 0) {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();

        return String::from_utf16_lossy(&units);
    }

    String::from_utf8_lossy(bytes).to_string()
}

/// Parses `wsl.exe --list --verbose` (or the bare `--list`).
///
/// Both shapes arrive with a blank first line and, in the verbose one, a
/// `NAME STATE VERSION` header and a `*` marking the default distro. Names may
/// contain spaces, so columns are split on two or more spaces rather than on
/// whitespace.
pub fn parse_interop_list(text: &str) -> Vec<WslDistro> {
    let mut distros: Vec<WslDistro> = Vec::new();

    for line in text.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            continue;
        }

        let is_default = trimmed.starts_with('*');
        let body = trimmed.trim_start_matches(['*', ' ', '\t']);

        let columns: Vec<&str> = body
            .split("  ")
            .map(str::trim)
            .filter(|column| !column.is_empty())
            .collect();

        let Some(name) = columns.first() else {
            continue;
        };

        if name
            .split_whitespace()
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("NAME"))
        {
            // The header row, whether or not its columns are padded.
            continue;
        }

        // `NAME STATE VERSION`: the version column is numeric and the state is
        // one word, neither of which a distro name can be mistaken for.
        let running = match (columns.get(1), columns.get(2)) {
            (Some(state), Some(version)) if is_version(version) => {
                Some(state.eq_ignore_ascii_case("Running"))
            }
            _ => None,
        };

        let name = name.trim();

        if name.is_empty() || distros.iter().any(|distro| distro.name == name) {
            continue;
        }

        distros.push(WslDistro {
            name: name.to_string(),
            display: name.to_string(),
            source: Source::Interop,
            running,
            is_default,
        });
    }

    distros
}

pub(super) fn is_version(column: &str) -> bool {
    !column.is_empty() && column.chars().all(|character| character.is_ascii_digit())
}
