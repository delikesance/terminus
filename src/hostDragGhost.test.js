/**
 * #92 — Host drag ghost helpers (pointer path).
 * AC4 hit-testing, AC5 tilt clamp + rAF cleanup.
 */

import {
  GHOST_TILT_MAX_DEG,
  clampGhostTilt,
  ghostTiltFromVelocity,
  spawnHostDragGhost,
  destroyHostDragGhost,
  tickGhostFrame,
} from "./hostDragGhost.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

/** Minimal DOM shim for spawn/destroy without jsdom. */
function makeDom() {
  const bodyChildren = [];
  const body = {
    appendChild(el) {
      bodyChildren.push(el);
      el.parentNode = body;
      return el;
    },
  };
  function makeEl(tag = "div") {
    const attrs = {};
    const classSet = new Set();
    const style = {};
    const el = {
      tagName: tag.toUpperCase(),
      style,
      dataset: {},
      parentNode: null,
      children: [],
      classList: {
        add: (...xs) => xs.forEach((x) => classSet.add(x)),
        remove: (...xs) => xs.forEach((x) => classSet.delete(x)),
        contains: (x) => classSet.has(x),
        toArray: () => [...classSet],
      },
      setAttribute(k, v) {
        attrs[k] = String(v);
      },
      getAttribute(k) {
        return attrs[k] ?? null;
      },
      removeAttribute(k) {
        delete attrs[k];
      },
      cloneNode() {
        const c = makeEl(tag);
        for (const [k, v] of Object.entries(attrs)) c.setAttribute(k, v);
        for (const cls of classSet) c.classList.add(cls);
        c.innerHTML = el.innerHTML;
        return c;
      },
      getBoundingClientRect() {
        return { width: 220, height: 40, left: 10, top: 20, right: 230, bottom: 60 };
      },
      remove() {
        const i = bodyChildren.indexOf(el);
        if (i >= 0) bodyChildren.splice(i, 1);
        el.parentNode = null;
      },
      innerHTML: "",
      textContent: "",
    };
    return el;
  }
  return {
    body,
    bodyChildren,
    makeEl,
    document: {
      body,
      createElement: (tag) => makeEl(tag),
    },
  };
}

function runTests() {
  const checks = [];

  // AC5 — tilt max constant
  {
    const ok = GHOST_TILT_MAX_DEG === 15;
    checks.push(check("AC5 GHOST_TILT_MAX_DEG is 15", ok, { GHOST_TILT_MAX_DEG }));
  }

  // AC5 — clamp
  {
    const ok =
      clampGhostTilt(0) === 0 &&
      clampGhostTilt(40) === 15 &&
      clampGhostTilt(-40) === -15 &&
      clampGhostTilt(12) === 12 &&
      clampGhostTilt(-12) === -12;
    checks.push(
      check("AC5 clampGhostTilt bounds to ±15", ok, {
        hi: clampGhostTilt(40),
        lo: clampGhostTilt(-40),
      }),
    );
  }

  // AC2/AC5 — velocity → tilt, always clamped
  {
    const soft = ghostTiltFromVelocity(0.2);
    const hard = ghostTiltFromVelocity(50);
    const neg = ghostTiltFromVelocity(-50);
    const ok =
      typeof soft === "number" &&
      Math.abs(soft) <= GHOST_TILT_MAX_DEG &&
      hard === GHOST_TILT_MAX_DEG &&
      neg === -GHOST_TILT_MAX_DEG &&
      ghostTiltFromVelocity(0) === 0;
    checks.push(check("AC2/AC5 ghostTiltFromVelocity clamps extreme vx", ok, { soft, hard, neg }));
  }

  // AC1/AC4 — spawn: class, pointer-events none, attached to body
  {
    const dom = makeDom();
    const source = dom.makeEl("div");
    source.classList.add("host-card", "item");
    source.setAttribute("data-host", "h1");
    source.setAttribute("data-testid", "host-h1");
    source.innerHTML = "<span>box</span>";
    const state = spawnHostDragGhost(source, 100, 50, { document: dom.document });
    const ghost = state.el;
    const pe = ghost.style.pointerEvents || ghost.style["pointer-events"];
    const ok =
      !!ghost &&
      ghost.classList.contains("host-drag-ghost") &&
      pe === "none" &&
      dom.bodyChildren.includes(ghost) &&
      ghost.getAttribute("data-testid") === null &&
      state.raf === null;
    checks.push(
      check("AC1/AC4 spawnHostDragGhost: class + pointer-events none + no testid", ok, {
        pe,
        classes: ghost?.classList?.toArray?.(),
        inBody: dom.bodyChildren.includes(ghost),
      }),
    );
    destroyHostDragGhost(state);
  }

  // AC3/AC5 — destroy removes node and cancels rAF
  {
    const dom = makeDom();
    const source = dom.makeEl("div");
    source.classList.add("host-card");
    const state = spawnHostDragGhost(source, 0, 0, { document: dom.document });
    let cancelled = false;
    state.raf = 42;
    const prevCancel = globalThis.cancelAnimationFrame;
    globalThis.cancelAnimationFrame = (id) => {
      if (id === 42) cancelled = true;
    };
    destroyHostDragGhost(state);
    globalThis.cancelAnimationFrame = prevCancel;
    const ok = cancelled && !dom.bodyChildren.includes(state.el) && state.raf === null;
    checks.push(check("AC3/AC5 destroyHostDragGhost removes ghost and cancels rAF", ok, { cancelled }));
  }

  // AC5 — tick updates position + tilt via CSS var, keeps clamp
  {
    const dom = makeDom();
    const source = dom.makeEl("div");
    const state = spawnHostDragGhost(source, 10, 10, { document: dom.document });
    state.x = 80;
    state.y = 40;
    state.lastX = 10;
    state.lastT = 0;
    state.tiltDeg = 0;
    tickGhostFrame(state, { now: 16, pointerX: 80, pointerY: 40 });
    const tilt = Number.parseFloat(String(state.el.style.getPropertyValue?.("--ghost-tilt") ?? state.el.style["--ghost-tilt"] ?? state.tiltDeg));
    const ok =
      Math.abs(state.tiltDeg) <= GHOST_TILT_MAX_DEG &&
      (state.el.style.transform || "").includes("translate") &&
      Number.isFinite(state.tiltDeg);
    checks.push(
      check("AC5 tickGhostFrame writes transform and bounded tilt", ok, {
        tiltDeg: state.tiltDeg,
        transform: state.el.style.transform,
        tiltVar: tilt,
      }),
    );
    destroyHostDragGhost(state);
  }

  // destroy(null) is safe
  {
    let threw = false;
    try {
      destroyHostDragGhost(null);
    } catch {
      threw = true;
    }
    checks.push(check("AC3 destroyHostDragGhost(null) is a no-op", !threw, {}));
  }

  let failed = 0;
  for (const c of checks) {
    const mark = c.ok ? "ok" : "FAIL";
    if (!c.ok) failed += 1;
    console.log(`${mark}  ${c.name}`, c.detail ?? "");
  }
  console.log(failed ? `\n${failed}/${checks.length} failed` : `\n${checks.length} passed`);
  process.exit(failed ? 1 : 0);
}

runTests();
