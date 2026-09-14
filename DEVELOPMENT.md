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

## Where the Terminus front end lives

Rio is upstream code. Everything Terminus adds to the UI is layered on top of
it, and the layering is deliberate: the geometry is separable from the drawing
so the mouse and the painter can never disagree.

| piece | file | job |
| --- | --- | --- |
| layout + state + hit-testing | `crates/terminus-ui/src/` | where every chrome box is, what it means, what a click at (x, y) hits |
| painting | `frontends/rioterm/src/renderer/chrome.rs` | turns those boxes into sugarloaf primitives |
| host storage | `frontends/rioterm/src/hosts.rs` | SQLite behind a worker thread, so the UI thread never awaits |
| wiring | `frontends/rioterm/src/screen/mod.rs`, `application.rs`, `router/mod.rs` | input routing, and reserving the chrome's strip in the grid margin |

Rules that hold this together:

* **Geometry is computed once, in `terminus-ui`.** The painter walks the same
  `*_rect` functions the hit-test does, so a control cannot be drawn somewhere
  it cannot be clicked. Widths live there too (`activity_bar::WIDTH`,
  `sidebar::WIDTH`) and the grid margin is derived from them, not duplicated.
* **The chrome does not paint itself.** `terminus-ui` has no sugarloaf
  dependency; it returns rectangles and colors, and the front end draws them.
  That keeps it testable without a GPU — `cargo test -p terminus-ui` covers the
  layout, the hit-tests, the scroll maths and the form's editing behaviour.
* **Sugarloaf has no SVG renderer** (`components::svg` is commented out), so
  icons are Lucide path data flattened into `line` primitives at build time:
  `crates/terminus-ui/src/icons.rs` is generated, one `fn` per icon, do not
  hand-edit it.
* **The chrome reserves space in the grid margin** rather than painting over
  the terminal, so the terminal reflows beside it. `reapply_chrome_inset`
  re-runs on any change to the reserved width, including a config hot-reload.

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

## Seeing the UI without a desktop

`scripts/screenshot.sh` runs the app against Xvfb (a virtual X server with a real
framebuffer), reads that framebuffer back with `xwd`, and writes a PNG — so the
front end can be developed and checked with no visible display.

```sh
scripts/screenshot.sh                                  # capture the current UI
scripts/screenshot.sh --size 1200x760                  # pick the viewport
scripts/screenshot.sh --type 'ls -la'                  # type into the terminal, then Enter
scripts/screenshot.sh --send 'ctrl+shift+e'            # press keys first
scripts/screenshot.sh --hot-config 'margin = [60, 60, 60, 60]'
```

Input steps replay **in the order you write them**, which is what makes a
dialog drivable: a `--click` that opens the editor has to precede the `--text`
that fills it.

```sh
# Add a host through the UI: open the editor, fill four fields, save.
scripts/screenshot.sh --out /tmp/added.png \
  --click 168,781 \
  --text web-01 \
  --send Tab --text web-01.example.com \
  --send Tab --text deploy \
  --send Tab --text 2222 \
  --send Return
```

* `--click X,Y` / `--move X,Y` — click, or just move the pointer to capture a
  hover state. Coordinates are **window-relative**.
* `--text TEXT` types without Enter (to fill a form field); `--type TEXT` types
  and presses Enter (to run a shell command).
* `--send KEYS` presses one xdotool key spec (`Tab`, `Escape`, `ctrl+shift+e`).
* Set `TERMINUS_DATA_DIR=/tmp/whatever` to point the host database somewhere
  disposable, so a verification run cannot touch your real one.

To check the result rather than the picture, read the database the app wrote:

```sh
sqlite3 /tmp/whatever/terminus.db 'select name, hostname, port from hosts'
```

`--hot-config` captures, applies the assignment to `.dev/config/config.toml`,
waits, captures again and reports the changed-pixel count — a pass/fail signal
for config reloading that does not depend on reading a log.

Four things to know:

* **`WAYLAND_DISPLAY` must be cleared** or the app connects to WSLg's Wayland
  compositor instead of Xvfb. The X display then stays empty and every capture is
  solid black, with no error anywhere — the script does this for you.
* **Input has to go through XTEST.** `xdotool key --window …` uses XSendEvent,
  which the winit backend drops silently; the script focuses the window
  (`XSetInputFocus`) and lets XTEST deliver the keys.
* **A UI-only change needs `Route::request_overlay_redraw`, not
  `request_redraw`.** Rio renders only when the active context is dirty, so
  `request_redraw` alone re-presents a stale framebuffer: the state changes and
  the capture shows the old frame. Anything that moves terminal cells
  (a margin change, a re-layout) marks itself dirty and can use either.
* **No window manager runs here**, so the script resizes the window itself with
  `xdotool windowsize`. Window decorations, drag, and multi-window behaviour
  cannot be exercised this way, and rendering is software (lavapipe): good enough
  to judge layout and logic, not animation or GPU performance. For how it really
  looks, run `scripts/dev-win.sh` and launch the exe on Windows.
