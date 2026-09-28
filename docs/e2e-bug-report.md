# E2E bug report — 2026-09-28

Features were exercised end to end: against a live OpenSSH server, through the
store/vault/sync APIs, and in the real `rio` binary driven with xdotool under
Xvfb. Baseline before testing: every existing suite passed (terminus-core 99,
terminus-ui 170, terminus-bridge 67, rioterm 252).

New suites (each known bug is an `#[ignore = "BUG: …"]` test, so CI stays green
and `-- --ignored` reproduces every failure):

* `crates/terminus-core/tests/e2e_data.rs`: store, vault, sync and managed keys.
* `crates/terminus-bridge/tests/real_sshd_e2e.rs`: SSH/SFTP against a real
  sshd. It is skipped unless `TERMINUS_E2E_SSHD` is set (setup at the end).

## High severity

| # | Feature | Bug | Evidence |
|---|---|---|---|
| 1 | Remote SQL Sync | `SyncEngine::push_all` / `pull_all` are stubs. Settings still shows **Connected** and "Sync ok — pushed 0, pulled 0", and it saves `last_sync`. The remote DB stays a 0-byte file. Users will believe their hosts are backed up. | `sync_now_actually_pushes_hosts_to_remote`; GUI: remote.db is 0 bytes after Test Sync |
| 2 | SSH shell (password) | The decrypted vault password is written to `/tmp/terminus-ssh-askpass/<uuid>.secret` with mode **0644**, so any local user can read it. Cleanup is a 120 s sleeping thread, so if the app exits first the file stays forever (a file from an earlier run was still present). | GUI: `cat` of the secret printed the SSH password; `ssh_shell` / `write_ssh_askpass` in `screen/mod.rs` |
| 3 | SFTP transfer | Upload and download buffer the whole file in RAM and run it under **one** 30 s `SftpSession` timeout. A 40 MB file on a 1 MB/s link fails at 30 s and leaves a **truncated** 29.5 MB file on the remote, overwriting any previous version. | `worker_upload_over_slow_link_does_not_hit_30s_whole_file_timeout` |
| 4 | SFTP browse | readdir attributes are lstat, so **symlinked directories are listed as files**. Double-clicking a "file" transfers it, so double-clicking `/lib` (→ `usr/lib`) to open it starts downloading the whole tree. | `sftp_lists_symlinked_directory_as_directory`; GUI: `bin`/`lib`/`sbin` show as 7–9 B, and double-clicking `link-to-tree` downloaded it |
| 5 | SFTP transfer | A single-file transfer (including a double-click) **silently overwrites** a same-named file on the other side. Folder transfers do prompt. | `worker_single_file_transfer_prompts_before_overwrite` |
| 6 | Managed SSH keys | Private keys are stored as **plaintext PEM** in `terminus.db` (host passwords *are* vault-sealed), and the DB file is created **0644**. | `managed_private_key_not_stored_in_plaintext`, `store_file_is_private` |

## Medium severity

