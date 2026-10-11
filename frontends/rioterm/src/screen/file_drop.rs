//! A file dropped on a terminal tab. On a local or WSL tab its local path is
//! typed. On an SSH tab the file is copied to the host's temp dir first, and
//! the path it has there is typed instead, since the local path means nothing
//! to the remote shell.

use super::image_paste::PasteTarget;
use super::remote_upload::{paste_upload_result, quote_for_shell, upload_file, RemoteOs};
use super::Screen;
use crate::crosswords::Mode;
use crate::platform::shell_escape;
use std::fmt::Display;
use std::fs::File;
use std::path::Path;

/// Prefix shared by every dropped file copied to a remote host.
const DROP_PREFIX: &str = "terminus-drop-";

impl Screen<'_> {
    pub fn drop_file(&mut self, path: &Path) {
        match self.paste_target() {
            PasteTarget::Ssh(id) => {
                if let Err(err) = self.upload_dropped_file(&id, path) {
                    self.report_paste_failure(&format!("File drop failed: {err}"));
                }
            }
            _ => self.paste(&(shell_escape(&path.to_string_lossy()) + " "), true),
        }
    }

    fn upload_dropped_file(&mut self, id: &str, path: &Path) -> Result<(), String> {
        if path.is_dir() {
            return Err("folders cannot be dropped on an SSH tab".into());
        }
        let file =
            File::open(path).map_err(|err| format!("{}: {err}", path.display()))?;
        let original = path.file_name().unwrap_or_default().to_string_lossy();
        let name = drop_file_name(chrono::Local::now(), &original);
        let ssh = self.ssh_upload_for(id)?;
        let windows = ssh.os == RemoteOs::Windows;
        let bracketed = self.get_mode().contains(Mode::BRACKETED_PASTE);
        let input = self.context_manager.current().messenger.channel.clone();
        let report = self.paste_errors.reporter();
        paste_upload_result("File drop failed", input, bracketed, report, move || {
            upload_file(&ssh, &name, file).map(|remote| quote_for_shell(&remote, windows))
        })
    }
}

/// `terminus-drop-20261011-101500-042-report.pdf`: unique per drop, and the
/// original name is kept, with only characters every shell takes.
pub(super) fn drop_file_name<Tz: chrono::TimeZone>(
    now: chrono::DateTime<Tz>,
    original: &str,
) -> String
where
    Tz::Offset: Display,
{
    let stamp = now.format("%Y%m%d-%H%M%S-%3f");
    let safe: String = original
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{DROP_PREFIX}{stamp}-{safe}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn dropped_file_keeps_its_name_under_a_unique_prefix() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 10, 11, 10, 15, 0)
            .unwrap()
            .checked_add_signed(chrono::TimeDelta::milliseconds(42))
            .unwrap();
        assert_eq!(
            drop_file_name(now, "rapport.pdf"),
            "terminus-drop-20261011-101500-042-rapport.pdf"
        );
    }

    #[test]
    fn characters_a_shell_could_mangle_become_underscores() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 10, 11, 10, 15, 0)
            .unwrap();
        assert_eq!(
            drop_file_name(now, "a$b;c'd\"e f.txt"),
            "terminus-drop-20261011-101500-000-a_b_c_d_e_f.txt"
        );
    }
}
