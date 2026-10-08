//! Temp files that hand a vault secret to the system `ssh` binary.
//!
//! Host tabs and tunnels run OpenSSH (see `screen::shell::ssh_shell`), which
//! can only take a managed private key as an `IdentityFile` and a password or
//! passphrase through an `SSH_ASKPASS` helper reading a file. Those files are
//! the only place a decrypted secret touches the disk, so they live in a
//! per-user `0700` directory with mode `0600`, and are removed as early as
//! each path allows:
//!
//! - **connected**: ssh's own `LocalCommand` deletes them right after
//!   authentication succeeds ([`local_command_removing`]);
//! - **failed / closed**: the tab or tunnel holding a [`SecretFiles`] guard
//!   deletes them when its `ssh` process is gone;
//! - **quit**: [`shred_all`] runs before `process::exit`;
//! - **anything missed** (a crash, a hung handshake): a TTL thread per file
//!   and [`sweep_on_startup`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Private dir (under [`private_temp_dir`]) holding temp identity files.
pub const IDENTITY_DIR: &str = "ssh-identity";
/// Private dir holding askpass secrets and the helper script.
pub const ASKPASS_DIR: &str = "ssh-askpass";
/// Env var the askpass helper reads the secret file path from.
pub const ASKPASS_FILE_ENV: &str = "TERMINUS_SSH_ASKPASS_FILE";

const IDENTITY_EXT: &str = "pem";
const ASKPASS_EXT: &str = "secret";

/// How long a temp secret may live before the fallback removes it (OpenSSH
/// reads them during the handshake only).
pub const TTL: std::time::Duration = std::time::Duration::from_secs(120);

/// Secrets written by this process and not removed yet.
static LIVE: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Where [`private_temp_dir`] puts `name`, without creating it.
pub fn private_temp_dir_path(name: &str) -> PathBuf {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        base.join(format!("terminus-{name}-{uid}"))
    }
    #[cfg(not(unix))]
    {
        // %TEMP% is already per-user on Windows.
        std::env::temp_dir().join(format!("terminus-{name}"))
    }
}

/// Per-user private directory for SSH temp secrets (`0700`, owned by us).
///
/// Lives under `$XDG_RUNTIME_DIR` when set (already per-user), else the temp
/// dir with the uid in the name. An existing directory is only reused when it
/// is a real directory we own (never a symlink) and gets its mode tightened,
/// so another local user cannot pre-create it, read the secrets or swap the
/// askpass helper.
pub fn private_temp_dir(name: &str) -> Result<PathBuf, String> {
    let dir = private_temp_dir_path(name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        let uid = unsafe { libc::getuid() };
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(format!("Could not create {}: {err}", dir.display())),
        }
        let meta = std::fs::symlink_metadata(&dir)
            .map_err(|e| format!("Could not inspect {}: {e}", dir.display()))?;
        if !meta.file_type().is_dir() || meta.uid() != uid {
            return Err(format!(
                "Refusing to use {}: not a directory owned by this user",
                dir.display()
            ));
        }
        if meta.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("Could not secure {}: {e}", dir.display()))?;
        }
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Create `path` exclusively with owner-only permissions and write `data`.
pub fn write_private_file(path: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    file.write_all(data)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))
}

/// Start tracking a secret just written: [`shred_all`] will remove it, and
/// so will a background thread once [`TTL`] has passed.
pub fn track(path: PathBuf) {
    if let Ok(mut live) = LIVE.lock() {
        live.insert(path.clone());
    }
    std::thread::spawn(move || {
        std::thread::sleep(TTL);
        shred(&path);
    });
}

/// Overwrite `path` with zeros, then delete it. Best effort and idempotent;
/// symlinks are unlinked, never followed.
pub fn shred(path: &Path) {
    if let Ok(mut live) = LIVE.lock() {
        live.remove(path);
    }
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_file() {
        // Zeroing first keeps the secret out of the freed blocks on a
        // plain filesystem; on CoW/SSD it is at worst wasted work.
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(path) {
            let zeros = vec![0u8; meta.len() as usize];
            let _ = file.write_all(&zeros);
            let _ = file.sync_all();
        }
    }
    let _ = std::fs::remove_file(path);
}

