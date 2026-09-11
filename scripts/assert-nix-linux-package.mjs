#!/usr/bin/env node
/**
 * Asserts the flake exposes an installable Linux desktop package (issue #151).
 *
 * AC1: packages.<system>.terminus exists; packages.default is the desktop app
 * AC2: nix build produces $out/bin/terminus
 * AC3: binary resolves shared libs (ldd) without "not found"
 *
 * Full build is opt-in via TERMINUS_NIX_BUILD=1 (slow). Eval checks always run.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function fail(msg) {
  console.error(`assert-nix-linux-package: ${msg}`);
  process.exit(1);
}

function nix(args, opts = {}) {
  const r = spawnSync("nix", args, {
    cwd: root,
    encoding: "utf8",
    ...opts,
  });
  if (r.error) fail(`nix ${args.join(" ")}: ${r.error.message}`);
  return r;
}

function nixEvalJson(expr) {
  const r = nix(["eval", "--json", expr], { stdio: ["ignore", "pipe", "pipe"] });
  if (r.status !== 0) {
    fail(`nix eval ${expr} failed:\n${r.stderr || r.stdout}`);
  }
  return JSON.parse(r.stdout);
}

const host = os.platform();
if (host !== "linux") {
  console.log(`assert-nix-linux-package: skip (host is ${host}, Linux required)`);
  process.exit(0);
}

// Flake installables resolve under packages.<currentSystem>, so use short refs.
const terminusRef = ".#terminus";
const defaultRef = ".#default";

const terminusPname = nixEvalJson(`${terminusRef}.pname`);
if (terminusPname !== "terminus") {
  fail(`expected ${terminusRef}.pname === "terminus", got ${JSON.stringify(terminusPname)}`);
}

const defaultPname = nixEvalJson(`${defaultRef}.pname`);
if (defaultPname !== "terminus") {
  fail(`expected ${defaultRef}.pname === "terminus", got ${JSON.stringify(defaultPname)}`);
}

const selftestPname = nixEvalJson(".#terminus-selftest.pname");
if (selftestPname !== "terminus-selftest") {
  fail(`must not regress terminus-selftest; got ${JSON.stringify(selftestPname)}`);
}

console.log("AC1 ok: .#terminus and .#default pname=terminus (selftest preserved)");

if (process.env.TERMINUS_NIX_BUILD !== "1") {
  console.log("AC2/AC3 skipped (set TERMINUS_NIX_BUILD=1 to build and check $out/bin/terminus)");
  process.exit(0);
}

const outLink = path.join(root, "result-nix-linux-package");
const build = nix(
  ["build", terminusRef, "--out-link", outLink],
  { stdio: "inherit" },
);
if (build.status !== 0) fail(`nix build ${terminusRef} failed`);

const bin = path.join(outLink, "bin", "terminus");
if (!fs.existsSync(bin)) {
  fail(`AC2: missing ${bin} after nix build`);
}
const st = fs.statSync(bin);
if (!st.isFile()) fail(`AC2: ${bin} is not a regular file`);

// wrapGAppsHook leaves the real ELF as .terminus-wrapped; prefer that for ldd.
const wrapped = path.join(outLink, "bin", ".terminus-wrapped");
const lddTarget = fs.existsSync(wrapped) ? wrapped : bin;
const ldd = spawnSync("ldd", [lddTarget], { encoding: "utf8" });
if (ldd.status !== 0 && !ldd.stdout) {
  fail(`AC3: ldd failed on ${lddTarget}: ${ldd.stderr}`);
}
const missing = (ldd.stdout || "")
  .split("\n")
  .filter((line) => /not found/i.test(line));
if (missing.length) {
  fail(`AC3: unresolved shared libraries:\n${missing.join("\n")}`);
}
const out = ldd.stdout || "";
for (const needle of ["libwebkit", "libgssapi"]) {
  if (!out.includes(needle)) {
    fail(`AC3: expected ${needle} linkage in ${lddTarget}`);
  }
}

console.log(`AC2/AC3 ok: ${bin} present; ldd clean on ${path.basename(lddTarget)}`);
process.exit(0);
