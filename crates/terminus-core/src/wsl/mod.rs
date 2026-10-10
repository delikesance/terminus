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

mod disks;
mod interop;
mod merge;
mod profiles;
mod roots;
#[cfg(test)]
mod tests;

pub use disks::DiskDistro;
use disks::*;
use interop::*;
pub use interop::{
    decode_windows_output, interop_hint, interop_ready, interop_short_hint,
    parse_interop_list, probe_interop,
};
use merge::*;
use profiles::*;
pub use profiles::{parse_terminal_profiles, TerminalProfiles};
pub use roots::WindowsRoots;
