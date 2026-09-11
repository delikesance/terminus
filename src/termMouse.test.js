/**
 * #148 — terminal mouse CSI encoding (SGR + legacy).
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  MODE_MOUSE,
  MODE_SGR_MOUSE,
  MODE_MOUSE_DRAG,
  mouseReportingEnabled,
  encodeTermMouse,
  xtermButtonCode,
} from "./termMouse.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const mainTs = fs.readFileSync(path.join(root, "src/main.ts"), "utf8");
const termRs = fs.readFileSync(path.join(root, "crates/terminus-core/src/term.rs"), "utf8");

assert.equal(mouseReportingEnabled(0), false);
assert.equal(mouseReportingEnabled(MODE_MOUSE), true);

// AC2 — SGR press/release
{
  const flags = MODE_MOUSE | MODE_SGR_MOUSE;
  const press = encodeTermMouse({
    modeFlags: flags,
    col: 2,
    row: 4,
    button: xtermButtonCode(0),
    action: "press",
  });
  assert.equal(press, "\x1b[<0;3;5M");
  const release = encodeTermMouse({
    modeFlags: flags,
    col: 2,
    row: 4,
    button: xtermButtonCode(0),
    action: "release",
  });
  assert.equal(release, "\x1b[<0;3;5m");
}

// Motion only when drag bit set
{
  const clickOnly = MODE_MOUSE | MODE_SGR_MOUSE;
  assert.equal(
    encodeTermMouse({
      modeFlags: clickOnly,
      col: 0,
      row: 0,
      button: 0,
      action: "motion",
    }),
    null,
  );
  const drag = clickOnly | MODE_MOUSE_DRAG;
  assert.equal(
    encodeTermMouse({
      modeFlags: drag,
      col: 0,
      row: 0,
      button: 0,
      action: "motion",
    }),
    "\x1b[<32;1;1M",
  );
}

// Legacy press
{
  const legacy = encodeTermMouse({
    modeFlags: MODE_MOUSE,
    col: 0,
    row: 0,
    button: 0,
    action: "press",
  });
  assert.equal(legacy, `\x1b[M${String.fromCharCode(32)}${String.fromCharCode(33)}${String.fromCharCode(33)}`);
}

// Wheel
{
  const flags = MODE_MOUSE | MODE_SGR_MOUSE;
  assert.equal(
    encodeTermMouse({
      modeFlags: flags,
      col: 1,
      row: 1,
      button: xtermButtonCode(0, "up"),
      action: "press",
    }),
    "\x1b[<64;2;2M",
  );
}

// Wiring
assert.match(termRs, /MOUSE_MODE|MOUSE_REPORT_CLICK/);
assert.match(termRs, /SGR_MOUSE/);
assert.match(mainTs, /from ["']\.\/termMouse/);
assert.match(mainTs, /encodeTermMouse/);
assert.match(mainTs, /mouseReportingEnabled/);

console.log("termMouse.test.js: ok");
