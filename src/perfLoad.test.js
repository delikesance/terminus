/**
 * Performance crash/load CPU harness (#45).
 */

import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  PERF_THRESHOLDS,
  assertCpuBench,
  benchFilterCpu,
  buildCpuReport,
  makeEntries,
} from "./perfLoad.js";
import { filterFileEntries } from "./sftpUx.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];
  const entries = makeEntries(100);
  const filtered = filterFileEntries(entries, { query: "file-00007.txt", showHidden: false });
  checks.push(check("makeEntries+filter smoke", filtered.length === 1 && filtered[0].name === "file-00007.txt", filtered.map((e) => e.name).join(",")));

  const cpu = benchFilterCpu(filterFileEntries, { n: 5000, iterations: 40 });
  let benchOk = true;
  let benchDetail = `${cpu.ms.toFixed(1)}ms (${cpu.opsPerSec.toFixed(0)} iter/s)`;
  try {
    assertCpuBench(cpu);
  } catch (err) {
    benchOk = false;
    benchDetail = String(err);
  }
  checks.push(check(`CPU filter ≤ ${PERF_THRESHOLDS.filter5kMs}ms`, benchOk, benchDetail));

  const report = buildCpuReport(cpu);
  const here = dirname(fileURLToPath(import.meta.url));
  const outDir = join(here, "..", "artifacts");
  mkdirSync(outDir, { recursive: true });
  const outPath = join(outDir, "perf-report.json");
  writeFileSync(outPath, JSON.stringify(report, null, 2));
  checks.push(check("wrote artifacts/perf-report.json", true, outPath));

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
