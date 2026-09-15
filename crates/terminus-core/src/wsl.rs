//! The WSL distros installed on the Windows machine this one is nested in.
//!
//! Opening one of them needs the **name Windows registered for it**, because
//! that is what `wsl.exe -d <name>` takes, and Windows is the only place that
//! name lives. Three sources are tried, most reliable first:
//!
//! 1. **Interop** — `wsl.exe --list --verbose`, straight from the registry, the
//!    only source that also knows which distro is running and which is default.
//!    Interop is a kernel feature (`binfmt_misc` runs `/init` for Windows
//!    binaries), so it is unavailable whenever `/init` is not the interop
//!    binary: a container, a sandboxed shell, a rootfs that never had WSL in
//!    it. [`probe_interop`] says so in as many words when that happens.
//! 2. **Windows Terminal** — the profiles it generated from those same registry
//!    entries. `source = "Windows.Terminal.Wsl"` carries the registered name,
//!    `Microsoft.WSL` carries it too (store WSL), and the packaged-distro
//!    profiles carry the *display* name from the Store listing.
//! 3. **The distro disks** — `ext4.vhdx` files under `%LOCALAPPDATA%`. These
//!    prove a distro exists and say which package it came from, but not what
//!    Windows calls it; a disk with no name from 1. or 2. is reported as
//!    unnamed rather than guessed at, since guessing would launch the wrong
//!    distro.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A Windows-side installation, as seen from this (Linux) side: the mount
/// point of the `C:` drive plus the user profiles to look under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsRoots {
    pub mount: PathBuf,
    pub profiles: Vec<PathBuf>,
}

impl WindowsRoots {
    /// Builds the roots from an explicit `C:` mount point.
    pub fn from_mount(mount: impl Into<PathBuf>, profiles: Vec<PathBuf>) -> Self {
        Self {
            mount: mount.into(),
            profiles,
        }
    }

    /// Finds the C-drive root and the user profiles under it, or `None` when
    /// there is no Windows drive to look at (the usual case off WSL).
    ///
    /// Where that root appears depends on which side we are on: inside WSL the
    /// drive is *mounted* at `/mnt/c`, while the native Windows build *is* the
    /// host and the drive is `C:\`. Gating everything on `/mnt/c` alone made
    /// the native build report "no distros" even though it can ask `wsl.exe`
    /// directly, so the root is chosen per target.
    pub fn detect() -> Option<Self> {
        #[cfg(target_os = "windows")]
        let drive = Path::new("C:\\");
        #[cfg(not(target_os = "windows"))]
        let drive = Path::new("/mnt/c");

        Self::detect_from_drive(drive)
    }

    /// Builds roots from an explicit `C:` root (mount point on WSL, the drive
    /// itself on a native build).
    fn detect_from_drive(drive: &Path) -> Option<Self> {
        if !drive.is_dir() {
            return None;
        }

        let mut profiles = Vec::new();
        if let Ok(entries) = std::fs::read_dir(drive.join("Users")) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                // Windows keeps service accounts here; they own no distros.
                if path.is_dir() && !matches!(name.as_str(), "public" | "default") {
                    profiles.push(path);
                }
            }
        }

        profiles.sort();
        Some(Self {
            mount: drive.to_path_buf(),
            profiles,
        })
    }

    /// `wsl.exe`, the launcher every distro session goes through. The inbox
    /// copy under `System32` is preferred: it is the one the interop layer is
    /// built around, and the Store copy is only present when Store WSL is.
    pub fn wsl_exe(&self) -> Option<PathBuf> {
        let inbox = self.mount.join("Windows/System32/wsl.exe");

        if inbox.is_file() {
            return Some(inbox);
        }

        self.profiles
            .iter()
            .map(|profile| profile.join("AppData/Local/Microsoft/WindowsApps/wsl.exe"))
            .find(|candidate| candidate.is_file())
    }

    fn local_app_data(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.profiles
            .iter()
            .map(|profile| profile.join("AppData/Local"))
    }
}

/// Where a distro's name came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `wsl.exe` itself: names, state and the default distro.
    Interop,
    /// Windows Terminal's generated profiles.
    WindowsTerminal,
    /// A distro disk with no name source; listed for honesty, not launchable.
    Disk,
}

/// A WSL distro worth putting in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslDistro {
    /// What `wsl.exe -d` takes.
    pub name: String,
    /// What the sidebar shows — the Store listing's name when there is one.
    pub display: String,
    pub source: Source,
    /// `Some` only when interop answered.
    pub running: Option<bool>,
    pub is_default: bool,
}

