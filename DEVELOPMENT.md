# Developing Terminus

## The dev loop

```sh
make dev-hot          # watch sources, rebuild, restart the app
make dev-hot-once     # build and launch once
scripts/dev.sh --no-run          # compile-check on every save, no window
scripts/dev.sh --features wgpu   # opt into the wgpu renderer
scripts/dev.sh -- --working-dir /tmp   # pass flags through to `rio`
```

`scripts/dev.sh` re-execs itself inside the project's nix devshell, watches the
Rust sources, rebuilds `rioterm` on save, restarts the app and streams both
logs:

| what | where |
| --- | --- |
| app stdout/stderr | `.dev/logs/run.log` |
| build diagnostics | `.dev/logs/build.log` |
| config the app is using | `.dev/config/config.toml` |

`.dev/` is generated and gitignored. Delete it to reset the loop.

## What is actually hot

**Rust code is not.** Nothing can swap the code out of a running binary safely,
so a save triggers an incremental rebuild plus a process restart. The loop is
tuned so that restart is short:

* dependencies are compiled once at `opt-level = 2` and then never rebuilt
  (`[profile.dev.package."*"]` in `Cargo.toml`) — an unoptimized wgpu/Sugarloaf
  debug build is slow enough at runtime to be useless for judging UI work;
* workspace crates stay at `opt-level = 0`, so the crate you are editing is
  still fast to recompile;
* `debug = 1` keeps line tables for readable backtraces while cutting the debug
  info that dominates link time.

The first build after these settings is long (it rebuilds the dependency tree
once). Every build after that is incremental — measured on this machine, a save
in a frontend source file costs 2–3 s end to end (rebuild + relink + restart),
and a no-op build is 0.24 s. No exotic linker is needed at that point: `mold`
does not pay for itself here, and its use would fork the incremental cache.

**Config and colours are genuinely live.** The app watches its config directory
and re-applies the file on save, with no restart:

```sh
$EDITOR .dev/config/config.toml   # colours, padding, font, window options
```

That is the loop to use while tuning margins, paddings, control heights and
palettes. Theme files live in `.dev/config/themes/`; the directory watch is
non-recursive, so a save inside `themes/` is not seen on its own — touch
`config.toml` afterwards (or keep the colours inline in `config.toml`, which
reloads directly) to pull the new theme in.

## Why the nix devshell is mandatory

A plain shell on this machine exports `PKG_CONFIG_PATH=/usr/lib/pkgconfig`,
which contains no `fontconfig.pc`. `yeslogic-fontconfig-sys` panics in its
build script, so `cargo build -p rioterm` cannot succeed outside the devshell —
`make dev` and `make dev-watch` fail there too. `nix develop` supplies the
`PKG_CONFIG_PATH` and `LD_LIBRARY_PATH` the build scripts and the runtime need.

Pass `TERMINUS_DEV_NO_NIX=1` to skip the devshell if you are already inside one
(`nix develop`, direnv) or have the equivalent environment.

## Windows build

The desktop build is cross-compiled with `cargo-xwin`:

```sh
make dev-hot-win                                   # watch, build, deploy the exe
scripts/dev-win.sh --once                          # build and deploy once
TERMINUS_DEV_WIN_DEPLOY_DIR=/mnt/c/somewhere scripts/dev-win.sh
```

It builds `target/x86_64-pc-windows-msvc/debug/rio.exe` (with `--features wgpu`,
which Sugarloaf requires on Windows) and copies it to `rio-dev.exe` on the
Windows desktop. Windows interop is broken in this WSL setup — `powershell.exe`
and `cmd.exe` fail with `cannot execute binary file` — so the script cannot
restart the app for you; launch `rio-dev.exe` from Windows after a deploy.

## Known environment caveats

* **Software rendering under WSLg.** No Vulkan ICD is installed, so the script
  points the app at mesa's lavapipe (CPU) device when nothing else provides one.
  It renders, it is just slow — fine for layout and logic, not for judging
  animation smoothness or GPU behaviour. Set `TERMINUS_DEV_VULKAN_ICD=0` to
  disable.
* **Toolchain.** `rust-toolchain.toml` pins 1.96.1, which is what the nix
  devshell provides; your rustup `stable` is 1.98.1, so artifacts built with one
  do not feed the other. Use the devshell for the loop.
* **`make dev` / `make dev-watch`** are upstream Rio targets: they assume macOS
  (`MTL_HUD_ENABLED`) and a shell where fontconfig's pkg-config file resolves.

## Watching compile errors without a window

`scripts/dev.sh --no-run` rebuilds on every save and keeps the previous app
process running, so you can sit in one terminal and see `cargo` diagnostics.

Upstream's `bacon.toml` / `cargo-watch` recipes are not wired up here: both
would need the devshell environment that `scripts/dev.sh` establishes, and
neither deploys or restarts the app.
