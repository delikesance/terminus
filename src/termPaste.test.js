/**
 * #141 — Ctrl+V must not double-insert when a DOM paste event follows keydown.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  PASTE_SUPPRESS_MS,
  nextPasteSuppressUntil,
  shouldIgnoreDomPaste,
  decideDomPasteAction,
} from "./termPaste.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const mainTs = fs.readFileSync(path.join(root, "src/main.ts"), "utf8");

assert.ok(PASTE_SUPPRESS_MS >= 50 && PASTE_SUPPRESS_MS <= 250);

// AC1 — keydown paste then immediate DOM paste → one delivery
{
  const t0 = 1_000_000;
  const suppressUntil = nextPasteSuppressUntil(t0);
  let sends = 0;
  // keydown path always sends once
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

// AC2 — DOM paste without recent keydown still sends clipboardData
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

// Guard expires
{
  const until = nextPasteSuppressUntil(1000);
  assert.equal(shouldIgnoreDomPaste(until, 1000 + PASTE_SUPPRESS_MS - 1), true);
  assert.equal(shouldIgnoreDomPaste(until, 1000 + PASTE_SUPPRESS_MS), false);
}

// Wiring: main must use the guard helpers / suppress window
assert.match(mainTs, /from ["']\.\/termPaste/);
assert.match(mainTs, /nextPasteSuppressUntil|decideDomPasteAction/);
assert.match(mainTs, /onpaste/);

console.log("termPaste.test.js: ok");
