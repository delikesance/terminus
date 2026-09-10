#!/usr/bin/env node
/**
 * Copy the MIT Kerberos GSSAPI shared-lib closure next to the Linux binary
 * so $ORIGIN/../lib/terminus resolves without distro krb5 packages.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const platform = process.env.TAURI_ENV_PLATFORM || process.platform;
const isLinux = platform === "linux";

if (!isLinux) {
  console.log(`stage-gssapi-libs: skip (${platform})`);
  process.exit(0);
}

const compile = spawnSync("node", [path.join(root, "scripts/compile-portable-bundle.mjs")], {
  cwd: root,
  stdio: "inherit",
});
if (compile.status !== 0) process.exit(compile.status ?? 1);

const { lddEntriesToVendor, parseLddMapping } = await import(
  pathToFileURL(path.join(root, "src/portableBundle.js")).href
);

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
const bins = walkForBinary(targetDir, 0, new Set(["terminus"])).filter(
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
  console.error("stage-gssapi-libs: terminus binary not found under", targetDir);
  process.exit(1);
}

const ldd = spawnSync("ldd", [bin], { encoding: "utf8" });
if (ldd.status !== 0) {
  console.error("stage-gssapi-libs: ldd failed", ldd.stderr);
  process.exit(1);
}

const vendor = lddEntriesToVendor(parseLddMapping(ldd.stdout));
const dest = process.argv[3] || path.join(root, "src-tauri/native-libs/gssapi");
fs.rmSync(dest, { recursive: true, force: true });
fs.mkdirSync(dest, { recursive: true });

if (!vendor.length) {
  console.warn("stage-gssapi-libs: no GSSAPI/krb5 libs in ldd output; writing empty dir");
  process.exit(0);
}

for (const { soname, path: src } of vendor) {
  if (!fs.existsSync(src)) {
    console.error(`stage-gssapi-libs: missing ${src} (${soname})`);
    process.exit(1);
  }
  const destFile = path.join(dest, soname);
  fs.copyFileSync(src, destFile);
  const patch = spawnSync(
    "patchelf",
    ["--force-rpath", "--set-rpath", "$ORIGIN", destFile],
    { encoding: "utf8" },
  );
  if (patch.status !== 0) {
    console.error(`stage-gssapi-libs: patchelf failed on ${soname}`, patch.stderr || patch.stdout);
    process.exit(1);
  }
  console.log(`staged ${soname}`);
}
