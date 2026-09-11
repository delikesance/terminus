/**
 * #153 — terminal URL detection and click-to-open gating.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  findUrlAt,
  isOpenableHttpUrl,
  shouldAttemptLinkOpen,
} from "./termLinks.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const mainTs = fs.readFileSync(path.join(root, "src/main.ts"), "utf8");
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));

// AC1 — detect URL at column
{
  const line = "see https://example.com/path?x=1 and more";
  const hit = findUrlAt(line, line.indexOf("example"));
  assert.ok(hit);
  assert.equal(hit.url, "https://example.com/path?x=1");
  assert.equal(hit.start, line.indexOf("https://"));
  assert.equal(hit.end, hit.start + hit.url.length);
  assert.equal(findUrlAt(line, 0), null);
  assert.equal(findUrlAt(line, line.indexOf(" and")), null);
}

// Trailing punctuation stripped
{
  const line = "docs (https://example.com/a).";
  const hit = findUrlAt(line, line.indexOf("example"));
  assert.ok(hit);
  assert.equal(hit.url, "https://example.com/a");
}

// http allowed
{
  const line = "http://localhost:8080/ok";
  const hit = findUrlAt(line, 0);
  assert.ok(hit);
  assert.equal(hit.url, "http://localhost:8080/ok");
  assert.equal(isOpenableHttpUrl(hit.url), true);
}

// AC4 — unsafe / non-http schemes rejected
assert.equal(isOpenableHttpUrl("javascript:alert(1)"), false);
assert.equal(isOpenableHttpUrl("file:///etc/passwd"), false);
assert.equal(isOpenableHttpUrl("ftp://example.com/a"), false);
assert.equal(findUrlAt("open javascript:alert(1) now", 5), null);
assert.equal(findUrlAt("file:///tmp/x", 0), null);

// AC3 — mouse mode / drag gating
assert.equal(
  shouldAttemptLinkOpen({
    button: 0,
    shiftKey: false,
    mouseReporting: false,
    dragOccurred: false,
  }),
  true,
);
assert.equal(
  shouldAttemptLinkOpen({
    button: 0,
    shiftKey: false,
    mouseReporting: true,
    dragOccurred: false,
  }),
  false,
);
assert.equal(
  shouldAttemptLinkOpen({
    button: 0,
    shiftKey: false,
    mouseReporting: false,
    dragOccurred: true,
  }),
  false,
);
assert.equal(
  shouldAttemptLinkOpen({
    button: 2,
    shiftKey: false,
    mouseReporting: false,
    dragOccurred: false,
  }),
  false,
);

// Wiring — main opens links via termLinks + shell open
assert.match(mainTs, /from ["']\.\/termLinks/);
assert.match(mainTs, /findUrlAt|shouldAttemptLinkOpen/);
assert.match(mainTs, /@tauri-apps\/plugin-shell|open\(/);
assert.ok(
  pkg.dependencies?.["@tauri-apps/plugin-shell"] ||
    pkg.devDependencies?.["@tauri-apps/plugin-shell"],
  "expected @tauri-apps/plugin-shell dependency",
);

console.log("termLinks.test.js: ok");
