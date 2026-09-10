import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  buildRsAvoidsVendoredGssapiRpath,
  cargoTomlGssapiUnixOnly,
  isKerberosSoname,
  lddKerberosEntries,
  linuxPortableViolations,
  macosGssViolations,
  macosJobsForceAppleGssOnMacOnly,
  parseLddMapping,
  parseOtoolL,
  parseReadelfDynamic,
  releaseYamlBuildsRpm,
  releaseYamlInstallsKrb5Toolchain,
  tauriConfDependsOnDistroGssapi,
  tauriConfUsesSystemKerberos,
  windowsGssViolations,
  workflowsRunPortableLibChecker,
} from "./portableBundle.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

assert.equal(isKerberosSoname("libgssapi_krb5.so.2"), true);
assert.equal(isKerberosSoname("libkrb5.so.3"), true);
assert.equal(isKerberosSoname("libwebkit2gtk-4.1.so.0"), false);
assert.equal(isKerberosSoname("libc.so.6"), false);

const ldd = `
	libgssapi_krb5.so.2 => /usr/lib/x86_64-linux-gnu/libgssapi_krb5.so.2 (0x1)
	libkrb5.so.3 => /usr/lib/x86_64-linux-gnu/libkrb5.so.3 (0x2)
	libwebkit2gtk-4.1.so.0 => /usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0 (0x6)
	libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x7)
`;
const mapped = parseLddMapping(ldd);
assert.deepEqual(
  lddKerberosEntries(mapped).map((e) => e.soname).sort(),
  ["libgssapi_krb5.so.2", "libkrb5.so.3"],
);

assert.deepEqual(
  linuxPortableViolations({
    needed: ["libgssapi_krb5.so.2", "libc.so.6"],
    rpath: "",
    bundled: [],
  }),
  [],
);

assert.ok(
  linuxPortableViolations({
    needed: ["libgssapi_krb5.so.2"],
    rpath: "$ORIGIN:$ORIGIN/../lib/terminus",
    bundled: [],
  }).some((v) => v.toLowerCase().includes("lib/terminus")),
);

assert.ok(
  linuxPortableViolations({
    needed: ["libgssapi_krb5.so.2"],
    rpath: "",
    bundled: ["libgssapi_krb5.so.2"],
  }).some((v) => v.includes("libgssapi_krb5")),
);

const readelf = `
 0x0000000000000001 (NEEDED)             Shared library: [libgssapi_krb5.so.2]
 0x000000000000001d (RUNPATH)            Library runpath: [$ORIGIN]
`;
assert.deepEqual(parseReadelfDynamic(readelf), {
  needed: ["libgssapi_krb5.so.2"],
  rpath: "$ORIGIN",
});

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

assert.deepEqual(windowsGssViolations(["KERNEL32.dll", "WEBVIEW2LOADER.dll"]), []);
assert.ok(windowsGssViolations(["gssapi64.dll"]).length > 0);

assert.equal(
  releaseYamlInstallsKrb5Toolchain(`
          sudo apt-get install -y libwebkit2gtk-4.1-dev libkrb5-dev clang libclang-dev
`),
  true,
);

assert.equal(
  tauriConfUsesSystemKerberos({
    build: { beforeBundleCommand: "" },
    bundle: {
      linux: {
        deb: { depends: ["libgssapi-krb5-2"], files: {} },
        rpm: { depends: ["krb5-libs"] },
        appimage: { files: {} },
      },
    },
  }),
  true,
);
assert.equal(
  tauriConfUsesSystemKerberos({
    build: { beforeBundleCommand: "node scripts/stage-gssapi-libs.mjs" },
    bundle: {
      linux: {
        deb: { files: { "/usr/lib/terminus": "native-libs/gssapi" } },
        appimage: { files: { "/usr/lib/terminus": "native-libs/gssapi" } },
      },
    },
  }),
  false,
);

assert.equal(
  tauriConfDependsOnDistroGssapi({
    bundle: {
      linux: {
        deb: { depends: ["libgssapi-krb5-2"] },
        rpm: { depends: ["krb5-libs"] },
      },
    },
  }),
  true,
);
assert.equal(
  tauriConfDependsOnDistroGssapi({
    bundle: { linux: { deb: { depends: ["libgssapi-krb5-2"] } } },
  }),
  false,
);

assert.equal(
  macosJobsForceAppleGssOnMacOnly(`
  build-macos:
    steps:
      - name: Force Apple GSS
        if: runner.os == 'macOS'
        run: echo "LIBGSSAPI_IMPL=apple" >> $GITHUB_ENV
`),
  true,
);

assert.equal(
  buildRsAvoidsVendoredGssapiRpath(`
    // no vendor rpath
    tauri_build::build()
`),
  true,
);
assert.equal(
  buildRsAvoidsVendoredGssapiRpath(`
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/terminus");
`),
  false,
);

assert.equal(
  cargoTomlGssapiUnixOnly(`
[target.'cfg(unix)'.dependencies]
libgssapi = "0.11"
`),
  true,
);

assert.equal(
  workflowsRunPortableLibChecker(`
      - run: node scripts/assert-portable-libs.mjs
`),
  true,
);

const releaseYml = fs.readFileSync(path.join(root, ".github/workflows/release.yml"), "utf8");
const ciYml = fs.readFileSync(path.join(root, ".github/workflows/ci.yml"), "utf8");
const tauriConf = JSON.parse(fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8"));
const buildRs = fs.readFileSync(path.join(root, "src-tauri/build.rs"), "utf8");
const coreToml = fs.readFileSync(path.join(root, "crates/terminus-core/Cargo.toml"), "utf8");

assert.equal(releaseYamlInstallsKrb5Toolchain(releaseYml), true, "release still needs krb5 headers to compile");
assert.equal(macosJobsForceAppleGssOnMacOnly(ciYml), true);
assert.equal(macosJobsForceAppleGssOnMacOnly(releaseYml), true);
assert.equal(tauriConfUsesSystemKerberos(tauriConf), true, "must use system Kerberos (no vendor)");
assert.equal(tauriConfDependsOnDistroGssapi(tauriConf), true, "deb+rpm must declare Kerberos runtime deps");
assert.equal(releaseYamlBuildsRpm(releaseYml), true, "release must build rpm and install rpm tooling");
assert.equal(buildRsAvoidsVendoredGssapiRpath(buildRs), true, "must not rpath to lib/terminus");
assert.equal(cargoTomlGssapiUnixOnly(coreToml), true);
assert.equal(workflowsRunPortableLibChecker(ciYml), true);
assert.equal(workflowsRunPortableLibChecker(releaseYml), true);

console.log("portableBundle tests ok");