/// Remove every secret this process still has on disk (app quit).
pub fn shred_all() {
    let paths: Vec<PathBuf> = match LIVE.lock() {
        Ok(live) => live.iter().cloned().collect(),
        Err(_) => return,
    };
    for path in paths {
        shred(&path);
    }
}

/// Whether `path` is one of our temp secrets: a `.pem` in the identity dir
/// or a `.secret` in the askpass dir. Anything else (a user's own
/// `~/.ssh/id_ed25519` passed with `-i` in a custom shell) is never touched.
pub fn is_managed_secret(path: &Path) -> bool {
    let (Some(parent), Some(ext)) = (path.parent(), path.extension()) else {
        return false;
    };
    [(IDENTITY_DIR, IDENTITY_EXT), (ASKPASS_DIR, ASKPASS_EXT)]
        .iter()
        .any(|(dir, want)| ext == *want && parent == private_temp_dir_path(dir))
}

/// The managed secrets a launch refers to: the `-i` IdentityFile and the
/// askpass secret named in its env.
pub fn launch_secrets<'a>(
    args: impl IntoIterator<Item = &'a str>,
    env: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "-i" {
            if let Some(file) = args.next() {
                paths.push(PathBuf::from(file));
            }
        }
    }
    paths.extend(
        env.into_iter()
            .filter(|(k, _)| *k == ASKPASS_FILE_ENV)
            .map(|(_, v)| PathBuf::from(v)),
    );
    paths.retain(|p| is_managed_secret(p));
    paths
}

/// OpenSSH `LocalCommand` that deletes `paths`.
///
/// ssh runs it through `$SHELL -c` on this machine once authentication has
/// succeeded, which is the earliest moment the key and passphrase are no
/// longer needed. `%` is ssh's token escape and is doubled. `None` when a
/// path cannot be quoted safely (the other cleanup paths still apply), and
/// on Windows, whose OpenSSH port runs it through `cmd`.
pub fn local_command_removing(paths: &[&Path]) -> Option<String> {
    if cfg!(windows) || paths.is_empty() {
        return None;
    }
    let mut cmd = String::from("rm -f --");
    for path in paths {
        let path = path.to_str()?;
        if path.contains('\'') || path.contains('\n') {
            return None;
        }
        cmd.push_str(" '");
        cmd.push_str(&path.replace('%', "%%"));
        cmd.push('\'');
    }
    Some(cmd)
}

/// Removes its secrets on drop: held by whatever owns the `ssh` process (a
/// tab's context, a tunnel), so a session that fails, is closed or never
/// spawns leaves nothing behind.
#[derive(Debug, Default)]
pub struct SecretFiles(Vec<PathBuf>);

impl SecretFiles {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self(paths.into_iter().filter(|p| is_managed_secret(p)).collect())
    }
}

impl Drop for SecretFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            shred(path);
        }
    }
}

/// Remove secrets older than [`TTL`] from `dir` (leftovers of a session whose
/// cleanup never ran: the app crashed or was killed).
pub fn sweep_stale_secrets(dir: &Path, extensions: &[&str]) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let wanted = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.contains(&e));
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > TTL);
        if wanted && stale {
            shred(&path);
        }
    }
}

