/**
 * Red/Green tests for Port Forwarding panel UX (#35).
 */

import {
  applyToggleFailure,
  applyToggleSuccess,
  buildForwardRows,
  createForwardUiState,
  formatForwardSubtitle,
  shouldShowHostFooterActions,
  shouldShowCreateForm,
  toggleCreateForm,
  toggleActionFromChecked,
  validateForwardForm,
} from "./forwardPanel.js";
import { clickActivity, createNavState } from "./activityBar.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  // Existing smoke from #28
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
      ],
      new Set(["a"]),
    );
    checks.push(
      check("buildForwardRows marks running", rows[0].active === true && rows[0].state === "running", ""),
    );
  }

  // #35 — create form closed by default
  {
    const s = createForwardUiState();
    checks.push(
      check(
        "AC1 create form closed by default",
        s.createOpen === false && shouldShowCreateForm(s) === false,
        JSON.stringify(s),
      ),
    );
  }

  // #35 — toggle open/close
  {
    let s = createForwardUiState();
    s = toggleCreateForm(s);
    const opened = shouldShowCreateForm(s) === true;
    s = toggleCreateForm(s);
    const closed = shouldShowCreateForm(s) === false;
    checks.push(check("AC2 toggleCreateForm opens then closes", opened && closed, JSON.stringify(s)));
  }

  // #35 — subtitle mapping
  {
    const sub = formatForwardSubtitle({
      bindHost: "127.0.0.1",
      bindPort: 15432,
      destHost: "127.0.0.1",
      destPort: 5432,
      sshLabel: "db-bastion",
    });
    const ok = sub === "localhost:15432 → 127.0.0.1:5432 via db-bastion";
    checks.push(check("AC3 formatForwardSubtitle directional mapping", ok, sub));
  }

  // #35 — footer host actions only on hosts activity
  {
    let nav = createNavState();
    const onHosts = shouldShowHostFooterActions(nav.active) === true;
    nav = clickActivity(nav, "forwards");
    const onForwards = shouldShowHostFooterActions(nav.active) === false;
    checks.push(
      check("AC5 host footer hidden on forwards, shown on hosts", onHosts && onForwards, nav.active),
    );
  }

  // validate still works
  {
    const r = validateForwardForm({
      hostId: "h1",
      localPort: "8080",
      remoteHost: "127.0.0.1",
      remotePort: "80",
    });
    checks.push(check("validateForwardForm happy path", r.ok === true, JSON.stringify(r)));
  }

  {
    const on = toggleActionFromChecked(true);
    const off = toggleActionFromChecked(false);
    let running = applyToggleSuccess(new Set(), "x", "start");
    running = applyToggleFailure(running, "y", "start");
    checks.push(check("toggle helpers still work", on === "start" && off === "stop" && !running.has("y"), ""));
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
