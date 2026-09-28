# E2E bug report — 2026-09-28

Features were exercised end to end: against a live OpenSSH server, through the
store/vault/sync APIs, and in the real `rio` binary driven with xdotool under
Xvfb. **Every finding below is fixed** on this branch. Each fix has a test
that failed first, and the GUI flows were re-checked in the running app.

Suites:

* `crates/terminus-core/tests/e2e_data.rs`: store, vault, sync (two-device
  convergence, tombstones, secrets gating) and managed keys.
* `crates/terminus-bridge/tests/real_sshd_e2e.rs`: SSH/SFTP against a real
  sshd. It is skipped unless `TERMINUS_E2E_SSHD` is set (setup at the end).

## Findings and fixes

| # | Feature | Bug | Fix | Commit |
|---|---|---|---|---|
| 1 | Remote SQL Sync | Push/pull were stubs that reported "Sync ok" and wrote nothing. | Real last-writer-wins merge on `updated_at` in both directions. Tombstones propagate. It runs on startup, on a timer and from Test Sync. | `9d08f40` |
| 2 | SSH shell | The decrypted password was written world-readable to `/tmp/terminus-ssh-askpass`, and could be left there forever. Any user could also pre-plant `askpass.sh`. | A per-user 0700 directory (owner checked, no symlinks) holds 0600 secrets created with `create_new`. The helper is always rewritten. Stale and legacy secrets are swept. | `6419f23` |
| 3 | SFTP transfer | Uploads and downloads held the whole file in RAM under one 30 s timeout. A failure left a truncated file. | Transfers stream in 256 KiB chunks with a per-chunk timeout, into a temp sibling that replaces the target only when complete. Permissions are kept; a symlinked target or a directory with no room for a temp file is written in place. | `3e29ab9` |
| 4 | SFTP browse | Symlinked directories were listed as files, and a double-click started downloading them. This hit both the remote and the local side. | Listings resolve symlinks (remote `stat`, local `fs::metadata`). | `3e29ab9` |
| 5 | SFTP transfer | A single-file transfer silently overwrote an existing file (and a local file could be copied onto itself). | It now asks Replace/Keep before overwriting. Copying a file onto itself is refused. | `3e29ab9` |
| 6 | Managed keys | Private keys were stored as plaintext PEM, and `terminus.db` was created 0644. | Keys and their passphrases are vault-sealed. Legacy rows are sealed on unlock. The data dir is 0700 and the DB 0600. | `121e5d7`, `840611c` |
| 7 | Auth | Password hosts failed on keyboard-interactive-only servers. | russh falls back to keyboard-interactive. The CLI now allows `password,keyboard-interactive`. | `6419f23` |
| 8 | Host keys | A known ECDSA key triggered a false "host key changed" warning. | The host-key algorithms already recorded for the host are offered first, as OpenSSH does. | `6419f23` |
| 9 | Host keys | Hashed known_hosts entries were ignored. | `\|1\|` entries are matched with HMAC-SHA1, and `@marker` lines are skipped. | `6419f23` |
| 10 | SFTP paths | Names with trailing spaces or backslashes could not be opened. | Names are no longer trimmed. `\` is a separator only in Windows-style paths, and `..` hidden behind backslashes is still rejected. | `3e29ab9` |
| 11 | Store | One malformed row panicked the host list. | Bad rows are skipped with a warning, and SQLite `datetime()` timestamps are accepted. | `840611c` |
| 12 | Vault | A short nonce panicked `decrypt`. | The nonce length is checked, returning `VaultDecryptFailed`. | `840611c` |
| 13 | Delete host | No confirmation, the credential was left behind, `updated_at` was not bumped, and the host's session became orphaned. | Delete takes two clicks (the label says how many sessions will close). Its tabs close and the credential is retired. The tombstone bumps `updated_at`. | `840611c`, `ddda3e4` |
| 14 | Import key | Encrypted keys could not be imported. | Settings has a passphrase field, and the errors say what is wrong. | `121e5d7` |
| 15 | Palette | Open SFTP always picked the first host. | It opens the only host, or shows a host picker. | `ddda3e4` |
| 16 | Title bar | It showed the host UUID. | It shows `user@host:port`. | `ddda3e4` |
| 17 | Rendering | Chrome labels and icons vanished or were half-drawn next to SFTP and under dialogs. | Sugarloaf's Vulkan text pass uploaded its early, late and overlay lists all at offset 0, so recorded draws read the wrong glyphs. The lists now use disjoint offsets. | `ddda3e4` |
| 18 | SFTP header | Long paths overflowed into the other pane. | The path keeps its tail (`…/dir`). Row names and the footer are elided too. | `ddda3e4` |
| 19 | SFTP start | The remote pane opened at `/`. | It opens in the login directory. | `3e29ab9` |
| 20 | Vault | A vault was created from an "Unlock" prompt with no confirm field. | A "Create Vault" prompt asks for the passphrase twice (minimum 8 characters). The Settings path routes there too. | `ddda3e4` |
| 21 | Dialogs | Vault subtitle and add-host errors overflowed or were cut. | Both wrap to two lines. | `ddda3e4` |
| 22 | Vault ids | `credential_id` used `DefaultHasher`. | UUIDv5. One live credential per owner and kind, so rows under the legacy id never shadow new ones. | `840611c` |

Found and fixed while fixing the above:

* The SFTP conflict dialog drew under the file list. It now uses the overlay
  layer, and its message no longer assumes a download.
* SFTP on a locked vault did nothing visible. It now opens the unlock modal
  and retries (this covers keys as well as passwords).
* Reordering hosts or groups and deleting a snippet did not bump
  `updated_at`, so those changes would not have synced.

## Usability pass

A second pass walked first launch, adding hosts, connecting, keys, the
palette and SFTP in the running app, and removed the friction it found. Each
change has a test that failed first and was re-checked in the GUI.

| Area | Before | After | Commit |
| --- | --- | --- | --- |
| First launch | Rio's "press enter" screen with a raw config path. | Writes the default config and opens the app. | `2ef945c` |
| Add host | `user@host:port` had to be split by hand; with no saved key the form started on a key picker that could not be used; "Connect" only saved. | The address field splits `user@host:port`, `ssh -p N user@host` and `ssh://…`; no saved key means password auth; the name is marked optional; "Connect" opens a session on the new host. | `29e2f60`, `778d311` |
| Add host | Two hosts with the same (default) name: the first one was selected. | The saved host is found by name and endpoint. | `29e2f60`, `778d311` |
| Errors | "Host unreachable: Connection refused (os error 111)", DNS errors cut mid-sentence. | "Nothing answers on that port. Is SSH running?", "Can't find that host…", and so on. | `d9f78a2` |
| Rail | Four unlabelled icons; the sync icon was always green. | Hover labels; sync is green only when connected ("Cloud Sync: not set up" otherwise). | `e22bb9a`, `7d8dbaf` |
| Host list | A blank panel with no hosts or no filter match; Esc left the filter applied. | "No saved hosts yet" / "No matches" with what to do; Esc clears the filter first. | `0d09512` |
| Palette | macOS shortcuts (Cmd+T…) shown on Linux and Windows. | Labels read from the live key bindings (Ctrl+Shift+T…), remaps included. | `755e486` |
| Keys | A generated key's public half was nowhere to be found; import needed pasted PEM. | "Copy public key" on each key; import takes a path such as `~/.ssh/id_ed25519`. | `b213951`, `778d311` |
| Host menu | No way to connect from it. | New session first; Copy SSH command added. | `c641b52` |
| Rename | Typing appended to the old name. | The old name starts selected. | `15f9726` |
| SFTP | An unreachable host left an empty pane saying "Ready". | That pane says it could not connect, why, and how to retry. | `fd8fcee` |

