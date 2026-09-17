# SFTP test matrix

Coverage target: every SFTP feature (shipped + roadmap 2.2b–2.4).
Status: **green** = passing.

| Feature | Unit test(s) | E2E test(s) | Status |
|---|---|---|---|
| Open SFTP (host menu) | `context_menu::host_menu_includes_open_sftp` | `e2e_open_remote_lists_root` | green |
| Open in other pane (Host\|Host) | `context_menu::host_menu_includes_open_sftp_other_pane` | `e2e_host_host_two_remotes` | green |
| Browse / enter dir | `sftp_pane::hit_test_rows_and_parent` | `e2e_cd_into_directory` | green |
| Parent / Backspace | `sftp_pane::parent_and_join_paths` | `e2e_cd_parent` | green |
| Focus side / Tab | `sftp_ui::key_tab_toggles_focus` | `e2e_tab_focus` | green |
| Mkdir (menu + Ctrl+N) | `sftp_pane::name_edit_*`, `sftp_ui::key_mkdir` | `e2e_mkdir_local` | green |
| Rename (F2) | `sftp_pane::name_edit_rename_label`, `sftp_ui::key_rename` | `e2e_rename_local` | green |
| Delete file | `sftp_worker::remove_local_file` | `e2e_delete_file` | green |
| Delete dir recursive (remote) | `sftp::remove_remote_recursive_desired` | `e2e_delete_remote_dir_tree` | green |
| Transfer file | `sftp_worker::transfer_local_to_local` | `e2e_transfer_file` | green |
| Transfer folder (2.3) | `sftp_worker::transfer_folder_command_exists` | `e2e_transfer_folder`, `e2e_transfer_folder_remote_to_local` | green (zip when a side is local) |
| Transfer folder Host\|Host | — | `e2e_transfer_folder_host_to_host` | green (remote tar/zip or PowerShell Compress-Archive; error if none) |
| Edit remote | `sftp_worker::edit_temp_path_*` | `e2e_edit_remote_ready` | green |
| Refresh | `sftp_ui::refresh_focused_lists` | `e2e_refresh` | green |
| Close / Esc | `sftp_pane::hit_test_close_*` | `e2e_close_emits_closed` | green |
| Breadcrumbs navigate (2.2b) | `sftp_pane::crumb_segments_and_navigate` | `e2e_click_crumb_cds` | green |
| Progress / queue / retry (2.3) | `sftp_worker::transfer_queue_events_desired` | `e2e_transfer_queue` | green |
| Palette `>sftp` (2.4) | `command_palette::sftp_action_listed` | `e2e_palette_opens_sftp` | green |
| Keyboard map | `sftp_ui::keyboard_map_table` | `e2e_keys_drive_session` | green |
| Empty-area context menu | `context_menu::sftp_empty_menu` | `e2e_empty_menu_actions` | green |
| Path sandbox / traversal | `sftp::normalization_*`, `sandbox_*` | `e2e_rejects_dotdot` | green |
| Auth options → SFTP connect | `ssh::connect_sftp_options_from_host_carry_auth` | `e2e_connect_with_password_fixture` | green |
| Overlay under modal | — | `e2e_lists_while_modal_flag` | green |

## How to run

```sh
cargo test -p terminus-ui sftp_pane context_menu
cargo test -p terminus-core sftp::
cargo test -p terminus-bridge --lib
cargo test -p terminus-bridge --test sftp_e2e
cargo test -p rioterm --bin rio sftp_
```