impl WslDistro {
    /// The argument list for `wsl.exe` that opens an interactive session on
    /// this distro.
    pub fn launch_args(&self) -> Vec<String> {
        vec!["-d".to_string(), self.name.clone()]
    }
}

/// Everything a scan found, including what it could not name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    pub distros: Vec<WslDistro>,
    /// Distro disks with no name source: real installs we must not guess at.
    pub unnamed: usize,
}

/// Discovers the distros, asking interop first and falling back to Windows
/// Terminal and then to the disks.
pub fn discover(roots: &WindowsRoots) -> Discovery {
    let interop = roots
        .wsl_exe()
        .and_then(|exe| interop_list(&exe))
        .unwrap_or_default();
    let profiles = terminal_profiles(roots);
    let disks = distro_disks(roots);

    merge(&interop, &profiles, &disks)
}

/// The non-interop half of [`discover`], for callers that already have interop
/// output (or know they cannot have it).
pub fn discover_without_interop(roots: &WindowsRoots) -> Discovery {
    merge(&[], &terminal_profiles(roots), &distro_disks(roots))
}

/// Everything Windows Terminal knows that `wsl.exe` did not already give us.
fn terminal_profiles(roots: &WindowsRoots) -> TerminalProfiles {
    let mut found = TerminalProfiles::default();

    for profile in roots.local_app_data() {
        let settings = profile.join("Packages");

        let Ok(entries) = std::fs::read_dir(&settings) else {
            continue;
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if !name.starts_with("Microsoft.WindowsTerminal") {
                continue;
            }

            let settings = entry.path().join("LocalState/settings.json");

            if let Ok(json) = std::fs::read_to_string(&settings) {
                found.merge(parse_terminal_profiles(&json));
            }
        }
    }

    found
}

/// Distro disks (`ext4.vhdx`), with the package family that named them.
fn distro_disks(roots: &WindowsRoots) -> Vec<DiskDistro> {
    let mut disks = Vec::new();
    let mut roots_to_scan = Vec::new();

    for local_app_data in roots.local_app_data() {
        // Store distros: one package family each.
        if let Ok(entries) = std::fs::read_dir(local_app_data.join("Packages")) {
            for entry in entries.flatten() {
                let disk = entry.path().join("LocalState/ext4.vhdx");

                if disk.is_file() {
                    roots_to_scan.push(DiskDistro {
                        package: Some(short_package_name(
                            &entry.file_name().to_string_lossy(),
                        )),
                        path: disk,
                    });
                }
            }
        }

        // Distros registered outside the Store live in per-id folders.
        if let Ok(entries) = std::fs::read_dir(local_app_data.join("wsl")) {
            for entry in entries.flatten() {
                let disk = entry.path().join("ext4.vhdx");

                if disk.is_file() {
                    roots_to_scan.push(DiskDistro {
                        package: None,
                        path: disk,
                    });
                }
            }
        }
    }

    disks.extend(roots_to_scan);
    disks.sort_by(|a, b| a.path.cmp(&b.path));
    disks
}

/// A distro disk and, when it came from the Store, the package that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskDistro {
    /// Package family without its version hash, e.g.
    /// `CanonicalGroupLimited.Ubuntu24.04LTS`.
    pub package: Option<String>,
    pub path: PathBuf,
}

/// The names Windows Terminal has for the WSL distros it is configured with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalProfiles {
    /// Distros Windows registered, in `wsl -l` vocabulary.
    names: Vec<ProfileName>,
    /// Names from the Store listings, keyed by package family.
    displays: Vec<ProfileName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileName {
    name: String,
    hidden: bool,
    package: Option<String>,
    is_default: bool,
}

impl TerminalProfiles {
    fn merge(&mut self, other: Self) {
        self.names.extend(other.names);
        self.displays.extend(other.displays);
    }

    /// Only the tests ask: the production path merges first and filters
    /// after, so it never needs to know whether the result is empty.
    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.names.is_empty() && self.displays.is_empty()
    }
}

