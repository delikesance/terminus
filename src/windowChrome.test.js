/**
 * #99 — OS-aware window chrome helper.
 */

import { resolveWindowChrome, applyWindowChrome } from "./windowChrome.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  {
    const ok =
      resolveWindowChrome({ platform: "MacIntel" }) === "mac" &&
      resolveWindowChrome({ platform: "Macintosh" }) === "mac" &&
      resolveWindowChrome({ userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)" }) === "mac";
    checks.push(check("AC1 resolveWindowChrome → mac for Mac platform/UA", ok, {}));
  }

  {
    const ok =
      resolveWindowChrome({ platform: "Win32" }) === "win" &&
      resolveWindowChrome({ platform: "Linux x86_64" }) === "win" &&
      resolveWindowChrome({ userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" }) === "win" &&
      resolveWindowChrome({}) === "win";
    checks.push(check("AC2 resolveWindowChrome → win for Windows/Linux/unknown", ok, {}));
  }

  {
    const ok =
      resolveWindowChrome({ platform: "Win32", force: "mac" }) === "mac" &&
      resolveWindowChrome({ platform: "MacIntel", force: "win" }) === "win";
    checks.push(check("force override wins over platform", ok, {}));
  }

  {
    const root = { dataset: {} };
    applyWindowChrome("mac", root);
    const a = root.dataset.osChrome === "mac";
    applyWindowChrome("win", root);
    const b = root.dataset.osChrome === "win";
    checks.push(check("applyWindowChrome sets data-os-chrome", a && b, root.dataset));
  }

  let failed = 0;
  for (const c of checks) {
    console.log(`${c.ok ? "ok" : "FAIL"}  ${c.name}`, c.detail ?? "");
    if (!c.ok) failed += 1;
  }
  console.log(failed ? `\n${failed}/${checks.length} failed` : `\n${checks.length} passed`);
  process.exit(failed ? 1 : 0);
}

runTests();
