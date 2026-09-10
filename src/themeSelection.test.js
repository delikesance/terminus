/**
 * #127 — themed terminal selection overlay (AC4).
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const themeTs = fs.readFileSync(path.join(root, "src/theme.ts"), "utf8");
const stylesCss = fs.readFileSync(path.join(root, "src/styles.css"), "utf8");

assert.match(
  themeTs,
  /set\(["']--term-selection["']/,
  "applyChrome must publish --term-selection from the theme",
);
assert.match(
  themeTs,
  /selection_background/,
  "selection CSS var must be driven by theme.selection_background",
);
assert.match(
  stylesCss,
  /\.term-selection\s*\{[^}]*background:\s*var\(--term-selection/,
  ".term-selection must use var(--term-selection), not a hardcoded blue wash",
);

console.log("themeSelection.test.js: ok");
