/**
 * Performance / crash-load helpers (#45).
 * Pure JS CPU harness + shared thresholds for e2e assertions.
 */

export type FileListEntry = { name: string };

export type PerfHeap = {
  usedMb: number;
  totalMb: number;
  limitMb: number;
} | null;

export type PerfSnapshot = {
  at: number;
  panes: number;
  canvases: number;
  heap: PerfHeap;
  webglOk: boolean;
};

export type CpuBenchResult = {
  name: string;
  n: number;
  iterations: number;
  ms: number;
  opsPerSec: number;
};

/** Soft ceilings — CI runners vary; these catch pathological regressions. */
export const PERF_THRESHOLDS = {
  /** Filter 5k entries × 40 iterations must finish under this. */
  filter5kMs: 2500,
  /** Canvas paint loop (see e2e) wall time. */
  canvasStressMs: 20_000,
  /** Max JS heap growth during an e2e stress (Chrome performance.memory). */
  heapGrowthMb: 50,
  /** Min rAF samples over a 1s window after stress (sanity, not hard FPS SLA). */
  minFpsSamples: 20,
  /** Native-comparable: sustained FPS under multi-pane paint stress (#47). */
  nativeMinFps: 55,
  /** Multi-pane open count for crash test. */
  paneCount: 6,
  /** Synthetic SFTP listing size. */
  sftpBulkCount: 800,
  /** Virtual list should not mount the full bulk set. */
  maxVirtualDomRows: 120,
} as const;

/** Epic #47 definition-of-done gates (preview + native). */
export const NATIVE_COMPARABLE_GATES = {
  minFps: 55,
  paintP95Ms: 16,
  sftpFilterVisibleMs: 50,
  heapGrowthMb: 50,
} as const;

export function makeEntries(n: number): FileListEntry[] {
  const out: FileListEntry[] = [];
  for (let i = 0; i < n; i++) {
    const hidden = i % 17 === 0 ? "." : "";
    out.push({ name: `${hidden}file-${i.toString().padStart(5, "0")}.txt` });
  }
  return out;
}

export type FilterFn = (
  entries: FileListEntry[],
  filter: { query: string; showHidden: boolean },
) => FileListEntry[];

/** CPU load: repeated listing filter (name query + hidden toggle). */
export function benchFilterCpu(
  filterFn: FilterFn,
  opts?: { n?: number; iterations?: number },
): CpuBenchResult {
  const n = opts?.n ?? 5000;
  const iterations = opts?.iterations ?? 40;
  const entries = makeEntries(n);
  const t0 = performance.now();
  let kept = 0;
  for (let i = 0; i < iterations; i++) {
    const q = i % 2 === 0 ? "file-00" : "file-12";
    const showHidden = i % 3 === 0;
    kept += filterFn(entries, { query: q, showHidden }).length;
  }
  const ms = performance.now() - t0;
  if (kept < 0) throw new Error("unreachable");
  return {
    name: "filterFileEntries",
    n,
    iterations,
    ms,
    opsPerSec: ms > 0 ? (iterations / ms) * 1000 : Infinity,
  };
}

export function heapFromPerformance(perf: {
  memory?: { usedJSHeapSize: number; totalJSHeapSize: number; jsHeapSizeLimit: number };
}): PerfHeap {
  const m = perf.memory;
  if (!m) return null;
  const mb = (b: number) => Math.round((b / (1024 * 1024)) * 10) / 10;
  return {
    usedMb: mb(m.usedJSHeapSize),
    totalMb: mb(m.totalJSHeapSize),
    limitMb: mb(m.jsHeapSizeLimit),
  };
}

export function assertCpuBench(result: CpuBenchResult): void {
  if (result.ms > PERF_THRESHOLDS.filter5kMs) {
    throw new Error(
      `CPU filter bench too slow: ${result.ms.toFixed(1)}ms > ${PERF_THRESHOLDS.filter5kMs}ms`,
    );
  }
}

export type PerfReport = {
  generatedAt: string;
  cpu: CpuBenchResult;
  thresholds: typeof PERF_THRESHOLDS;
  e2e?: Record<string, unknown>;
};

export function buildCpuReport(cpu: CpuBenchResult): PerfReport {
  assertCpuBench(cpu);
  return {
    generatedAt: new Date().toISOString(),
    cpu,
    thresholds: PERF_THRESHOLDS,
  };
}
