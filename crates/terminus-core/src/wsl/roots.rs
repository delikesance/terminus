use std::path::{Path, PathBuf};

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
    pub(super) fn detect_from_drive(drive: &Path) -> Option<Self> {
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

    pub(super) fn local_app_data(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.profiles
            .iter()
            .map(|profile| profile.join("AppData/Local"))
    }
}
