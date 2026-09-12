/**
 * #141 / #143 — paste must reach the PTY once per gesture.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  PASTE_SUPPRESS_MS,
  MODE_BRACKETED_PASTE,
  isBracketedPaste,
  formatTerminalPaste,
  nextPasteSuppressUntil,
  shouldIgnorePaste,
  decideDomPasteAction,
  claimPasteDelivery,
} from "./termPaste.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const mainTs = fs.readFileSync(path.join(root, "src/main.ts"), "utf8");

assert.ok(PASTE_SUPPRESS_MS >= 50 && PASTE_SUPPRESS_MS <= 250);

// AC1 — keydown paste then immediate DOM paste → one delivery
{
  const t0 = 1_000_000;
  let suppressUntil = 0;
  let sends = 0;
  const claimed = claimPasteDelivery(suppressUntil, t0);
  assert.ok(claimed !== null);
  suppressUntil = claimed;
  sends += 1;
  const dom = decideDomPasteAction({
    suppressUntil,
    now: t0 + 1,
    clipboardText: "git@github.com:ketsuna-org/bot-creator.git",
  });
  if (dom !== "ignore") sends += 1;
  assert.equal(dom, "ignore");
  assert.equal(sends, 1, "DOM paste must not double-insert after Ctrl+V");
}

// AC1b — keybinding (Ctrl+Shift+V) then pane keydown → one delivery
{
  const t0 = 2_000_000;
  let suppressUntil = 0;
  let sends = 0;
  const viaBinding = claimPasteDelivery(suppressUntil, t0);
  assert.ok(viaBinding !== null);
  suppressUntil = viaBinding;
  sends += 1;
  const viaPane = claimPasteDelivery(suppressUntil, t0 + 1);
  assert.equal(viaPane, null);
  if (viaPane !== null) sends += 1;
  assert.equal(sends, 1, "Ctrl+Shift+V must not paste via keybinding AND pane handler");
}

// AC2 — DOM paste without recent delivery still sends clipboardData
{
  const action = decideDomPasteAction({
    suppressUntil: 0,
    now: 5_000,
    clipboardText: "hello",
  });
  assert.deepEqual(action, { send: "hello" });
}

// AC2b — empty clipboardData falls back to Tauri path
{
  assert.equal(
    decideDomPasteAction({ suppressUntil: 0, now: 5_000, clipboardText: "" }),
    "fallback",
  );
}

// DOM first then keydown twin
{
  const t0 = 3_000_000;
  let suppressUntil = 0;
  let sends = 0;
  const dom = decideDomPasteAction({
    suppressUntil,
    now: t0,
    clipboardText: "once",
  });
  assert.deepEqual(dom, { send: "once" });
  sends += 1;
  suppressUntil = nextPasteSuppressUntil(t0);
  const viaKey = claimPasteDelivery(suppressUntil, t0 + 1);
  assert.equal(viaKey, null);
  if (viaKey !== null) sends += 1;
  assert.equal(sends, 1);
}

// Guard expires
{
  const until = nextPasteSuppressUntil(1000);
  assert.equal(shouldIgnorePaste(until, 1000 + PASTE_SUPPRESS_MS - 1), true);
  assert.equal(shouldIgnorePaste(until, 1000 + PASTE_SUPPRESS_MS), false);
}

// Bracketed paste detection
assert.equal(isBracketedPaste(0), false);
assert.equal(isBracketedPaste(MODE_BRACKETED_PASTE), true);
assert.equal(isBracketedPaste(0b1111), true);
assert.equal(isBracketedPaste(undefined), false);

// Paste formatting — bracketed paste inactive
assert.equal(formatTerminalPaste("", 0), "");
assert.equal(formatTerminalPaste("git status", 0), "git status");
assert.equal(formatTerminalPaste("git status\n", 0), "git status", "strips trailing newline to prevent auto-execution");
assert.equal(formatTerminalPaste("git status\r\n", 0), "git status");
assert.equal(formatTerminalPaste("echo a\necho b\n", 0), "echo a\recho b");

// Paste formatting — bracketed paste active (DECSET 2004)
assert.equal(
  formatTerminalPaste("git status", MODE_BRACKETED_PASTE),
  "\x1b[200~git status\x1b[201~",
);
assert.equal(
  formatTerminalPaste("git status\n", MODE_BRACKETED_PASTE),
  "\x1b[200~git status\x1b[201~",
  "strips trailing newline to cleanly place cursor at line end",
);
assert.equal(
  formatTerminalPaste("echo 1\r\necho 2\r\n", MODE_BRACKETED_PASTE),
  "\x1b[200~echo 1\recho 2\x1b[201~",
  "normalizes CRLF to single CR within bracketed paste",
);
assert.equal(
  formatTerminalPaste("line1\n\nline2\n", MODE_BRACKETED_PASTE),
  "\x1b[200~line1\r\rline2\x1b[201~",
  "preserves blank lines within multiline paste",
);
assert.equal(
  formatTerminalPaste("\x1b[201~malicious\r\x1b[200~", MODE_BRACKETED_PASTE),
  "\x1b[200~malicious\x1b[201~",
  "sanitizes bracketed paste injection tokens",
);
assert.equal(
  formatTerminalPaste("keep\n", MODE_BRACKETED_PASTE, { stripTrailingNewline: false }),
  "\x1b[200~keep\r\x1b[201~",
  "preserves trailing newline when explicitly requested",
);

// Wiring
assert.match(mainTs, /from ["']\.\/termPaste/);
assert.match(mainTs, /claimPasteDelivery/);
assert.match(mainTs, /decideDomPasteAction/);
assert.match(mainTs, /formatTerminalPaste/);
assert.match(mainTs, /formatTerminalPaste\(text, pane\.modeFlags\)/);
assert.match(mainTs, /case "terminal\.paste"/);

console.log("termPaste.test.js: ok");
