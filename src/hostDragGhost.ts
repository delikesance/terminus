/**
 * Host drag ghost + pendulum tilt (#92) — pointer-path only.
 * Pure helpers + DOM spawn/destroy; frame loop driven by caller via tickGhostFrame / scheduleGhostFrame.
 */

export const GHOST_TILT_MAX_DEG = 15;

/** Degrees per (px/ms) of horizontal velocity — tuned so moderate flicks reach near max. */
export const GHOST_TILT_VEL_SCALE = 0.45;

export type GhostState = {
  el: HTMLElement;
  raf: number | null;
  x: number;
  y: number;
  lastX: number;
  lastT: number;
  tiltDeg: number;
  /** Latest pointer sample written from pointermove (read on rAF). */
  pointerX: number;
  pointerY: number;
  alive: boolean;
};

export type SpawnOpts = {
  document?: Document;
};

export function clampGhostTilt(deg: number): number {
  if (deg > GHOST_TILT_MAX_DEG) return GHOST_TILT_MAX_DEG;
  if (deg < -GHOST_TILT_MAX_DEG) return -GHOST_TILT_MAX_DEG;
  return deg;
}

export function ghostTiltFromVelocity(vxPxPerMs: number, scale = GHOST_TILT_VEL_SCALE): number {
  return clampGhostTilt(vxPxPerMs * scale);
}

function applyGhostVisual(state: GhostState): void {
  const { el, x, y, tiltDeg } = state;
  const halfW = (el.getBoundingClientRect?.().width || el.offsetWidth || 0) / 2 || 110;
  const halfH = (el.getBoundingClientRect?.().height || el.offsetHeight || 0) / 2 || 20;
  el.style.transform = `translate3d(${x - halfW}px, ${y - halfH}px, 0) rotate(${tiltDeg}deg)`;
  el.style.setProperty?.("--ghost-tilt", `${tiltDeg}deg`);
  // Test shims / older style bags
  (el.style as unknown as Record<string, string>)["--ghost-tilt"] = `${tiltDeg}deg`;
}

export function spawnHostDragGhost(
  source: HTMLElement,
  clientX: number,
  clientY: number,
  opts: SpawnOpts = {},
): GhostState {
  const doc = opts.document ?? document;
  const ghost = source.cloneNode(true) as HTMLElement;
  ghost.removeAttribute("id");
  ghost.removeAttribute("data-testid");
  ghost.removeAttribute("draggable");
  ghost.removeAttribute("tabindex");
  ghost.classList.add("host-drag-ghost");
  ghost.style.pointerEvents = "none";
  ghost.style.position = "fixed";
  ghost.style.left = "0";
  ghost.style.top = "0";
  ghost.style.zIndex = "10000";
  ghost.style.margin = "0";
  ghost.style.boxSizing = "border-box";
  const rect = source.getBoundingClientRect();
  if (rect.width > 0) ghost.style.width = `${rect.width}px`;

  doc.body.appendChild(ghost);

  const state: GhostState = {
    el: ghost,
    raf: null,
    x: clientX,
    y: clientY,
    lastX: clientX,
    lastT: typeof performance !== "undefined" ? performance.now() : 0,
    tiltDeg: 0,
    pointerX: clientX,
    pointerY: clientY,
    alive: true,
  };
  applyGhostVisual(state);
  return state;
}

/**
 * One physics/visual step. Caller supplies `now` + pointer sample (or uses state.pointer*).
 */
export function tickGhostFrame(
  state: GhostState,
  sample?: { now: number; pointerX: number; pointerY: number },
): void {
  if (!state.alive) return;
  const now = sample?.now ?? (typeof performance !== "undefined" ? performance.now() : state.lastT);
  const px = sample?.pointerX ?? state.pointerX;
  const py = sample?.pointerY ?? state.pointerY;
  const dt = Math.max(now - state.lastT, 1);
  const vx = (px - state.lastX) / dt;
  const targetTilt = ghostTiltFromVelocity(vx);
  // Light smoothing toward target (pendulum / gravity feel)
  state.tiltDeg = clampGhostTilt(state.tiltDeg * 0.65 + targetTilt * 0.35);
  state.x = px;
  state.y = py;
  state.lastX = px;
  state.lastT = now;
  state.pointerX = px;
  state.pointerY = py;
  applyGhostVisual(state);
}

/** Start/continue rAF loop that follows state.pointerX/Y until destroy. */
export function scheduleGhostFrame(state: GhostState): void {
  if (!state.alive || state.raf != null) return;
  const loop = () => {
    if (!state.alive) {
      state.raf = null;
      return;
    }
    tickGhostFrame(state);
    state.raf = requestAnimationFrame(loop);
  };
  state.raf = requestAnimationFrame(loop);
}

export function destroyHostDragGhost(state: GhostState | null): void {
  if (!state) return;
  state.alive = false;
  if (state.raf != null) {
    cancelAnimationFrame(state.raf);
    state.raf = null;
  }
  try {
    state.el.remove();
  } catch {
    /* ignore */
  }
}
