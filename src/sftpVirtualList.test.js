/**
 * Virtual list math (#50).
 */

import {
  SFTP_VIRTUAL_OVERSCAN,
  SFTP_VIRTUAL_ROW_STRIDE_PX,
  computeVirtualRange,
} from "./sftpVirtualList.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];
  checks.push(check("stride", SFTP_VIRTUAL_ROW_STRIDE_PX === 29, String(SFTP_VIRTUAL_ROW_STRIDE_PX)));
  checks.push(check("overscan", SFTP_VIRTUAL_OVERSCAN === 6, String(SFTP_VIRTUAL_OVERSCAN)));

  const empty = computeVirtualRange(0, 400, 0);
  checks.push(check("empty", empty.end === 0 && empty.totalHeight === 0, JSON.stringify(empty)));

  const fit = computeVirtualRange(0, 400, 10);
  checks.push(check("fits viewport", fit.start === 0 && fit.end === 10, JSON.stringify(fit)));

  const mid = computeVirtualRange(290, 300, 100);
  const firstVisible = Math.floor(290 / 29);
  checks.push(
    check(
      "mid-scroll start",
      mid.start === Math.max(0, firstVisible - SFTP_VIRTUAL_OVERSCAN),
      `start=${mid.start}`,
    ),
  );

  const padOk =
    mid.paddingTop + (mid.end - mid.start) * SFTP_VIRTUAL_ROW_STRIDE_PX + mid.paddingBottom ===
    mid.totalHeight;
  checks.push(check("padding sums to totalHeight", padOk, JSON.stringify(mid)));

  const top = computeVirtualRange(0, 200, 50);
  checks.push(check("top clamp", top.start === 0, String(top.start)));

  const bottom = computeVirtualRange(50 * 29, 200, 50);
  checks.push(check("bottom clamp", bottom.end === 50, String(bottom.end)));

  return checks;
}

const results = runTests();
let failed = 0;
for (const r of results) {
  const mark = r.ok ? "ok" : "FAIL";
  if (!r.ok) failed += 1;
  console.log(`${mark}  ${r.name}${r.detail ? ` — ${r.detail}` : ""}`);
}
console.log(`\n${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
