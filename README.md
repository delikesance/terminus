# Terminus

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](./CONTRIBUTING.md)

Open-source [Termius](https://termius.com) alternative: a fast, highly customizable terminal emulator with SSH, SFTP, snippets, command history, and optional SQL-backed sync.

**Contributions are welcome** — see [CONTRIBUTING.md](./CONTRIBUTING.md). MIT licensed; you can use, fork, and ship changes freely.

![Terminus preview](./docs/media/hero-preview.png)

<p align="center">
  <img src="./docs/media/app-main.png" alt="Terminus — SSH session with hosts sidebar and terminal" width="860" />
</p>

<p align="center">
  <img src="./docs/media/app-hosts.png" alt="Terminus — dual-pane SFTP between two remotes" width="860" />
</p>

Stack: **Rust**, **Tauri 2**, **xterm.js (WebGL)**, **SQLite** locally, **PostgreSQL** (or any sqlx URL) for cloud sync.

## Features

- Local PTY and SSH sessions with tabs / tiled panes
- Host inventory, groups, identities, snippets, searchable history
- SFTP listing and local port forwarding
- Themes, fonts, renderer, padding, opacity, custom CSS, keybindings
- Settings → database URL for PostgreSQL sync (hosts, history, snippets, forwards)
- Secrets stay local unless you enable **encrypted vault sync** (Argon2id + XChaCha20-Poly1305). The database never sees plaintext SSH keys or passwords.
- Kerberos / GSSAPI (`gssapi-with-mic`) on Linux and macOS using the ticket cache from `kinit` — no Kerberos password stored in Terminus.

## Contributing

Anyone can contribute — bug fixes, UX polish, docs, and tests all help.

1. Read [CONTRIBUTING.md](./CONTRIBUTING.md) for setup and PR expectations.
2. Open an [issue](https://github.com/delikesance/terminus/issues) if you want design feedback first (optional).
3. Send a pull request against `main`.

By contributing you agree your work is released under the [MIT License](./LICENSE).

### Marketing / Canva preview

Product screenshots live in [`docs/media/`](./docs/media/). To refresh them:

```bash
VITE_E2E=1 nix develop -c npm run build
nix develop -c node scripts/capture-preview.mjs
```

For a social/GitHub hero in Canva, follow [`docs/media/CANVA_BRIEF.md`](./docs/media/CANVA_BRIEF.md) (size, colors, copy, import steps).

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

SFTP path helpers:

```bash
npm run test:sftp-path
```

`terminus-selftest` is the proof harness: it opens a local PTY, talks to the compose SSH server, writes SQLite state, and round-trips it through PostgreSQL.

## Cross-platform builds

GitHub Actions builds on Ubuntu, macOS, and Windows. Pushing to `main` publishes a GitHub Release for all platforms and writes `latest.json` for the in-app updater.

Locally, build on the target OS:

```bash
npm run tauri -- build
```

The desktop app checks GitHub Releases on launch when online and installs a newer version automatically.

CI caches the Cargo registry and compiled crates (`Swatinem/rust-cache` + `sccache`) so later runs skip re-downloading and recompiling russh/tauri/sqlx. The first run after a `Cargo.lock` change is still cold.

Updater signing uses GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty is fine). The matching public key lives in `src-tauri/tauri.conf.json`. That key only authenticates in-app updates; it does **not** Authenticode-sign the Windows installer. SmartScreen will keep saying the `.exe` is unrecognized until you add an OV/EV code-signing certificate as secrets `WINDOWS_CERTIFICATE` (base64-encoded `.pfx`) and `WINDOWS_CERTIFICATE_PASSWORD`. The Windows release job imports it and Tauri signs the NSIS setup with `signtool`. A self-signed cert will not clear SmartScreen.

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

## License

[MIT](./LICENSE) © Terminus Contributors
