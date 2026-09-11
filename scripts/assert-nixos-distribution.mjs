#!/usr/bin/env node
/**
 * Asserts NixOS distribution channels are wired for end users (issue #156).
 *
 * AC1: README documents NixOS install via nixpkgs, FlakeHub, and github+Cachix
 * AC3: Release CI builds/pushes the Nix package to Cachix (like deb/rpm/exe)
 * AC4: Release CI publishes the flake to FlakeHub
 * AC5: Shared nix/package.nix exists for flake + nixpkgs
 * AC6: Flake still exposes terminus / default / terminus-selftest (Linux)
 *
 * CI/CD constraint: Nix publish must live in release-path workflows (push to main
 * / release.yml family), not only ad-hoc local docs.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function fail(msg) {
  console.error(`assert-nixos-distribution: ${msg}`);
  process.exit(1);
}

function read(rel) {
  const p = path.join(root, rel);
  if (!fs.existsSync(p)) fail(`missing required file: ${rel}`);
  return fs.readFileSync(p, "utf8");
}

function listWorkflows() {
  const dir = path.join(root, ".github", "workflows");
  if (!fs.existsSync(dir)) fail("missing .github/workflows");
  return fs
    .readdirSync(dir)
    .filter((f) => /\.ya?ml$/i.test(f))
    .map((f) => ({
      name: f,
      text: fs.readFileSync(path.join(dir, f), "utf8"),
    }));
}

const packageNix = path.join(root, "nix", "package.nix");
if (!fs.existsSync(packageNix)) {
  fail("AC5: expected nix/package.nix (shared derivation for flake + nixpkgs)");
}
const packageNixText = fs.readFileSync(packageNix, "utf8");
if (!/pname\s*=\s*"terminus"/.test(packageNixText) && !/pname\s*=\s*'terminus'/.test(packageNixText)) {
  fail('AC5: nix/package.nix must set pname = "terminus" (or document alt name)');
}
console.log("AC5 ok: nix/package.nix present with pname terminus");

const readme = read("README.md");
const readmeNeedles = [
  { re: /NixOS/i, label: "NixOS" },
  { re: /nixpkgs|environment\.systemPackages/i, label: "nixpkgs / systemPackages" },
  { re: /flakehub\.com|FlakeHub/i, label: "FlakeHub" },
  { re: /cachix/i, label: "Cachix" },
  { re: /github:delikesance\/terminus#terminus/, label: "github flake install" },
];
for (const { re, label } of readmeNeedles) {
  if (!re.test(readme)) {
    fail(`AC1: README must document ${label} install path for NixOS users`);
  }
}
console.log("AC1 ok: README documents nixpkgs, FlakeHub, Cachix, and github flake");

const workflows = listWorkflows();
const releaseish = workflows.filter(({ name, text }) => {
  const namedRelease = /^release\./i.test(name);
  const pushesMain =
    /branches:\s*\[[^\]]*main/.test(text) ||
    (/push:/.test(text) && /-\s*main\b/.test(text));
  return namedRelease || pushesMain;
});
if (!releaseish.length) {
  fail("AC3/AC4: no release-path workflow found (expected release.yml or push-to-main)");
}

const releaseCorpus = releaseish.map((w) => w.text).join("\n---\n");
const allCorpus = workflows.map((w) => w.text).join("\n---\n");

// Prefer release-path; fall back to any workflow only to give a clearer error.
function requireInReleasePath(re, ac, label) {
  if (re.test(releaseCorpus)) return;
  if (re.test(allCorpus)) {
    fail(
      `${ac}: ${label} found in CI but not on the release path (push to main / release.yml). ` +
        "Nix distribution must ship via CI/CD like deb/rpm/exe.",
    );
  }
  fail(`${ac}: release-path CI must ${label}`);
}

requireInReleasePath(/cachix\/cachix-action|cachix push/i, "AC3", "push Nix builds to Cachix");
requireInReleasePath(/nix build[^\n]*#terminus|\.#terminus/, "AC3", "nix build .#terminus (or equivalent)");
requireInReleasePath(
  /DeterminateSystems\/flakehub-push|flakehub-push@/i,
  "AC4",
  "publish the flake to FlakeHub",
);
console.log("AC3/AC4 ok: release-path CI builds/pushes Cachix and publishes FlakeHub");

// AC6 — flake outputs (Linux only; matches assert-nix-linux-package).
if (os.platform() !== "linux") {
  console.log("AC6 skipped (flake eval requires Linux host in this assert)");
  process.exit(0);
}

function nixEvalJson(expr) {
  const r = spawnSync("nix", ["eval", "--json", expr], {
    cwd: root,
    encoding: "utf8",
  });
  if (r.error) fail(`nix eval: ${r.error.message}`);
  if (r.status !== 0) fail(`nix eval ${expr} failed:\n${r.stderr || r.stdout}`);
  return JSON.parse(r.stdout);
}

const terminusPname = nixEvalJson(".#terminus.pname");
if (terminusPname !== "terminus") {
  fail(`AC6: .#terminus.pname expected "terminus", got ${JSON.stringify(terminusPname)}`);
}
const defaultPname = nixEvalJson(".#default.pname");
if (defaultPname !== "terminus") {
  fail(`AC6: .#default.pname expected "terminus", got ${JSON.stringify(defaultPname)}`);
}
const selftestPname = nixEvalJson(".#terminus-selftest.pname");
if (selftestPname !== "terminus-selftest") {
  fail(`AC6: must not regress terminus-selftest; got ${JSON.stringify(selftestPname)}`);
}

// Flake must call the shared package file (not only inline duplicate).
const flakeText = read("flake.nix");
if (!/nix\/package\.nix|.\.\/nix\/package\.nix/.test(flakeText) && !/package\.nix/.test(flakeText)) {
  fail("AC5/AC6: flake.nix must import nix/package.nix for the desktop package");
}

console.log("AC6 ok: flake outputs intact and flake imports shared package.nix");
process.exit(0);