| # | Feature | Bug | Evidence |
|---|---|---|---|
| 7 | Auth | Password hosts fail on servers that only offer `keyboard-interactive` (common with PAM or 2FA front-ends): russh only tries `password`, and the CLI path forces `PreferredAuthentications=password`. | `keyboard_interactive_only_server_accepts_password`; manual `ssh` run with the app's flags → `Permission denied (publickey,keyboard-interactive)` |
| 8 | Host keys (SFTP) | If `known_hosts` has the server's *ecdsa* key (as written by OpenSSH, which the shell tab uses) and russh negotiates ed25519, the result is **"host key changed … verify the server"**. That is a false MITM alarm that blocks SFTP. | `tofu_with_openssh_recorded_other_algorithm_is_not_a_mitm` |
| 9 | Host keys | Hashed `known_hosts` entries (`\|1\|…`, the Debian/Ubuntu default) are never matched, so `Strict` refuses known hosts and TOFU appends duplicate plaintext entries. | `strict_accepts_hashed_known_hosts_entry` |
| 10 | SFTP paths | `normalize_remote_path` trims whitespace and treats `\` as a separator on every server, so remote files named `"trail "` or `a\b.txt` cannot be read, downloaded or deleted. | `sftp_handles_names_with_edge_whitespace_and_backslash` |
| 11 | Store | `list_hosts` / `list_groups` `unwrap()` UUIDs and RFC 3339 dates. One row in another format (e.g. `2026-09-28 12:00:00`) **panics** the host worker and the sidebar goes empty. | `list_hosts_survives_one_malformed_row` |
| 12 | Vault | `decrypt` calls `XNonce::from_slice` on an unchecked length, so a corrupt or foreign envelope **panics** instead of returning an error. | `vault_decrypt_with_bad_nonce_length_does_not_panic` |
| 13 | Delete host | `delete_host` does not bump `updated_at`, so a future last-writer-wins sync would resurrect the host. The sealed password in `credentials` is left behind, there is no confirmation or undo, and the host's open SSH session loses its sidebar entry. | `delete_host_bumps_updated_at`; GUI + DB inspection |
| 14 | Import key | Settings import always passes `passphrase = None`, so any passphrase-protected key fails with "The key is encrypted". | `import_encrypted_pem_without_passphrase_field` |
| 15 | Palette | `>sftp` → Open SFTP always opens the **first** host (`hosts.first()`), with no way to choose another. | `screen/mod.rs` `PaletteAction::OpenSftp` |

## Low severity / UI

* SSH tab title bar shows the host **UUID** (`a2d52fbe-… · local-pw`); it
  clipped to `574 · local-pw` while the context menu was open.
* With an SFTP pane visible, the sidebar and activity-bar labels, OS icons and
  file icons are not drawn, or only partly drawn ("computer" without "This").
  They come back when you switch to Terminal. Seen under Xvfb with software
  rendering; please confirm on a GPU.
* The SFTP header's long local path overflows into and covers the remote
  pane's path.
* The remote SFTP pane always opens at `/` instead of the user's home.
* On first use the vault is **created** from an "Unlock Vault" prompt with no
  confirm-passphrase field, so a typo permanently locks the passwords saved
  after it. The dialog's description text runs past its right edge.
* The add-host error line is truncated ("Authentication failed (wrong pass…").
* `vault::credential_id` derives persisted/synced IDs from `DefaultHasher`,
  whose algorithm Rust does not guarantee to stay the same across releases.

## Verified working

Password and ed25519/RSA key auth, and wrong-password classification. A
generated managed key authenticates once authorized. TOFU record → known →
changed-key refusal. russh shell I/O and resize. Remote OS detection. A 20 MB
upload/download round trip with checksum match. Folder upload with spaces,
unicode, quotes and a dir symlink. Remote folder download with `'`, `$` and
backticks in the name. Edit-remote save → re-upload. Recursive remote delete
(does not follow a symlink to `/etc`). Group reorder, and group delete keeping
its hosts. Vault wrong/right passphrase. In the GUI: terminal I/O and UTF-8,
the add-host form with a failed then successful probe, vault seal (the
`password` column stays NULL), the OpenSSH shell tab, and SFTP browsing.

## Reproducing the real-sshd suite (Linux)

```sh
useradd -m tuser && echo 'tuser:hunter2' | chpasswd
ssh-keygen -t ed25519 -N '' -f /tmp/e2e_key   # add .pub to ~tuser/.ssh/authorized_keys
# sshd #1: port 2222, PasswordAuthentication yes, Subsystem sftp internal-sftp
# sshd #2: port 2223, PasswordAuthentication no, KbdInteractiveAuthentication yes
# throttled proxy 2224 → 2222 at ~1 MB/s (any TCP rate limiter)
TERMINUS_E2E_SSHD=127.0.0.1:2222 TERMINUS_E2E_KBD_SSHD=127.0.0.1:2223 \
TERMINUS_E2E_SLOW_SSHD=127.0.0.1:2224 TERMINUS_E2E_KEY=/tmp/e2e_key \
  cargo test -p terminus-bridge --test real_sshd_e2e -- --include-ignored
TERMINUS_E2E_ENC_KEY=/path/to/passphrase-protected-key \
  cargo test -p terminus-core --test e2e_data -- --include-ignored
```
