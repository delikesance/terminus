import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  buildRsSetsOriginRpath,
  cargoTomlGssapiUnixOnly,
  lddEntriesToVendor,
  linuxPortableViolations,
  macosGssViolations,
  macosJobsForceAppleGssOnMacOnly,
  parseLddMapping,
  parseOtoolL,
  parseReadelfDynamic,
  releaseYamlInstallsKrb5Toolchain,
  shouldVendorSoname,
  tauriConfDoesNotDependOnDistroGssapi,
  tauriConfVendorsLinuxGssapi,
  windowsGssViolations,
  workflowsRunPortableLibChecker,
} from "./portableBundle.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

assert.equal(shouldVendorSoname("libgssapi_krb5.so.2"), true);
assert.equal(shouldVendorSoname("libkrb5.so.3"), true);
assert.equal(shouldVendorSoname("libkrb5support.so.0"), true);
assert.equal(shouldVendorSoname("libk5crypto.so.3"), true);
assert.equal(shouldVendorSoname("libcom_err.so.2"), true);
assert.equal(shouldVendorSoname("libkeyutils.so.1"), true);
assert.equal(shouldVendorSoname("libwebkit2gtk-4.1.so.0"), false);
assert.equal(shouldVendorSoname("libgtk-3.so.0"), false);
assert.equal(shouldVendorSoname("libc.so.6"), false);
assert.equal(shouldVendorSoname("libssl.so.3"), false);

const ldd = `
	linux-vdso.so.1 (0x00007ffe)
	libgssapi_krb5.so.2 => /usr/lib/x86_64-linux-gnu/libgssapi_krb5.so.2 (0x1)
	libkrb5.so.3 => /usr/lib/x86_64-linux-gnu/libkrb5.so.3 (0x2)
	libk5crypto.so.3 => /usr/lib/x86_64-linux-gnu/libk5crypto.so.3 (0x3)
	libkrb5support.so.0 => /usr/lib/x86_64-linux-gnu/libkrb5support.so.0 (0x4)
	libcom_err.so.2 => /lib/x86_64-linux-gnu/libcom_err.so.2 (0x5)
	libwebkit2gtk-4.1.so.0 => /usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0 (0x6)
	libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x7)
	libssl.so.3 => /usr/lib/x86_64-linux-gnu/libssl.so.3 (0x8)
`;
const mapped = parseLddMapping(ldd);
assert.equal(mapped.find((e) => e.soname === "libgssapi_krb5.so.2")?.path?.endsWith("libgssapi_krb5.so.2"), true);
const vendor = lddEntriesToVendor(mapped);
assert.deepEqual(
  vendor.map((e) => e.soname).sort(),
  [
    "libcom_err.so.2",
    "libgssapi_krb5.so.2",
    "libk5crypto.so.3",
    "libkrb5.so.3",
    "libkrb5support.so.0",
  ],
);
assert.equal(vendor.some((e) => e.soname.startsWith("libwebkit") || e.soname.startsWith("libssl") || e.soname === "libc.so.6"), false);

const readelf = `
 0x0000000000000001 (NEEDED)             Shared library: [libgssapi_krb5.so.2]
 0x0000000000000001 (NEEDED)             Shared library: [libwebkit2gtk-4.1.so.0]
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]
 0x000000000000001d (RUNPATH)            Library runpath: [$ORIGIN:$ORIGIN/../lib/terminus]
`;
assert.deepEqual(parseReadelfDynamic(readelf), {
  needed: ["libgssapi_krb5.so.2", "libwebkit2gtk-4.1.so.0", "libc.so.6"],
  rpath: "$ORIGIN:$ORIGIN/../lib/terminus",
});

assert.deepEqual(
  linuxPortableViolations({
    needed: ["libgssapi_krb5.so.2", "libc.so.6"],
    rpath: "$ORIGIN:$ORIGIN/../lib/terminus",
    bundled: ["libgssapi_krb5.so.2", "libkrb5.so.3"],
  }),
  [],
);

assert.ok(
  linuxPortableViolations({
    needed: ["libgssapi_krb5.so.2"],
    rpath: "/usr/lib/x86_64-linux-gnu",
    bundled: ["libgssapi_krb5.so.2"],
  }).some((v) => v.toLowerCase().includes("origin")),
);

assert.ok(
  linuxPortableViolations({
    needed: ["libgssapi_krb5.so.2"],
    rpath: "$ORIGIN/../lib/terminus",
    bundled: [],
  }).some((v) => v.includes("libgssapi_krb5")),
);

const otoolApple = parseOtoolL(`
/target/release/terminus:
	/System/Library/Frameworks/GSS.framework/Versions/A/GSS (compatibility version 1.0.0, current version 1.0.0)
	/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1319.0.0)
`);
assert.deepEqual(macosGssViolations(otoolApple), []);

