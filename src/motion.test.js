/**
 * #79 — Clean motion system (tokens + overlay phase helpers).
 * Pure helpers — no real DOM required.
 */

import {
  MOTION_FAST_MS,
  MOTION_BASE_MS,
  MOTION_SLOW_MS,
  EASE_OUT,
  EASE_SPRING,
  prefersReducedMotion,
  motionDurationMs,
  nextOverlayPhase,
  beginOverlayClose,
  prepareOverlayOpen,
} from "./motion.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function fakeEl(initial = []) {
  const set = new Set(initial);
  return {
    classList: {
      add: (...xs) => xs.forEach((x) => set.add(x)),
      remove: (...xs) => xs.forEach((x) => set.delete(x)),
      contains: (x) => set.has(x),
      toggle: (x, force) => {
        if (force === true) set.add(x);
        else if (force === false) set.delete(x);
        else if (set.has(x)) set.delete(x);
        else set.add(x);
      },
      toArray: () => [...set],
    },
  };
}

function runTests() {
  const checks = [];

  // AC1 — token durations & easings
  {
    const ok =
      MOTION_FAST_MS === 120 &&
      MOTION_BASE_MS === 160 &&
      MOTION_SLOW_MS === 220 &&
      typeof EASE_OUT === "string" &&
      EASE_OUT.includes("cubic-bezier") &&
      typeof EASE_SPRING === "string" &&
      EASE_SPRING.includes("cubic-bezier");
    checks.push(check("AC1 tokens: fast/base/slow + easings", ok, { MOTION_FAST_MS, MOTION_BASE_MS, MOTION_SLOW_MS, EASE_OUT, EASE_SPRING }));
  }

  // AC1 — reduced motion zeroes durations
  {
    const ok =
      prefersReducedMotion({ matches: true }) === true &&
      prefersReducedMotion({ matches: false }) === false &&
      motionDurationMs(MOTION_BASE_MS, true) === 0 &&
      motionDurationMs(MOTION_BASE_MS, false) === MOTION_BASE_MS;
    checks.push(check("AC1 reduced-motion zeros duration", ok, {}));
  }

  // AC2 — overlay open prep removes hidden / motion-out
  {
    const el = fakeEl(["hidden", "motion-out"]);
    prepareOverlayOpen(el, false);
    const cls = el.classList.toArray().sort();
    const ok = !cls.includes("hidden") && !cls.includes("motion-out") && cls.includes("motion-prep");
    checks.push(check("AC2 prepareOverlayOpen clears hidden and sets prep", ok, { cls }));
  }

  // AC2 — reduced open is instant (motion-open, no prep)
  {
    const el = fakeEl(["hidden"]);
    prepareOverlayOpen(el, true);
    const cls = el.classList.toArray().sort();
    const ok = !cls.includes("hidden") && cls.includes("motion-open") && !cls.includes("motion-prep");
    checks.push(check("AC2 prepareOverlayOpen reduced → instant open", ok, { cls }));
  }

  // AC2 — close animate vs instant
  {
    const anim = fakeEl(["motion-open"]);
    const modeA = beginOverlayClose(anim, false);
    const clsA = anim.classList.toArray().sort();
    const instant = fakeEl(["motion-open"]);
    const modeB = beginOverlayClose(instant, true);
    const clsB = instant.classList.toArray().sort();
    const ok =
      modeA === "animate" &&
      clsA.includes("motion-out") &&
      !clsA.includes("motion-open") &&
      modeB === "instant" &&
      clsB.includes("hidden") &&
      !clsB.includes("motion-open");
    checks.push(check("AC2 beginOverlayClose animate vs instant", ok, { modeA, clsA, modeB, clsB }));
  }

  // Overlay phase state machine
  {
    const ok =
      nextOverlayPhase("closed", "show") === "opening" &&
      nextOverlayPhase("opening", "opened") === "open" &&
      nextOverlayPhase("open", "hide") === "closing" &&
      nextOverlayPhase("closing", "closed") === "closed" &&
      nextOverlayPhase("closing", "show") === "opening";
    checks.push(check("overlay phase machine", ok, {}));
  }

  const failed = checks.filter((c) => !c.ok);
  for (const c of checks) {
    console.log(`${c.ok ? "ok" : "FAIL"}  ${c.name}${c.ok ? "" : " — " + JSON.stringify(c.detail)}`);
  }
  if (failed.length) {
    console.error(`\n${failed.length}/${checks.length} failed`);
    process.exit(1);
  }
  console.log(`\n${checks.length}/${checks.length} passed`);
}

runTests();
