# Remote-host wizard regression verification

Verified on 2026-09-30: 222 UI tests, all 288 frontend tests (including six
renderer tests), and all 153 real-window checks across 26 screenshot states
passed. The final application build and
`git diff --check` passed. Final captures were also visually inspected.

The wizard keeps focused placeholders visible, paints active step labels over
an opaque accent wash. Its layout is composed from the reusable `Block` and
`TextBlock` components in `crates/terminus-ui/src/layout.rs`: padded containers,
columns, rows, and named components for the title, steps, captions, inputs,
validation, and actions. Edit mode advances with Next and offers Save on Details.

Every rendered frame measures validation text with the actual UI font, then
rebuilds the block layout using the current fields and viewport. The painter and
pointer handling use the same component rectangles and measured lines. An absent
notice consumes no space; a present notice grows with its wrapped content rather
than reserving a fixed height or truncating at two lines. Dialog width, field
widths, step pills, and the flexible space between actions adapt to the viewport.
The unit regression first failed on the old reserved notice block with
`an absent notice must not reserve a fixed block` before this change.

Run the unit tests and build:

```sh
nix develop --command cargo test -p terminus-ui
nix develop --command cargo test -p rioterm --bin terminus renderer::chrome::tests
nix develop --command cargo test -p rioterm --bin terminus
nix develop --command cargo build -p rioterm
```

The real-window regression uses Xvfb, XTEST keyboard/mouse events, the actual
Terminus binary, and ImageMagick framebuffer readback. Resolve the capture
tools with `scripts/screenshot.sh` first, and configure a compatible Vulkan
driver in `.dev/gpu-env.sh` when running without a hardware GPU. Then run:

```sh
nix develop --command bash -c \
  'source .dev/gpu-env.sh; python3 scripts/test-host-wizard.py'
```

The test uses a disposable configuration and database. It exercises all three
steps, password/key/GSSAPI authentication, cursor movement, masked and visible
passwords, backward navigation, retained text, error clearing, invalid ports,
and editing a database fixture through the host context menu. Invalid drafts
stop before any SSH probe. Pixel checks verify a visible wizard title and step
label, focused placeholders, and validation text outside the action buttons.
The edit footer is compared with the new-host footer to catch overlapping
Next/Save labels. A long invalid port verifies two rendered error lines, followed
by a live resize from 1000 to 320 pixels wide that adds a third line and grows
the dialogue. Back and Next are clicked at their recalculated positions in the
narrow window, then the window is widened after clearing the notice. The test
reads the dialog shell from the framebuffer rather than predicting its height.

Artifacts are in `.dev/shots/host-wizard/`: full-window screenshots, cropped
dialog screenshots, `app.log`, and `results.json` (including each frame's viewport,
dialog bounds, and footer position). Keep the screenshots for
visual inspection alongside the automated results. These captures prove the
Linux Vulkan path with software rendering; they do not establish Windows or
macOS E2E coverage.

For the saved pre-fix captures from this investigation,
`python3 scripts/test-host-wizard.py --baseline` checks the same rendering
requirements and must fail. The layout unit regression also failed first with
`Target/key: error overlaps actions` before the layout implementation changed.
