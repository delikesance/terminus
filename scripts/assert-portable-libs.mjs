#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const compile = spawnSync("node", [path.join(root, "scripts/compile-portable-bundle.mjs")], {
  cwd: root,
  stdio: "inherit",
});
if (compile.status !== 0) process.exit(compile.status ?? 1);

const {
  linuxPortableViolations,
  macosGssViolations,
  parseElfDynamic,
  parseOtoolL,
  parsePeImportDlls,
  windowsGssViolations,
} = await import(pathToFileURL(path.join(root, "src/portableBundle.js")).href);

function walkForBinary(dir, depth, names) {
  if (depth > 7 || !fs.existsSync(dir)) return [];
  const hits = [];
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) hits.push(...walkForBinary(p, depth + 1, names));
    else if (names.has(ent.name)) hits.push(p);
  }
  return hits;
}

const targetDir = process.env.CARGO_TARGET_DIR || path.join(root, "target");
const exeNames =
  process.platform === "win32" || process.env.TAURI_ENV_PLATFORM === "windows"
    ? new Set(["terminus.exe"])
    : new Set(["terminus"]);
const bins = walkForBinary(targetDir, 0, exeNames).filter(
  (p) => p.includes(`${path.sep}release${path.sep}`) || p.includes(`${path.sep}debug${path.sep}`),
);
bins.sort((a, b) => {
  const score = (p) =>
    (p.includes(`${path.sep}release${path.sep}`) ? 4 : 0) +
    (!p.includes(`${path.sep}bundle${path.sep}`) ? 2 : 0);
  return score(b) - score(a);
});
const bin = process.argv[2] || bins[0];
if (!bin || !fs.existsSync(bin)) {
  console.error("assert-portable-libs: terminus binary not found under", targetDir);
  process.exit(1);
}

const platform = process.env.TAURI_ENV_PLATFORM || process.platform;
const bytes = fs.readFileSync(bin);

if (platform === "linux") {
  const { needed, rpath } = parseElfDynamic(bytes);
  const bundleDir = path.join(root, "src-tauri/native-libs/gssapi");
  const bundled = fs.existsSync(bundleDir)
    ? fs.readdirSync(bundleDir).filter((f) => f !== ".gitkeep")
    : [];
  const violations = linuxPortableViolations({ needed, rpath, bundled });
  if (violations.length) {
    console.error("Linux portable-lib violations:\n" + violations.map((v) => `  - ${v}`).join("\n"));
    console.error("NEEDED:", needed.join(", "));
    console.error("RPATH:", rpath);
    console.error("bundled:", bundled.join(", ") || "(none)");
    process.exit(1);
  }
  console.log("assert-portable-libs: linux ok", path.relative(root, bin));
  process.exit(0);
}

if (platform === "darwin" || platform === "macos") {
  const otool = spawnSync("otool", ["-L", bin], { encoding: "utf8" });
  if (otool.status !== 0) {
    console.error("otool failed", otool.stderr);
    process.exit(1);
  }
  const violations = macosGssViolations(parseOtoolL(otool.stdout));
  if (violations.length) {
    console.error("macOS GSS violations:\n" + violations.map((v) => `  - ${v}`).join("\n"));
    console.error(otool.stdout);
    process.exit(1);
  }
  console.log("assert-portable-libs: macos ok", path.relative(root, bin));
  process.exit(0);
}

if (platform === "win32" || platform === "windows") {
  const imports = parsePeImportDlls(bytes);
  const violations = windowsGssViolations(imports);
  if (violations.length) {
    console.error("Windows GSS violations:\n" + violations.map((v) => `  - ${v}`).join("\n"));
    process.exit(1);
  }
  console.log("assert-portable-libs: windows ok", path.relative(root, bin));
  process.exit(0);
}

console.error("assert-portable-libs: unsupported platform", platform);
process.exit(1);
