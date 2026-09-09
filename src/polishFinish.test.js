/**
 * Red/Green tests for polish finish (#33): host column layout + update badge.
 */

import {
  HOST_ACTIONS_COLUMN_PX,
  HOST_STATUS_COLUMN_PX,
  hostCardGridTemplate,
} from "./hostCard.js";
import {
  settingsHasUpdateBadge,
  shouldShowFloatingUpdateToast,
} from "./updateNotify.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  {
    const ok =
      HOST_STATUS_COLUMN_PX >= 20 &&
      HOST_STATUS_COLUMN_PX <= 40 &&
      HOST_ACTIONS_COLUMN_PX >= 40 &&
      HOST_ACTIONS_COLUMN_PX <= 56;
    checks.push(
      check(
        "AC1 fixed status/actions column widths",
        ok,
        `status=${HOST_STATUS_COLUMN_PX} actions=${HOST_ACTIONS_COLUMN_PX}`,
      ),
    );
  }

  {
    const tpl = hostCardGridTemplate();
    const ok =
      typeof tpl === "string" &&
      tpl.includes(`${HOST_STATUS_COLUMN_PX}px`) &&
      tpl.includes(`${HOST_ACTIONS_COLUMN_PX}px`);
    checks.push(check("AC1 hostCardGridTemplate includes fixed columns", ok, tpl));
  }

  {
    checks.push(
      check(
        "AC4 floating update toast never used for availability",
        shouldShowFloatingUpdateToast("available") === false &&
          shouldShowFloatingUpdateToast("progress") === false,
        "",
      ),
    );
  }

  {
    const on = settingsHasUpdateBadge(true);
    const off = settingsHasUpdateBadge(false);
    checks.push(
      check(
        "AC4 settings badge flag mirrors availability",
        on === true && off === false,
        `on=${on} off=${off}`,
      ),
    );
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
