//! Command history capture rules: which commands are recorded and how a
//! machine (sidebar row id) maps onto the `history` table columns.

use uuid::Uuid;

/// Environment switch: `TERMINUS_HISTORY=0|off|false|no` disables recording.
pub const ENV_DISABLE: &str = "TERMINUS_HISTORY";
/// `settings` key mirroring [`ENV_DISABLE`]; same values.
pub const SETTING_KEY: &str = "history_recording";

/// Longest command kept; anything longer is almost certainly pasted data.
pub const MAX_COMMAND_LEN: usize = 4096;

/// `true` unless the value says "off".
pub fn recording_enabled_value(value: Option<&str>) -> bool {
    !matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("0" | "off" | "false" | "no")
    )
}

/// Whether `command` may be stored. Mirrors bash `HISTCONTROL=ignorespace`:
/// a leading space or tab keeps the command out of history. Blank and
/// oversized text is dropped too.
pub fn should_record(command: &str) -> bool {
    if command.trim().is_empty() || command.len() > MAX_COMMAND_LEN {
        return false;
    }
    !command.starts_with([' ', '\t'])
}

/// Where a history row lives for a sidebar row id (`local`, `wsl:<distro>`
/// or a host uuid): `(host_id column, session_kind column)`.
///
/// Only SSH hosts have a uuid; local and WSL rows store a NULL host and put
/// the full row id in `session_kind`, so each machine has one filter value.
pub fn machine_columns(row_id: &str) -> (Option<Uuid>, String) {
    match Uuid::parse_str(row_id) {
        Ok(id) => (Some(id), "ssh".to_string()),
        Err(_) => (None, row_id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_space_or_tab_is_not_recorded() {
        assert!(!should_record(" secret --token x"));
        assert!(!should_record("\tls"));
        assert!(should_record("ls -la"));
        assert!(should_record("ls "));
    }

    #[test]
    fn blank_and_huge_commands_are_not_recorded() {
        assert!(!should_record(""));
        assert!(!should_record("   "));
        assert!(!should_record(&"a".repeat(MAX_COMMAND_LEN + 1)));
        assert!(should_record(&"a".repeat(MAX_COMMAND_LEN)));
    }

    #[test]
    fn recording_switch_values() {
        assert!(recording_enabled_value(None));
        assert!(recording_enabled_value(Some("1")));
        assert!(recording_enabled_value(Some("")));
        for off in ["0", "off", "OFF", "false", " no "] {
            assert!(!recording_enabled_value(Some(off)), "{off}");
        }
    }

    #[test]
    fn machine_columns_split_ssh_from_local_and_wsl() {
        let id = Uuid::new_v4();
        assert_eq!(
            machine_columns(&id.to_string()),
            (Some(id), "ssh".to_string())
        );
        assert_eq!(machine_columns("local"), (None, "local".to_string()));
        assert_eq!(
            machine_columns("wsl:Ubuntu"),
            (None, "wsl:Ubuntu".to_string())
        );
    }
}
