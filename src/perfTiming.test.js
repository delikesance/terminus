/**
 * Debounce / frame timing helpers (#48, #53).
 */

import { SFTP_FILTER_DEBOUNCE_MS, createTrailingDebounce, FRAME_MIN_MS } from "./perfTiming.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];
  checks.push(check("SFTP_FILTER_DEBOUNCE_MS", SFTP_FILTER_DEBOUNCE_MS === 100, String(SFTP_FILTER_DEBOUNCE_MS)));
  checks.push(check("FRAME_MIN_MS ~16.6", FRAME_MIN_MS > 16 && FRAME_MIN_MS < 17, String(FRAME_MIN_MS)));

  // Node has no window — polyfill minimal timers for unit check
  globalThis.window = globalThis;
  let calls = 0;
  const d = createTrailingDebounce(20, () => {
    calls += 1;
  });
  d.schedule();
  d.schedule();
  d.schedule();
  checks.push(check("debounce pending after schedule", d.pending(), ""));

  return new Promise((resolve) => {
    setTimeout(() => {
      checks.push(check("debounce trailing once", calls === 1, String(calls)));
      d.schedule();
      d.cancel();
      setTimeout(() => {
        checks.push(check("debounce cancel", calls === 1, String(calls)));
        resolve(checks);
      }, 30);
    }, 30);
  });
}

const results = await runTests();
let failed = 0;
for (const r of results) {
  const mark = r.ok ? "ok" : "FAIL";
  if (!r.ok) failed += 1;
  console.log(`${mark}  ${r.name}${r.detail ? ` — ${r.detail}` : ""}`);
}
console.log(`\n${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
