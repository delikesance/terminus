# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Terminus is a fork of the Rio terminal (`upstream` = raphamorim/rio) that adds an
SSH/SFTP/WSL host manager and its own UI chrome on top. Rio's crates (`sugarloaf`,
`rio-backend`, `rio-window`, `teletypewriter`, …) are upstream code; Terminus work
lives mostly in `crates/terminus-*` and in `frontends/rioterm`. `AGENTS.md`,
`DEVELOPMENT.md` and `milestone.md` (French roadmap) hold the longer versions of
what follows.

## Workflow rules (from AGENTS.md / .cursor/rules)

- Day-to-day work goes on `development`, never directly on `main`. `main` only
  moves through a reviewed PR `development` → `main` (review `git diff main...development`
  first). Don't revive long-lived branches like `feat/rio-integration`.
- TDD is mandatory for behavior changes: write the failing test, run it to see it
  fail for the right reason, then implement. Only pure docs/chores skip this.
  Prefer unit tests next to the code (e.g. in `terminus-ui`) before wiring the
  painter/frontend.

## Build & test

The nix devshell is required to build `rioterm`: outside it, fontconfig's
pkg-config file doesn't resolve and `yeslogic-fontconfig-sys` panics. Run commands
inside `nix develop` (or direnv via `.envrc`). Toolchain is pinned to 1.96.1 in
`rust-toolchain.toml` (the devshell provides it; host rustup stable differs, so
don't mix artifacts).

```sh
make dev-hot                         # watch + rebuild + restart app (Linux/WSLg), config in .dev/config
scripts/dev.sh --no-run              # compile-check on every save, no window
make dev-hot-win                     # cross-build for Windows (cargo-xwin, nix develop .#windows)
cargo test -p terminus-ui            # chrome layout / hit-test / form tests, no GPU needed
cargo test -p terminus-core          # also: terminus-bridge, terminus-update, terminus-walk
cargo test -p terminus-ui sidebar::  # filter to one module / test name
cargo clippy -p rioterm
cargo fmt                            # rustfmt.toml: max_width = 90
```

`make dev` / `make dev-watch` are upstream macOS-oriented targets; prefer `dev-hot`.

Headless UI verification: `scripts/screenshot.sh` runs the app under Xvfb and
writes a PNG, replaying `--click X,Y`, `--text`, `--type`, `--send KEYS` in order.
Use `TERMINUS_DATA_DIR=/tmp/x` so it doesn't touch the real host database.
Rendering in WSL is lavapipe (CPU) — fine for layout/logic, not for perf.

Config/colour changes hot-reload live (edit `.dev/config/config.toml`); Rust
changes always need rebuild + restart.

## Architecture

Workspace crates added by Terminus:

- `crates/terminus-core` — pure Tokio domain: SQLite `Store`, `Vault`, sync,
  russh SSH/SFTP, port-forward runtime, WSL discovery (`wsl.rs`), machine/OS detect.
- `crates/terminus-bridge` — shared workers/transports: `sftp_worker`, and
  `SshTransport` (ready but **not wired**, see below). Its facade trait is a stub.
- `crates/terminus-ui` — chrome state, geometry and hit-testing. **No sugarloaf/GPU
  dependency**; it returns rects and colors only.
- `crates/terminus-update` — signed self-updater (minisign + SHA-256 over GitHub releases).
- `crates/terminus-walk` — parallel dir walk/hash helper for SFTP sync.
- `frontends/rioterm` — the app binary (`rio`): paints the chrome and wires input.

Key layering in `frontends/rioterm/src`:

- `renderer/chrome.rs` is the painter: it walks the same `*_rect` functions that
  `terminus-ui` uses for hit-testing and emits sugarloaf primitives, so a control
  can't be drawn where it can't be clicked. Never compute chrome geometry in the
  frontend; add it to `terminus-ui` (widths like `activity_bar::WIDTH`,
  `sidebar::WIDTH` live there and the grid margin is derived from them).
- The chrome reserves space in the grid margin rather than overlaying the
  terminal; `reapply_chrome_inset` must re-run whenever the reserved width changes.
- `screen/` (`chrome_input.rs`, `sessions.rs`, `shell.rs`, `sftp.rs`, …),
  `application.rs` and `router/mod.rs` handle input routing and session wiring.
- `hosts.rs` runs SQLite on a worker thread so the UI thread never blocks;
  `hosts::sidebar_rows` builds the sidebar. The UI only knows row ids (`local`,
  `wsl:<distro>`, or a host id); `Screen::open_host_session` maps an id back to a `Shell`.
- A UI-only state change must call `Route::request_overlay_redraw`, not
  `request_redraw` — Rio only re-renders when the terminal context is dirty, so
  the latter shows a stale frame.

SSH: interactive host tabs use a local PTY running system `ssh`
(`screen/shell.rs` → `ssh_shell`, `StrictHostKeyChecking=accept-new`), while SFTP
uses russh via `terminus-bridge::sftp_worker`. This split is an accepted MVP
decision (debt "1.4-debt" in `milestone.md`); don't switch tabs to `SshTransport`
without updating the roadmap and TOFU/auth UX.

Icons: sugarloaf has no SVG renderer. `crates/terminus-ui/src/icons.rs` holds
Lucide path data and is generated by `scripts/gen-lucide-icons.py --write` — don't
hand-edit it. The frontend rasterizes paths with `tiny-skia` into coverage masks.

## Releases

Version comes from `[workspace.package] version` in `Cargo.toml`; the git tag must
be exactly `v<version>` (the updater compares them). Prepare with
`misc/prepare-release.sh X.Y.Z`, publish with `nix run .#release`
(`-- --build-only` to skip publishing). See DEVELOPMENT.md for signing and per-platform update behavior.
