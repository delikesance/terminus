#!/usr/bin/env node
/**
 * After AppImage packaging, remove MIT Kerberos libs that linuxdeploy may have
 * copied into the AppDir so the binary uses the host stack (KCM-compatible).
 *
 * Requires: squashfs-tools (unsquashfs/mksquashfs) OR a working AppImage extract
 * + appimagetool. On CI Ubuntu we prefer unsquashfs when the AppImage runtime
 * allows offset discovery via `od`.
 *
 * Simpler path used here: if an unpacked AppDir still exists next to the
 * AppImage (Tauri sometimes leaves build dirs), strip there; otherwise extract
 * with the AppImage itself and rebuild with appimagetool when available.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const targetDir = process.env.CARGO_TARGET_DIR || path.join(root, "target");

const KERBEROS_GLOBS = [
  "libgssapi_krb5.so*",
  "libgssapi.so*",
  "libkrb5.so*",
  "libkrb5support.so*",
  "libk5crypto.so*",
  "libcom_err.so*",
  "libkeyutils.so*",
  "libverto.so*",
];

function walk(dir, depth, pred) {
  if (depth > 8 || !fs.existsSync(dir)) return [];
  const hits = [];
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) hits.push(...walk(p, depth + 1, pred));
    else if (pred(p, ent.name)) hits.push(p);
  }
  return hits;
}

function stripDir(libRoot) {
  if (!fs.existsSync(libRoot)) return 0;
  let n = 0;
  for (const ent of fs.readdirSync(libRoot, { withFileTypes: true })) {
    const p = path.join(libRoot, ent.name);
    if (ent.isDirectory()) {
      n += stripDir(p);
      continue;
    }
    if (KERBEROS_GLOBS.some((g) => ent.name.match(globToReg(g)))) {
      fs.unlinkSync(p);
      console.log("stripped", p);
      n += 1;
    }
  }
  return n;
}

function globToReg(g) {
  const esc = g.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*");
  return new RegExp(`^${esc}$`);
}

function findAppImages() {
  return walk(
    path.join(targetDir, "release", "bundle", "appimage"),
    0,
    (_p, name) => name.endsWith(".AppImage"),
  );
}

function which(cmd) {
  const r = spawnSync("bash", ["-lc", `command -v ${cmd}`], { encoding: "utf8" });
  if (r.status === 0 && r.stdout.trim()) return r.stdout.trim();
  const home = os.homedir();
  const searchRoots = [
    path.join(home, ".cache", "tauri"),
    path.join(root, "src-tauri"),
    path.join(targetDir),
  ];
  for (const dir of searchRoots) {
    const hits = walk(
      dir,
      0,
      (_p, name) => name === "appimagetool" || name.startsWith("appimagetool-"),
    );
    if (hits[0]) return hits[0];
  }
  return "";
}

function extractAndStrip(appImage) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "terminus-appimage-"));
  const extract = spawnSync(
    appImage,
    ["--appimage-extract"],
    {
      cwd: tmp,
      encoding: "utf8",
      env: { ...process.env, APPIMAGE_EXTRACT_AND_RUN: "1" },
    },
  );
  if (extract.status !== 0) {
    console.warn("strip-appimage-krb5: extract failed", extract.stderr || extract.stdout);
    fs.rmSync(tmp, { recursive: true, force: true });
    return false;
  }
  const appDir = path.join(tmp, "squashfs-root");
  const removed = stripDir(path.join(appDir, "usr", "lib")) + stripDir(path.join(appDir, "usr", "lib64"));
  const tool = which("appimagetool");
  if (!tool) {
    console.warn("strip-appimage-krb5: appimagetool not found; left", removed, "libs removed in temp only");
    fs.rmSync(tmp, { recursive: true, force: true });
    return false;
  }
  const out = appImage + ".stripped";
  const pack = spawnSync(tool, ["--no-appstream", appDir, out], {
    encoding: "utf8",
    env: { ...process.env, ARCH: process.env.ARCH || "x86_64" },
  });
  if (pack.status !== 0) {
    console.warn("strip-appimage-krb5: appimagetool failed", pack.stderr || pack.stdout);
    fs.rmSync(tmp, { recursive: true, force: true });
    return false;
  }
  fs.renameSync(out, appImage);
  fs.chmodSync(appImage, 0o755);
  fs.rmSync(tmp, { recursive: true, force: true });
  console.log(`strip-appimage-krb5: rebuilt ${appImage} (removed ${removed} kerberos libs)`);
  return true;
}

if (process.platform !== "linux" && process.env.TAURI_ENV_PLATFORM !== "linux") {
  console.log("strip-appimage-krb5: skip (not linux)");
  process.exit(0);
}

const images = findAppImages();
if (!images.length) {
  console.log("strip-appimage-krb5: no AppImage under", targetDir);
  process.exit(0);
}

let ok = 0;
for (const img of images) {
  if (extractAndStrip(img)) ok += 1;
}
if (ok === 0 && images.length) {
  console.warn(
    "strip-appimage-krb5: could not rebuild AppImage(s). Install appimagetool or use the .deb for Kerberos/KCM.",
  );
}
process.exit(0);
