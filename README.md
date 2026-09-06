# Terminus

Open-source [Termius](https://termius.com) alternative: a fast, highly customizable terminal emulator with SSH, SFTP, snippets, command history, and optional SQL-backed sync.

Stack: **Rust**, **Tauri 2**, **xterm.js (WebGL)**, **SQLite** locally, **PostgreSQL** (or any sqlx URL) for cloud sync.

## Features

- Local PTY and SSH sessions with tabs / tiled panes
- Host inventory, groups, identities, snippets, searchable history
- SFTP listing and local port forwarding
- Themes, fonts, renderer, padding, opacity, custom CSS, keybindings
- Settings → database URL for PostgreSQL sync (hosts, history, snippets, forwards)
- Secrets stay local unless you opt in to secret sync

## Development (Nix flake)

```bash
nix develop
npm install
python3 scripts/gen-icons.py
docker compose up -d --build
cargo run -p terminus-core --bin terminus-selftest
npm run tauri -- dev
```

Parse unit check for known_hosts import:

```bash
npm run test:known-hosts
```

Identity kind helpers:

```bash
npm run test:identity-kind
```

`terminus-selftest` is the proof harness: it opens a local PTY, talks to the compose SSH server, writes SQLite state, and round-trips it through PostgreSQL.

## Cross-platform builds

GitHub Actions builds on Ubuntu, macOS, and Windows. Locally, build on the target OS:

```bash
npm run tauri -- build
```

The Rust core also lists GNU/Windows and Darwin targets in the flake toolchain for library-level cross compilation.

## Sync URL examples

```
postgres://user:pass@db.example:5432/terminus
```

Default local fixtures (docker compose):

```
postgres://terminus:terminus@127.0.0.1:54329/terminus
ssh terminus@127.0.0.1 -p 2222   # password: terminus
```

SSH host keys use fail-closed verification against `~/.ssh/known_hosts` (override with `TERMINUS_KNOWN_HOSTS`). First connect shows a TOFU dialog; Trust appends the presented key atomically via `ssh_host_key_trust`.