## Known limits

* Secret sync (`sync_secrets`) is off by default and has no toggle in
  Settings yet. With it on, secrets only move between devices that share one
  vault; a mismatch is reported and hosts still sync.
* Only `sqlite:` remotes are supported for sync (PostgreSQL is still refused
  with a clear message).
* If the connection dies mid-upload, a hidden `.name.terminus-part-*` file
  can remain on the server. The original file is left intact.
* Keyboard-interactive answers password prompts only. One-time codes still
  need the shell tab.
* The Windows and macOS paths compile in CI but were not driven end to end
  here.

## Reproducing the real-sshd suite (Linux)

```sh
useradd -m tuser && echo 'tuser:hunter2' | chpasswd
ssh-keygen -t ed25519 -N '' -f /tmp/e2e_key   # add .pub to ~tuser/.ssh/authorized_keys
# sshd #1: port 2222, PasswordAuthentication yes, Subsystem sftp internal-sftp
# sshd #2: port 2223, PasswordAuthentication no, KbdInteractiveAuthentication yes
# throttled proxy 2224 → 2222 at ~1 MB/s (any TCP rate limiter)
TERMINUS_E2E_SSHD=127.0.0.1:2222 TERMINUS_E2E_KBD_SSHD=127.0.0.1:2223 \
TERMINUS_E2E_SLOW_SSHD=127.0.0.1:2224 TERMINUS_E2E_KEY=/tmp/e2e_key \
  cargo test -p terminus-bridge --test real_sshd_e2e
TERMINUS_E2E_ENC_KEY=/path/to/passphrase-protected-key \
  cargo test -p terminus-core --test e2e_data
```
