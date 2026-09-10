/**
 * #101 — Multi-remote SFTP session helpers.
 */

import {
  createSftpSessions,
  openOrFocusRemote,
  rememberRemote,
  closeRemote,
  openRemoteIds,
  hasRemote,
  defaultSessionSnap,
} from "./sftpSessions.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  {
    const s0 = createSftpSessions();
    const s1 = openOrFocusRemote(s0, "a");
    const s2 = openOrFocusRemote(s1, "b");
    const ok =
      openRemoteIds(s2).join(",") === "a,b" &&
      hasRemote(s2, "a") &&
      hasRemote(s2, "b") &&
      s2.byId.a.path === "." &&
      s2.byId.b.path === ".";
    checks.push(check("AC1 openOrFocusRemote parks both A and B", ok, openRemoteIds(s2)));
  }

  {
    let s = openOrFocusRemote(createSftpSessions(), "a");
    s = rememberRemote(s, "a", { path: "/home/a", cwd: "/home/a", root: "/", selected: ["/home/a/x"] });
    s = openOrFocusRemote(s, "b");
    s = rememberRemote(s, "b", { path: "/var", cwd: "/var", root: "/", selected: [] });
    const ok =
      s.byId.a.path === "/home/a" &&
      s.byId.a.selected[0] === "/home/a/x" &&
      s.byId.b.path === "/var" &&
      openRemoteIds(s).join(",") === "a,b";
    checks.push(check("AC2 rememberRemote preserves A while B is remembered", ok, s.byId));
  }

  {
    let s = openOrFocusRemote(createSftpSessions(), "a");
    s = openOrFocusRemote(s, "b");
    const closed = closeRemote(s, "a", "b");
    const ok =
      !hasRemote(closed.sessions, "a") &&
      hasRemote(closed.sessions, "b") &&
      closed.nextActive === "b";
    checks.push(check("AC3 closeRemote removes A and keeps B active", ok, closed));
  }

  {
    let s = openOrFocusRemote(createSftpSessions(), "a");
    s = openOrFocusRemote(s, "b");
    const closed = closeRemote(s, "b", "b");
    const ok = closed.nextActive === "a" && openRemoteIds(closed.sessions).join(",") === "a";
    checks.push(check("AC3 close active B falls back to A", ok, closed));
  }

  {
    let s = openOrFocusRemote(createSftpSessions(), "a");
    s = openOrFocusRemote(s, "a");
    const ok = openRemoteIds(s).join(",") === "a" && Object.keys(s.byId).length === 1;
    checks.push(check("re-open same host does not duplicate", ok, openRemoteIds(s)));
  }

  {
    const d = defaultSessionSnap();
    const ok = d.path === "." && d.root === "/" && Array.isArray(d.selected);
    checks.push(check("defaultSessionSnap shape", ok, d));
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
