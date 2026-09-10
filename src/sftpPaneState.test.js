/**
 * Unit checks for per-host SFTP pane load seq (#109).
 */

import {
  beginPaneLoad,
  createSftpPaneLive,
  failPaneLoad,
  finishPaneLoad,
  isPaneLoadCurrent,
} from "./sftpPaneState.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  {
    const live = createSftpPaneLive("h1");
    checks.push(
      check(
        "AC109 default live not listed",
        live.hostId === "h1" && live.loadSeq === 0 && !live.loading && !live.listed,
        JSON.stringify(live),
      ),
    );
  }

  {
    let a = createSftpPaneLive("h1");
    let b = createSftpPaneLive("h2");
    const a1 = beginPaneLoad(a, ".");
    a = a1.live;
    const b1 = beginPaneLoad(b, ".");
    b = b1.live;
    checks.push(
      check(
        "AC109 beginPaneLoad is independent per host",
        a.loadSeq === 1 && b.loadSeq === 1 && a.loading && b.loading,
        JSON.stringify({ a, b }),
      ),
    );
    // Finishing B must not affect A seq validity
    b = finishPaneLoad(b, b1.seq, { path: "/home/b", cwd: "/home/b", root: "/" }) ?? b;
    checks.push(
      check(
        "AC109 A seq still current after B finishes",
        isPaneLoadCurrent(a, a1.seq) && b.listed && !b.loading,
        JSON.stringify({ a, b }),
      ),
    );
    a = finishPaneLoad(a, a1.seq, { path: "/home/a", cwd: "/home/a", root: "/" }) ?? a;
    checks.push(
      check("AC109 A can finish after B", a.listed && a.path === "/home/a", JSON.stringify(a)),
    );
  }

  {
    let live = createSftpPaneLive("h1");
    const first = beginPaneLoad(live, ".");
    live = first.live;
    const second = beginPaneLoad(live, "/other");
    live = second.live;
    const stale = finishPaneLoad(live, first.seq, { path: ".", cwd: "", root: "/" });
    checks.push(
      check("AC109 stale finish ignored", stale === null && live.loadSeq === 2, JSON.stringify(live)),
    );
    live = finishPaneLoad(live, second.seq, { path: "/other", cwd: "/home", root: "/" }) ?? live;
    checks.push(check("AC109 current finish applies", live.listed && live.path === "/other", JSON.stringify(live)));
  }

  {
    let live = createSftpPaneLive("h1");
    const started = beginPaneLoad(live, ".");
    live = started.live;
    live = failPaneLoad(live, started.seq, { kind: "SftpIo", message: "boom" }) ?? live;
    checks.push(
      check(
        "AC109 failPaneLoad",
        !live.loading && live.error?.kind === "SftpIo" && !live.listed,
        JSON.stringify(live),
      ),
    );
  }

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
