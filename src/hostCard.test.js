/**
 * Red/Green tests for Host Card view-model (#31).
 * Pure helpers — no DOM.
 */

import {
  CONNECTION_DOT_COLORS,
  CONNECTION_DOT_SIZE_PX,
  HOST_TITLE_FONT_SIZE_PX,
  HOST_TITLE_FONT_WEIGHT,
  HOST_SUBTITLE_FONT_SIZE_PX,
  connectionDotClassList,
  hostCardClassList,
  hostCardAriaStatus,
  shouldShowSessionCount,
  sessionCountLabel,
} from "./hostCard.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  // Typography tokens
  {
    const ok =
      HOST_TITLE_FONT_SIZE_PX === 13 &&
      HOST_TITLE_FONT_WEIGHT === 600 &&
      HOST_SUBTITLE_FONT_SIZE_PX >= 11 &&
      HOST_SUBTITLE_FONT_SIZE_PX <= 12;
    checks.push(
      check(
        "AC1 typography tokens (title 13/600, subtitle 11–12)",
        ok,
        `title=${HOST_TITLE_FONT_SIZE_PX}/${HOST_TITLE_FONT_WEIGHT} sub=${HOST_SUBTITLE_FONT_SIZE_PX}`,
      ),
    );
  }

  // Dot size 6–8px, colors contract
  {
    const ok =
      CONNECTION_DOT_SIZE_PX >= 6 &&
      CONNECTION_DOT_SIZE_PX <= 8 &&
      CONNECTION_DOT_COLORS.connected === "#10B981" &&
      CONNECTION_DOT_COLORS.connecting === "#F59E0B" &&
      CONNECTION_DOT_COLORS.error === "#EF4444" &&
      CONNECTION_DOT_COLORS.disconnected === "#6B7280" &&
      typeof CONNECTION_DOT_COLORS.local === "string" &&
      CONNECTION_DOT_COLORS.local.length > 0;
    checks.push(
      check("AC2 connection dot size + colors", ok, JSON.stringify({ size: CONNECTION_DOT_SIZE_PX, ...CONNECTION_DOT_COLORS })),
    );
  }

  // Dot classes: pulsed only for connecting, no halo class
  {
    const connecting = connectionDotClassList("connecting");
    const connected = connectionDotClassList("connected");
    const error = connectionDotClassList("error");
    const idle = connectionDotClassList("disconnected");
    const local = connectionDotClassList("local");
    const ok =
      connecting.includes("connection-dot") &&
      connecting.includes("is-pulsed") &&
      !connected.includes("is-pulsed") &&
      !error.includes("is-pulsed") &&
      !idle.includes("is-halo") &&
      !connecting.includes("is-halo") &&
      local.includes("connection-dot") &&
      local.includes("state-local");
    checks.push(
      check(
        "AC2 dot classes pulse only when connecting, never halo",
        ok,
        `conn=${connecting} ok=${connected} err=${error} idle=${idle} local=${local}`,
      ),
    );
  }

  // Card classes: active + open
  {
    const active = hostCardClassList({ openCount: 1, active: true, kind: "host" });
    const idle = hostCardClassList({ openCount: 0, active: false, kind: "host" });
    const local = hostCardClassList({ openCount: 2, active: true, kind: "local" });
    const ok =
      active.includes("host-card") &&
      active.includes("item") &&
      active.includes("active-host") &&
      active.includes("open") &&
      !idle.includes("active-host") &&
      !idle.includes("open") &&
      local.includes("pinned") &&
      local.includes("active-host") &&
      local.includes("open");
    checks.push(check("AC4 hostCardClassList active/open/local", ok, `a=${active} i=${idle} l=${local}`));
  }

  // Session count badge
  {
    const ok =
      shouldShowSessionCount(1) === false &&
      shouldShowSessionCount(2) === true &&
      sessionCountLabel(3) === "×3";
    checks.push(check("AC2 session count badge only when >1", ok, ""));
  }

  // A11y status labels
  {
    const labels = ["connected", "connecting", "error", "disconnected", "local"].map((s) =>
      hostCardAriaStatus(s),
    );
    const ok = labels.every((l) => typeof l === "string" && l.length > 0) && new Set(labels).size === 5;
    checks.push(check("AC5 aria status labels unique & non-empty", ok, labels.join(" | ")));
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