/// Remove world-readable secrets left in the shared temp dirs used by older
/// builds (`/tmp/terminus-ssh-askpass`, `/tmp/terminus-ssh-identity`).
pub fn sweep_legacy_secret_dirs() {
    for (name, ext) in [
        ("terminus-ssh-askpass", ASKPASS_EXT),
        ("terminus-ssh-identity", IDENTITY_EXT),
    ] {
        let dir = std::env::temp_dir().join(name);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Startup cleanup: whatever a crashed previous run left in the private
/// dirs, without waiting for the next connection to trigger a sweep.
pub fn sweep_on_startup() {
    sweep_legacy_secret_dirs();
    // Age-gated: a second running instance may be mid-handshake.
    sweep_stale_secrets(&private_temp_dir_path(IDENTITY_DIR), &[IDENTITY_EXT]);
    sweep_stale_secrets(&private_temp_dir_path(ASKPASS_DIR), &[ASKPASS_EXT]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_identity() -> PathBuf {
        let dir = private_temp_dir(IDENTITY_DIR).unwrap();
        let path = dir.join(format!("{}.{IDENTITY_EXT}", uuid::Uuid::new_v4()));
        write_private_file(&path, b"-----BEGIN OPENSSH PRIVATE KEY-----\n").unwrap();
        path
    }

    fn write_askpass_secret() -> PathBuf {
        let dir = private_temp_dir(ASKPASS_DIR).unwrap();
        let path = dir.join(format!("{}.{ASKPASS_EXT}", uuid::Uuid::new_v4()));
        write_private_file(&path, b"hunter2\n").unwrap();
        path
    }

    #[test]
    fn only_our_private_dirs_hold_managed_secrets() {
        let key = write_identity();
        let secret = write_askpass_secret();
        assert!(is_managed_secret(&key));
        assert!(is_managed_secret(&secret));
        // Right dir, wrong kind: the askpass helper itself.
        assert!(!is_managed_secret(
            &private_temp_dir_path(ASKPASS_DIR).join("askpass.sh")
        ));
        // A user's own key passed with `-i`.
        assert!(!is_managed_secret(Path::new("/home/alice/.ssh/id_ed25519")));
        assert!(!is_managed_secret(&std::env::temp_dir().join("x.pem")));
        shred(&key);
        shred(&secret);
    }

    #[test]
    fn launch_secrets_finds_identity_and_askpass_file() {
        let key = write_identity();
        let secret = write_askpass_secret();
        let key_s = key.to_string_lossy().into_owned();
        let secret_s = secret.to_string_lossy().into_owned();
        let args = [
            "-p",
            "22",
            "-i",
            key_s.as_str(),
            "-i",
            "/home/a/.ssh/id",
            "a@b",
        ];
        let env = [
            ("TERM", "xterm-256color"),
            (ASKPASS_FILE_ENV, secret_s.as_str()),
        ];
        assert_eq!(launch_secrets(args, env), vec![key.clone(), secret.clone()]);
        assert!(launch_secrets(["-p", "22", "a@b"], [("TERM", "x")]).is_empty());
        shred(&key);
        shred(&secret);
    }

    #[test]
    fn shred_removes_the_file_and_tolerates_a_missing_one() {
        let key = write_identity();
        shred(&key);
        assert!(!key.exists());
        shred(&key);
    }

    #[test]
    fn guard_removes_its_secrets_when_the_session_ends() {
        let key = write_identity();
        let secret = write_askpass_secret();
        let guard = SecretFiles::new(vec![key.clone(), secret.clone()]);
        assert!(key.exists() && secret.exists());
        drop(guard);
        assert!(!key.exists(), "identity left on disk after the session");
        assert!(
            !secret.exists(),
            "askpass secret left on disk after the session"
        );
    }

    #[test]
    fn guard_never_deletes_a_file_it_does_not_manage() {
        let dir =
            std::env::temp_dir().join(format!("terminus-user-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let users_key = dir.join("id_ed25519");
        std::fs::write(&users_key, b"mine").unwrap();
        drop(SecretFiles::new(vec![users_key.clone()]));
        assert!(users_key.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shred_all_removes_tracked_secrets() {
        let key = write_identity();
        track(key.clone());
        shred_all();
        assert!(!key.exists());
    }

    #[cfg(unix)]
    #[test]
    fn local_command_deletes_the_files_once_ssh_runs_it() {
        let key = write_identity();
        let secret = write_askpass_secret();
        let cmd = local_command_removing(&[&key, &secret]).expect("command");
        // ssh expands `%%` to `%` before handing the line to the shell.
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd.replace("%%", "%"))
            .status()
            .unwrap();
        assert!(status.success());
        assert!(!key.exists());
        assert!(!secret.exists());
    }

    #[cfg(unix)]
    #[test]
    fn local_command_escapes_ssh_tokens_and_refuses_unquotable_paths() {
        let cmd = local_command_removing(&[Path::new("/run/u/a%h.pem")]).unwrap();
        assert!(cmd.contains("a%%h.pem"), "{cmd}");
        assert!(!cmd.replace("%%", "").contains('%'), "{cmd}");
        assert_eq!(local_command_removing(&[Path::new("/tmp/it's.pem")]), None);
        assert_eq!(local_command_removing(&[]), None);
    }

    #[test]
    fn startup_sweep_keeps_fresh_secrets() {
        // Another running instance may be mid-handshake with a fresh file.
        let key = write_identity();
        sweep_on_startup();
        assert!(key.exists());
        shred(&key);
    }
}
