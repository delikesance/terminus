/**
 * #134 — Linux/system clipboard paste uses Tauri clipboard-manager.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const caps = fs.readFileSync(path.join(root, "src-tauri/capabilities/default.json"), "utf8");
const cargo = fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf8");
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const libRs = fs.readFileSync(path.join(root, "src-tauri/src/lib.rs"), "utf8");
const mainTs = fs.readFileSync(path.join(root, "src/main.ts"), "utf8");

assert.match(caps, /clipboard-manager:allow-read-text/);
assert.match(caps, /clipboard-manager:allow-write-text/);
assert.match(cargo, /tauri-plugin-clipboard-manager/);
assert.ok(
  pkg.dependencies?.["@tauri-apps/plugin-clipboard-manager"],
  "npm clipboard-manager dependency missing",
);
assert.match(libRs, /tauri_plugin_clipboard_manager::init/);
assert.match(mainTs, /tauriClipboardReadText/);
assert.match(mainTs, /tauriClipboardWriteText/);

console.log("clipboardLinux.test.js: ok");
