/**
 * Port Forwarding panel UX tests (#35 + #37 compact form).
 */

import {
  applyToggleFailure,
  applyToggleSuccess,
  buildForwardRows,
  compactForwardLayout,
  compactForwardTabOrder,
  createForwardUiState,
  formatForwardSubtitle,
  forwardAddBtnMode,
  shouldCloseCreateFormOnEscape,
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

  // #37 — compact tab order (mapping row then SSH + actions)
  {
    const order = compactForwardTabOrder();
    const expected = [
      "fwd-local-port",
      "fwd-remote-host",
      "fwd-remote-port",
      "fwd-host",
      "fwd-form-cancel",
      "fwd-form-submit",
    ];
    const ok = JSON.stringify(order) === JSON.stringify(expected);
    checks.push(check("AC5 compactForwardTabOrder sequential", ok, JSON.stringify(order)));
  }

  // #37 — Escape closes only when form open
  {
    const open = createForwardUiState({ createOpen: true });
    const closed = createForwardUiState({ createOpen: false });
    const a = shouldCloseCreateFormOnEscape("Escape", open.createOpen) === true;
    const b = shouldCloseCreateFormOnEscape("Escape", closed.createOpen) === false;
    const c = shouldCloseCreateFormOnEscape("Enter", open.createOpen) === false;
    checks.push(check("AC4 shouldCloseCreateFormOnEscape", a && b && c, ""));
  }

  // #37 — add button mode flips when form open
  {
    const add = forwardAddBtnMode(false);
    const close = forwardAddBtnMode(true);
    checks.push(
      check(
        "AC3 forwardAddBtnMode add vs close",
        add === "add" && close === "close",
        `${add}/${close}`,
      ),
    );
  }

  // #39 — compact layout tokens for CSS polish
  {
    const L = compactForwardLayout;
    const ok =
      L.sshSelectMinWidthPx === 140 &&
      L.portFieldMinWidthPx === 60 &&
      L.actionsGapPx === 8 &&
      L.searchPaddingRightPx >= 16;
    checks.push(check("AC #39 compactForwardLayout tokens", ok, JSON.stringify(L)));
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
