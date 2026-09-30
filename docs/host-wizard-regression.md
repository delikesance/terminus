# Remote-host wizard regression verification

Verified on 2026-09-30: 216 UI tests, six renderer tests, and all 128 real-window
checks across 22 screenshot states passed. The final application build and
`git diff --check` passed. Final captures were also visually inspected.

The wizard keeps focused placeholders visible, paints active step labels over
an opaque accent wash, and reserves a separate two-line validation area above
the actions. Edit mode advances with Next and offers Save on Details.

Run the unit tests and build:

```sh
nix develop --command cargo test -p terminus-ui
nix develop --command cargo test -p rioterm --bin terminus renderer::chrome::tests
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
Next/Save labels. A long invalid port verifies two rendered error lines.

Artifacts are in `.dev/shots/host-wizard/`: full-window screenshots, cropped
dialog screenshots, `app.log`, and `results.json`. Keep the screenshots for
visual inspection alongside the automated results. These captures prove the
Linux Vulkan path with software rendering; they do not establish Windows or
macOS E2E coverage.

For the saved pre-fix captures from this investigation,
`python3 scripts/test-host-wizard.py --baseline` checks the same rendering
requirements and must fail. The layout unit regression also failed first with
`Target/key: error overlaps actions` before the layout implementation changed.