/// Parses Windows Terminal's `settings.json`.
///
/// Only the profiles that *are* WSL distros survive: `Windows.Terminal.Wsl`
/// (the registry entries, often hidden behind a Store profile of the same
/// distro) and `Microsoft.WSL` (store WSL registers these directly). Everything
/// else under `Windows.Terminal.*` — Azure, PowerShell, Visual Studio — is a
/// different kind of thing wearing the same JSON.
pub fn parse_terminal_profiles(json: &str) -> TerminalProfiles {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(json) else {
        return TerminalProfiles::default();
    };

    let default_profile = document
        .get("defaultProfile")
        .and_then(|value| value.as_str())
        .map(|guid| guid.to_lowercase());

    let Some(profiles) = document
        .get("profiles")
        .and_then(|profiles| profiles.get("list"))
        .and_then(|list| list.as_array())
    else {
        return TerminalProfiles::default();
    };

    let mut found = TerminalProfiles::default();

    for profile in profiles {
        let Some(name) = profile.get("name").and_then(|value| value.as_str()) else {
            continue;
        };

        let Some(source) = profile.get("source").and_then(|value| value.as_str()) else {
            continue;
        };

        let hidden = profile
            .get("hidden")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let is_default = profile
            .get("guid")
            .and_then(|value| value.as_str())
            .map(|guid| guid.to_lowercase())
            .zip(default_profile.as_deref())
            .is_some_and(|(guid, default)| guid == default);

        let name = name.trim();

        if name.is_empty() {
            continue;
        }

        match source {
            "Windows.Terminal.Wsl" | "Microsoft.WSL" => found.names.push(ProfileName {
                name: name.to_string(),
                hidden,
                package: None,
                is_default,
            }),
            other if other.starts_with("Windows.Terminal.") => {}
            other => found.displays.push(ProfileName {
                name: name.to_string(),
                hidden,
                package: Some(short_package_name(other)),
                is_default,
            }),
        }
    }

    found
}

/// Drops the version hash Windows appends to a package family name, so a
/// Store profile (`CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc`) matches
/// the folder on disk (`CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc`).
fn short_package_name(package: &str) -> String {
    match package.rsplit_once('_') {
        Some((family, hash))
            if hash.len() >= 8 && hash.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            family.to_string()
        }
        _ => package.to_string(),
    }
}

/// Folds the three sources into one list.
fn merge(
    interop: &[WslDistro],
    profiles: &TerminalProfiles,
    disks: &[DiskDistro],
) -> Discovery {
    let mut distros: Vec<WslDistro> = Vec::new();

    // Interop first: it is the only source that also knows about state.
    for distro in interop {
        distros.push(distro.clone());
    }

    // Then whatever Windows Terminal knows that interop did not already name.
    for profile in &profiles.names {
        if distros
            .iter()
            .any(|distro| distro.name.eq_ignore_ascii_case(&profile.name))
        {
            continue;
        }

        distros.push(WslDistro {
            name: profile.name.clone(),
            display: profile.name.clone(),
            source: Source::WindowsTerminal,
            running: None,
            is_default: profile.is_default,
        });
    }

    // Store listings only ever improve a name we already have.
    for distro in &mut distros {
        if let Some(display) = best_display(&distro.name, profiles) {
            distro.display = display;
        }
    }

    // A disk is only worth reporting when nothing named it: a distro we cannot
    // pass to `wsl -d` would be a row that cannot open anything.
    let named_packages: Vec<String> = distros
        .iter()
        .flat_map(|distro| package_for(&distro.name, profiles))
        .collect();

    let unnamed = disks
        .iter()
        .filter(|disk| match &disk.package {
            Some(package) => !named_packages.iter().any(|named| named == package),
            // Store WSL keeps its disks in bare id folders; without a name
            // there is nothing to say about them beyond "not launchable".
            None => true,
        })
        .count();

    Discovery { distros, unnamed }
}

/// The Store listing's name for a registered distro, if it has one.
fn best_display(name: &str, profiles: &TerminalProfiles) -> Option<String> {
    let wanted = normalize_name(name);

    let mut candidates: Vec<&ProfileName> = profiles
        .displays
        .iter()
        .filter(|display| normalize_name(&display.name).starts_with(&wanted))
        .collect();

    // The friendly name and the registered name of one distro can differ by a
    // service-pack suffix ("Ubuntu 24.04" vs "Ubuntu 24.04.1 LTS"); the shorter
    // one is the name of the product rather than of the build.
    candidates.sort_by_key(|candidate| (candidate.hidden, candidate.name.len()));

    candidates.first().map(|candidate| candidate.name.clone())
}

