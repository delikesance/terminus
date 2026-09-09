/**
 * Red/Green tests for Port Forwarding sidebar panel (#28).
 * Pure view-model — no DOM.
 */

import {
  applyToggleFailure,
  applyToggleSuccess,
  buildForwardRows,
  toggleActionFromChecked,
  validateForwardForm,
} from "./forwardPanel.js";
import { clickActivity, createNavState, isPanelVisible } from "./activityBar.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  // View transition — Activity Bar → forwards panel visible
  {
    let s = createNavState();
    s = clickActivity(s, "forwards");
    const ok =
      s.active === "forwards" &&
      s.sidebarOpen === true &&
      isPanelVisible(s, "forwards") &&
      !isPanelVisible(s, "hosts");
    checks.push(
      check("view transition: forwards activity shows only forwards panel", ok, JSON.stringify(s)),
    );
  }

  // AC2 — list rows reflect active/inactive from running set
  {
    const rows = buildForwardRows(
      [
        {
          id: "a",
          name: "A",
          bind_host: "127.0.0.1",
          bind_port: 8080,
          dest_host: "127.0.0.1",
          dest_port: 80,
          host_id: "h1",
        },
        {
          id: "b",
          name: "B",
          bind_host: "127.0.0.1",
          bind_port: 9090,
          dest_host: "db",
          dest_port: 5432,
          host_id: "h1",
        },
      ],
      new Set(["a"]),
    );
    const ok =
      rows.length === 2 &&
      rows[0].id === "a" &&
      rows[0].active === true &&
      rows[0].state === "running" &&
      rows[1].id === "b" &&
      rows[1].active === false &&
      rows[1].state === "stopped" &&
      rows[0].bindPort === 8080 &&
      rows[1].destHost === "db";
    checks.push(check("AC2 buildForwardRows marks active/inactive", ok, JSON.stringify(rows)));
  }

  // Toggle intent: checked → start, unchecked → stop
  {
    const on = toggleActionFromChecked(true);
    const off = toggleActionFromChecked(false);
    checks.push(
      check(
        "toggle ON maps to start, OFF to stop",
        on === "start" && off === "stop",
        `on=${on} off=${off}`,
      ),
    );
  }

  // Apply toggle success mutates running set
  {
    let running = new Set();
    running = applyToggleSuccess(running, "x", "start");
    const started = running.has("x");
    running = applyToggleSuccess(running, "x", "stop");
    const stopped = !running.has("x");
    checks.push(
      check("applyToggleSuccess start then stop", started && stopped, [...running].join(",")),
    );
  }

  // Start failure keeps forward stopped
  {
    let running = new Set();
    running = applyToggleFailure(running, "x", "start");
    checks.push(
      check("applyToggleFailure on start keeps stopped", !running.has("x"), [...running].join(",")),
    );
  }

  // Form validation — happy path (local port / remote host / remote port)
  {
    const r = validateForwardForm({
      hostId: "h1",
      localPort: "15432",
      remoteHost: "127.0.0.1",
      remotePort: "5432",
    });
    const ok =
      r.ok === true &&
      r.bindPort === 15432 &&
      r.destHost === "127.0.0.1" &&
      r.destPort === 5432 &&
      r.bindHost === "127.0.0.1" &&
      r.hostId === "h1" &&
      typeof r.name === "string" &&
      r.name.length > 0;
    checks.push(check("validateForwardForm accepts valid ports", ok, JSON.stringify(r)));
  }

  // Form validation — invalid local port
  {
    const r = validateForwardForm({
      hostId: "h1",
      localPort: "0",
      remoteHost: "127.0.0.1",
      remotePort: "80",
    });
    checks.push(
      check("validateForwardForm rejects invalid local port", r.ok === false && !!r.error, JSON.stringify(r)),
    );
  }

  // Form validation — missing host
  {
    const r = validateForwardForm({
      hostId: "",
      localPort: 8080,
      remoteHost: "127.0.0.1",
      remotePort: 80,
    });
    checks.push(
      check("validateForwardForm rejects missing SSH host", r.ok === false && !!r.error, JSON.stringify(r)),
    );
  }

  // Form validation — empty remote host
  {
    const r = validateForwardForm({
      hostId: "h1",
      localPort: 8080,
      remoteHost: "  ",
      remotePort: 80,
    });
    checks.push(
      check("validateForwardForm rejects blank remote host", r.ok === false && !!r.error, JSON.stringify(r)),
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
