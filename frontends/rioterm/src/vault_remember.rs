//! Persist the vault passphrase ("remember me").
//!
//! Prefer the OS credential store (Windows Credential Manager, macOS
//! Keychain, Linux keyutils). On Unix we also keep a 0600 fallback file
//! under the Terminus data dir so "forever" survives reboot when keyutils
//! does not.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

const SERVICE: &str = "terminus.vault";
const USER: &str = "passphrase";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, USER).map_err(|e| e.to_string())
}

fn fallback_path() -> PathBuf {
    crate::hosts::data_dir().join("vault.remember")
}

/// Store the vault passphrase for automatic unlock on later launches.
pub fn remember_passphrase(passphrase: &str) -> Result<(), String> {
    if let Err(err) =
        entry().and_then(|e| e.set_password(passphrase).map_err(|e| e.to_string()))
    {
        tracing::warn!("keyring remember failed ({err}); using local fallback");
        write_fallback(passphrase)?;
    }
    // Mirror to fallback on Unix so reboot does not lose the secret when
    // only keyutils (in-memory) is available.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = write_fallback(passphrase);
    }
    Ok(())
}

/// Load a previously remembered passphrase, if any.
pub fn load_remembered_passphrase() -> Option<String> {
    if let Ok(pw) = entry().ok()?.get_password() {
        if !pw.is_empty() {
            return Some(pw);
        }
    }
    read_fallback()
}

/// Whether a passphrase is currently stored.
pub fn has_remembered_passphrase() -> bool {
    load_remembered_passphrase().is_some()
}

/// Forget a stored passphrase (checkbox unchecked, or bad unlock).
pub fn forget_passphrase() {
    if let Ok(e) = entry() {
        let _ = e.delete_credential();
    }
    let path = fallback_path();
    let _ = fs::remove_file(path);
}

fn write_fallback(passphrase: &str) -> Result<(), String> {
    let path = fallback_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_private(&path, passphrase)
}

fn write_private(path: &std::path::Path, contents: &str) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    f.write_all(contents.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::write_private;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn private_file_is_0600_even_when_it_already_existed() {
        let path = std::env::temp_dir()
            .join(format!("terminus-remember-{}", std::process::id()));
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, "secret").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret");
        let _ = std::fs::remove_file(&path);
    }
}

fn read_fallback() -> Option<String> {
    let raw = fs::read_to_string(fallback_path()).ok()?;
    let trimmed = raw.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