assert.ok(
  macosGssViolations(
    parseOtoolL(`
	/opt/homebrew/opt/krb5/lib/libgssapi_krb5.2.dylib (compatibility version 2.0.0)
	/usr/lib/libSystem.B.dylib
`),
  ).length > 0,
);

assert.deepEqual(windowsGssViolations(["KERNEL32.dll", "WEBVIEW2LOADER.dll", "WS2_32.dll"]), []);
assert.ok(windowsGssViolations(["KERNEL32.dll", "gssapi64.dll"]).length > 0);

assert.equal(
  releaseYamlInstallsKrb5Toolchain(`
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libkrb5-dev clang libclang-dev
`),
  true,
);
assert.equal(
  releaseYamlInstallsKrb5Toolchain(`
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libfuse2
`),
  false,
);

assert.equal(
  tauriConfVendorsLinuxGssapi({
    build: { beforeBundleCommand: "node scripts/stage-gssapi-libs.mjs" },
    bundle: {
      linux: {
        deb: { files: { "/usr/lib/terminus": "native-libs/gssapi" } },
        appimage: { files: { "/usr/lib/terminus": "native-libs/gssapi" } },
      },
    },
  }),
  true,
);
assert.equal(tauriConfVendorsLinuxGssapi({ bundle: { linux: { deb: { files: {} } } } }), false);

assert.equal(
  tauriConfDoesNotDependOnDistroGssapi({
    bundle: { linux: { deb: { depends: ["libwebkit2gtk-4.1-0"] } } },
  }),
  true,
);
assert.equal(
  tauriConfDoesNotDependOnDistroGssapi({
    bundle: { linux: { deb: { depends: ["libgssapi-krb5-2"] } } },
  }),
  false,
);

assert.equal(
  macosJobsForceAppleGssOnMacOnly(`
  build-macos:
    runs-on: macos-latest
    steps:
      - name: Force Apple GSS
        if: runner.os == 'macOS'
        run: echo "LIBGSSAPI_IMPL=apple" >> $GITHUB_ENV
`),
  true,
);
assert.equal(
  macosJobsForceAppleGssOnMacOnly(`
env:
  LIBGSSAPI_IMPL: apple
jobs:
  publish:
    runs-on: ubuntu-22.04
`),
  false,
);

assert.equal(
  buildRsSetsOriginRpath(`
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/terminus");
`),
  true,
);
assert.equal(buildRsSetsOriginRpath("tauri_build::build()"), false);

assert.equal(
  cargoTomlGssapiUnixOnly(`
[target.'cfg(unix)'.dependencies]
libgssapi = "0.11"
`),
  true,
);
assert.equal(
  cargoTomlGssapiUnixOnly(`
[dependencies]
libgssapi = "0.11"
`),
  false,
);

assert.equal(
  workflowsRunPortableLibChecker(`
      - name: Build Tauri
        run: nix develop --command npm run tauri -- build --bundles deb
      - name: Assert bundled native libs
        run: node scripts/assert-portable-libs.mjs
`),
  true,
);
assert.equal(
  workflowsRunPortableLibChecker(`
      - name: Build Tauri
        run: npm run tauri -- build --bundles nsis
`),
  false,
);

const releaseYml = fs.readFileSync(path.join(root, ".github/workflows/release.yml"), "utf8");
const ciYml = fs.readFileSync(path.join(root, ".github/workflows/ci.yml"), "utf8");
const tauriConf = JSON.parse(fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8"));
const buildRs = fs.readFileSync(path.join(root, "src-tauri/build.rs"), "utf8");
const coreToml = fs.readFileSync(path.join(root, "crates/terminus-core/Cargo.toml"), "utf8");

assert.equal(releaseYamlInstallsKrb5Toolchain(releaseYml), true, "release Linux job must apt-install krb5 + clang for bindgen");
assert.equal(macosJobsForceAppleGssOnMacOnly(ciYml), true, "CI macOS must set LIBGSSAPI_IMPL=apple");
assert.equal(macosJobsForceAppleGssOnMacOnly(releaseYml), true, "release macOS must set LIBGSSAPI_IMPL=apple without forcing it on Linux");
assert.equal(tauriConfVendorsLinuxGssapi(tauriConf), true, "deb + AppImage must map staged GSSAPI libs");
assert.equal(tauriConfDoesNotDependOnDistroGssapi(tauriConf), true, "must not rely on deb Depends for GSSAPI");
assert.equal(buildRsSetsOriginRpath(buildRs), true, "Linux binary must have $ORIGIN rpath");
assert.equal(cargoTomlGssapiUnixOnly(coreToml), true);
assert.equal(workflowsRunPortableLibChecker(ciYml), true, "CI must run the portable-lib checker after bundling");
assert.equal(workflowsRunPortableLibChecker(releaseYml), true, "release must run the portable-lib checker after bundling");

console.log("portableBundle tests ok");
