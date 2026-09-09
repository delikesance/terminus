/**
 * Red/Green tests for Activity Bar + contextual sidebar navigation (#27 / #29).
 * Pure state machine — no DOM.
 */

import {
  ACTIVITIES,
  ACTIVITY_BAR_WIDTH_PX,
  ACTIVITY_BAR_ITEM_SIZE_PX,
  ACTIVITY_BAR_TOP_PADDING_PX,
  SEARCH_INPUT_HEIGHT_PX,
  SEARCH_PAD_TOP_PX,
  TERMINAL_PANE_PADDING_X_PX,
  TERMINAL_PANE_PADDING_Y_PX,
  activityBarFirstIconCenterY,
  clickActivity,
  createNavState,
  isPanelVisible,
  searchInputCenterY,
  toggleSidebarOpen,
} from "./activityBar.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  // AC1 — Activity Bar contract width + known activities
  {
    const ok =
      ACTIVITY_BAR_WIDTH_PX === 48 &&
      Array.isArray(ACTIVITIES) &&
      ACTIVITIES.includes("hosts") &&
      ACTIVITIES.includes("snippets") &&
      ACTIVITIES.includes("history") &&
      ACTIVITIES.includes("forwards") &&
      ACTIVITIES.includes("sftp") &&
      ACTIVITIES.length === 5;
    checks.push(
      check(
        "AC1 activity bar is 48px with five activities",
        ok,
        `width=${ACTIVITY_BAR_WIDTH_PX} activities=${JSON.stringify(ACTIVITIES)}`,
      ),
    );
  }

  // #29 — terminal pane padding contract
  {
    const ok = TERMINAL_PANE_PADDING_Y_PX === 16 && TERMINAL_PANE_PADDING_X_PX === 20;
    checks.push(
      check(
        "#29 terminal pane padding is 16px 20px",
        ok,
        `y=${TERMINAL_PANE_PADDING_Y_PX} x=${TERMINAL_PANE_PADDING_X_PX}`,
      ),
    );
  }

  // #29 — first icon center aligns with search input center (layout tokens)
  {
    const iconY = activityBarFirstIconCenterY();
    const searchY = searchInputCenterY();
    const ok =
      ACTIVITY_BAR_ITEM_SIZE_PX === 36 &&
      SEARCH_PAD_TOP_PX === 14 &&
      SEARCH_INPUT_HEIGHT_PX === 32 &&
      ACTIVITY_BAR_TOP_PADDING_PX === 12 &&
      iconY === searchY &&
      Math.abs(iconY - searchY) <= 2;
    checks.push(
      check(
        "#29 first activity icon center aligns with search",
        ok,
        `iconY=${iconY} searchY=${searchY} topPad=${ACTIVITY_BAR_TOP_PADDING_PX}`,
      ),
    );
  }

  // Default: hosts active, sidebar open
  {
    const s = createNavState();
    const ok = s.active === "hosts" && s.sidebarOpen === true && isPanelVisible(s, "hosts");
    checks.push(check("default nav is hosts + sidebar open", ok, JSON.stringify(s)));
  }

  // AC2 — switch activity shows only that panel and keeps sidebar open
  {
    let s = createNavState();
    s = clickActivity(s, "snippets");
    const ok =
      s.active === "snippets" &&
      s.sidebarOpen === true &&
      isPanelVisible(s, "snippets") &&
      !isPanelVisible(s, "hosts") &&
      !isPanelVisible(s, "history");
    checks.push(check("AC2 switch to snippets opens that panel only", ok, JSON.stringify(s)));
  }

  // AC3 — re-click active collapses sidebar, keeps activity
  {
    let s = createNavState({ active: "hosts", sidebarOpen: true });
    s = clickActivity(s, "hosts");
    const ok =
      s.active === "hosts" &&
      s.sidebarOpen === false &&
      !isPanelVisible(s, "hosts");
    checks.push(check("AC3 re-click active collapses sidebar", ok, JSON.stringify(s)));
  }

  // AC4 — click other activity from collapsed reopens sidebar
  {
    let s = createNavState({ active: "hosts", sidebarOpen: false });
    s = clickActivity(s, "history");
    const ok =
      s.active === "history" &&
      s.sidebarOpen === true &&
      isPanelVisible(s, "history");
    checks.push(check("AC4 switch from collapsed reopens sidebar", ok, JSON.stringify(s)));
  }

  // AC6 — titlebar toggle does not change activity
  {
    let s = createNavState({ active: "forwards", sidebarOpen: true });
    s = toggleSidebarOpen(s);
    const mid = s.sidebarOpen === false && s.active === "forwards";
    s = toggleSidebarOpen(s, true);
    const ok = mid && s.sidebarOpen === true && s.active === "forwards";
    checks.push(check("AC6 titlebar toggle preserves activity", ok, JSON.stringify(s)));
  }

  // Invalid activity is a no-op
  {
    const before = createNavState({ active: "sftp", sidebarOpen: true });
    const after = clickActivity(before, /** @type {any} */ ("nope"));
    const ok = after.active === "sftp" && after.sidebarOpen === true;
    checks.push(check("invalid activity is a no-op", ok, JSON.stringify(after)));
  }

  // Programmatic SFTP entry: set activity + force open
  {
    let s = createNavState({ active: "hosts", sidebarOpen: false });
    s = clickActivity(s, "sftp");
    const ok = s.active === "sftp" && s.sidebarOpen === true && isPanelVisible(s, "sftp");
    checks.push(check("SFTP entry activates sftp and opens sidebar", ok, JSON.stringify(s)));
  }

  return checks;
}

const checks = runTests();
const failed = checks.filter((c) => !c.ok);
for (const c of checks) {
  console.log(`${c.ok ? "PASS" : "FAIL"} ${c.name}${c.detail ? ` — ${c.detail}` : ""}`);
}
if (failed.length) {
  console.error(`\n${failed.length}/${checks.length} failed`);
  process.exit(1);
}
console.log(`\n${checks.length}/${checks.length} passed`);