/// The package families that could plausibly be this distro's, so its disk can
/// be recognised.
fn package_for(name: &str, profiles: &TerminalProfiles) -> Option<String> {
    let wanted = normalize_name(name);

    profiles
        .displays
        .iter()
        .find(|display| normalize_name(&display.name).starts_with(&wanted))
        .and_then(|display| display.package.clone())
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

/// A `wsl.exe` invocation for a probe.
///
/// Every probe goes through this. On Windows a console child started from a
/// GUI process is given its own console window for as long as it runs, and
/// `wsl.exe` cold-starts slowly enough for that window to be visible — the
/// distro list would come up with a black rectangle flashing over the
/// terminal. There is nothing to hide on the WSL side, so the flag is
/// Windows-only.
fn wsl_probe(exe: &Path) -> std::process::Command {
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
fn interop_list(exe: &Path) -> Option<Vec<WslDistro>> {
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
fn impostor_init() -> Option<String> {
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
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
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

fn is_version(column: &str) -> bool {
    !column.is_empty() && column.chars().all(|character| character.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows Terminal reports UTF-16LE; this is the shape `wsl -l -v` has
    /// always had, header and all.
    const VERBOSE: &str = "\r\n  NAME            STATE           VERSION\r\n* NixOS           Running         2\r\n  Ubuntu-24.04    Stopped         2\r\n  Debian          Installing      2\r\n";
    const PLAIN: &str = "\r\nUbuntu-24.04\r\nNixOS\r\n";

    /// Trimmed from a real Windows Terminal profile list: the Store profile,
    /// the hidden registry profile behind it, and the unrelated
    /// `Windows.Terminal.*` sources that must not be mistaken for distros.
    const PROFILES: &str = r#"{
      "defaultProfile": "{61683bbc-3c0b-5397-985a-4b9aa31e5ea9}",
      "profiles": {
        "list": [
          { "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}", "hidden": false, "name": "Windows PowerShell" },
          { "guid": "{b453ae62-4e3d-5e58-b989-0a998ec441b8}", "hidden": false, "name": "Azure Cloud Shell", "source": "Windows.Terminal.Azure" },
          { "guid": "{574e775e-4f2a-5b96-ac1e-a2962a402336}", "hidden": false, "name": "PowerShell", "source": "Windows.Terminal.PowershellCore" },
          { "guid": "{acbafd15-cbbb-5bb3-8a61-bed446ff4b83}", "hidden": false, "name": "Ubuntu 24.04 LTS", "source": "CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc" },
          { "guid": "{963ff2f7-6aed-5ce3-9d91-90d99571f53a}", "hidden": true, "name": "Ubuntu-24.04", "source": "Windows.Terminal.Wsl" },
          { "guid": "{d8e96812-b789-5068-a5ae-10b2fb53e95f}", "hidden": false, "name": "Ubuntu 24.04.1 LTS", "source": "CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc" },
          { "guid": "{61683bbc-3c0b-5397-985a-4b9aa31e5ea9}", "hidden": false, "name": "NixOS", "source": "Microsoft.WSL" },
          { "guid": "{5b93f4be-fad1-5578-baab-323904db4cce}", "hidden": false, "name": "Developer Command Prompt for VS 2022", "source": "Windows.Terminal.VisualStudio" }
        ]
      }
    }"#;

    #[test]
    fn verbose_list_parses_state_and_default() {
        let distros = parse_interop_list(VERBOSE);

        assert_eq!(distros.len(), 3);
        assert_eq!(distros[0].name, "NixOS");
        assert_eq!(distros[0].running, Some(true));
        assert!(distros[0].is_default);
        assert_eq!(distros[1].name, "Ubuntu-24.04");
        assert_eq!(distros[1].running, Some(false));
        assert!(!distros[1].is_default);
        assert_eq!(distros[2].name, "Debian");
        assert_eq!(distros[2].running, Some(false));
        assert_eq!(distros[2].source, Source::Interop);
    }

    #[test]
    fn plain_list_parses_names_without_state() {
        let distros = parse_interop_list(PLAIN);

        assert_eq!(distros.len(), 2);
        assert_eq!(distros[0].name, "Ubuntu-24.04");
        assert_eq!(distros[0].running, None);
        assert_eq!(distros[0].source, Source::Interop);
    }

    #[test]
    fn a_name_with_spaces_is_not_three_columns() {
        let distros =
            parse_interop_list("\n  NAME STATE VERSION\n  Alpine Linux  Stopped  2\n");

        assert_eq!(distros.len(), 1);
        assert_eq!(distros[0].name, "Alpine Linux");
        assert_eq!(distros[0].running, Some(false));
    }

    #[test]
    fn the_header_is_not_a_distro() {
        assert!(parse_interop_list("  NAME  STATE  VERSION\n").is_empty());
        assert!(parse_interop_list("").is_empty());
        assert!(parse_interop_list("\r\n \r\n").is_empty());
    }

    #[test]
    fn terminal_profiles_keep_only_wsl_distros() {
        let profiles = parse_terminal_profiles(PROFILES);

        let names: Vec<&str> = profiles.names.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Ubuntu-24.04", "NixOS"]);

        let displays: Vec<&str> =
            profiles.displays.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(displays, vec!["Ubuntu 24.04 LTS", "Ubuntu 24.04.1 LTS"]);

        // Azure, PowerShell and Visual Studio share the `source` key but are
        // not distros.
        assert!(!names.contains(&"Azure Cloud Shell"));
        assert!(!displays.iter().any(|name| name.contains("Developer")));

        // The default profile is the NixOS one.
        assert!(profiles
            .names
            .iter()
            .any(|p| p.name == "NixOS" && p.is_default));
    }

    #[test]
    fn store_listing_supplies_the_friendly_name() {
        let profiles = parse_terminal_profiles(PROFILES);
        assert_eq!(
            best_display("Ubuntu-24.04", &profiles).as_deref(),
            Some("Ubuntu 24.04 LTS")
        );
        // A distro with no Store listing keeps its registered name.
        assert_eq!(best_display("NixOS", &profiles), None);
    }

    #[test]
    fn disks_are_matched_by_package_family() {
        let profiles = parse_terminal_profiles(PROFILES);

        let ubuntu = DiskDistro {
            package: Some(short_package_name(
                "CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc",
            )),
            path: PathBuf::from("/c/Ubuntu/ext4.vhdx"),
        };
        let nameless = DiskDistro {
            package: None,
            path: PathBuf::from("/c/wsl/{a007b793}/ext4.vhdx"),
        };

        let discovery = merge(&[], &profiles, &[ubuntu, nameless]);

        assert_eq!(discovery.distros.len(), 2);
        assert_eq!(discovery.distros[0].display, "Ubuntu 24.04 LTS");
        assert_eq!(discovery.distros[0].name, "Ubuntu-24.04");
        assert_eq!(discovery.distros[1].name, "NixOS");
        // The Ubuntu disk is accounted for; the bare id folder is not.
        assert_eq!(discovery.unnamed, 1);
    }

    #[test]
    fn interop_wins_and_keeps_its_state() {
        let profiles = parse_terminal_profiles(PROFILES);
        let interop = parse_interop_list(VERBOSE);

        let discovery = merge(&interop, &profiles, &[]);

        assert_eq!(discovery.distros.len(), 3);
        assert_eq!(discovery.distros[0].name, "NixOS");
        assert_eq!(discovery.distros[0].running, Some(true));
        assert_eq!(discovery.distros[0].source, Source::Interop);
        // Interop's names still get the Store name for display.
        let ubuntu = discovery
            .distros
            .iter()
            .find(|distro| distro.name == "Ubuntu-24.04")
            .expect("Ubuntu-24.04 in the merged list");
        assert_eq!(ubuntu.display, "Ubuntu 24.04 LTS");
    }

    #[test]
    fn a_profile_interop_never_mentioned_is_still_listed() {
        let profiles = parse_terminal_profiles(PROFILES);
        let interop = parse_interop_list("\n  NAME STATE VERSION\n* NixOS  Running  2\n");

        let discovery = merge(&interop, &profiles, &[]);
        let names: Vec<&str> = discovery
            .distros
            .iter()
            .map(|distro| distro.name.as_str())
            .collect();

        assert_eq!(names, vec!["NixOS", "Ubuntu-24.04"]);
        assert_eq!(discovery.distros[1].source, Source::WindowsTerminal);
    }

    #[test]
    fn windows_output_is_decoded_from_utf16() {
        let utf16: Vec<u8> = "\r\nUbuntu-24.04\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(decode_windows_output(&utf16), "\r\nUbuntu-24.04\r\n");
        assert_eq!(decode_windows_output(b"Ubuntu-24.04\n"), "Ubuntu-24.04\n");
        assert_eq!(decode_windows_output(&[]), "");
    }

    #[test]
    fn launch_args_are_what_wsl_expects() {
        let distro = WslDistro {
            name: "Ubuntu-24.04".to_string(),
            display: "Ubuntu 24.04 LTS".to_string(),
            source: Source::Interop,
            running: None,
            is_default: false,
        };

        assert_eq!(distro.launch_args(), vec!["-d", "Ubuntu-24.04"]);
    }

    #[test]
    fn package_hashes_are_dropped_but_real_underscores_are_kept() {
        assert_eq!(
            short_package_name("CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc"),
            "CanonicalGroupLimited.Ubuntu24.04LTS"
        );
        assert_eq!(
            short_package_name("Ubuntu_24.04"),
            "Ubuntu_24.04",
            "a short suffix is part of the name, not a version hash"
        );
        assert_eq!(short_package_name("NixOS"), "NixOS");
    }

    #[test]
    fn roots_without_a_windows_drive_are_absent() {
        assert!(WindowsRoots::from_mount("/definitely/not/here", vec![])
            .wsl_exe()
            .is_none());
    }

    #[test]
    fn garbage_json_is_not_a_panic() {
        assert!(parse_terminal_profiles("not json").is_empty());
        assert!(parse_terminal_profiles("{}").is_empty());
        assert!(parse_terminal_profiles(r#"{"profiles": {"list": []}}"#).is_empty());
    }

    #[test]
    fn interop_hint_names_the_culprit() {
        // This test runs on a machine that may or may not have interop; the
        // hint must always say something actionable.
        let hint = interop_hint();
        assert!(hint.contains("WSL interop"));
    }

    /// Not a unit test: it prints what the *real* Windows side of whatever
    /// machine it runs on answers. Fixtures can drift from the formats
    /// `wsl.exe` and Windows Terminal actually emit, and this is the only
    /// check that would notice.
    ///
    ///     cargo test -p terminus-core -- --ignored --nocapture discovery_on_this_machine
    #[test]
    #[ignore]
    fn discovery_on_this_machine() {
        let Some(roots) = WindowsRoots::detect() else {
            println!("no Windows drive mounted: this is not a WSL machine");
            return;
        };

        println!("windows mount   {:?}", roots.mount);
        println!("user profiles   {:?}", roots.profiles);
        println!("wsl.exe         {:?}", roots.wsl_exe());

        match roots.wsl_exe() {
            Some(exe) => match interop_list(&exe) {
                Some(list) => println!("interop         {list:?}"),
                None => println!("interop         unavailable ({})", interop_hint()),
            },
            None => println!("interop         no wsl.exe on the Windows drive"),
        }

        let profiles = roots
            .profiles
            .iter()
            .filter_map(|profile| {
                let path = profile.join(
                    "AppData/Local/Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json",
                );
                std::fs::read_to_string(path).ok()
            })
            .next();
        println!(
            "terminal        {:?}",
            profiles
                .as_deref()
                .map(|json| parse_terminal_profiles(json).names)
        );

        let discovered = discover(&roots);
        println!("on disk         {:?}", distro_disks(&roots));
        for distro in &discovered.distros {
            println!(
                "distro          {} ({}) [{:?} running={:?} default={}]",
                distro.display,
                distro.name,
                distro.source,
                distro.running,
                distro.is_default
            );
        }
        println!("unnamed         {}", discovered.unnamed);
        println!("current         {:?}", crate::machine::detect().wsl_distro);
    }

    /// `interop_ready` must not re-run `wsl.exe` on every session open: the
    /// probe cold-starts the WSL service, and session opens are interactive.
    /// The fake `wsl.exe` here counts its own invocations, so this fails if the
    /// memoisation is ever dropped.
    #[cfg(unix)]
    #[test]
    fn interop_is_probed_once_per_process() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir()
            .join(format!("terminus-wsl-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let counter = dir.join("count");
        let script = dir.join("wsl");

        std::fs::write(
            &script,
            format!("#!/bin/sh\necho run >> '{}'\nexit 0\n", counter.display()),
        )
        .expect("script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod");

        assert_eq!(interop_ready(&script), Ok(()));
        assert_eq!(interop_ready(&script), Ok(()));

        let runs = std::fs::read_to_string(&counter).unwrap_or_default();
        assert_eq!(
            runs.lines().count(),
            1,
            "the probe ran {} times, not once",
            runs.lines().count()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
