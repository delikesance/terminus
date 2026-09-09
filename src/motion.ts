/**
 * Clean motion helpers (#79) — tokens + overlay open/close phase helpers.
 * Keep in sync with CSS custom properties in styles.css.
 */

export const MOTION_FAST_MS = 120;
export const MOTION_BASE_MS = 160;
export const MOTION_SLOW_MS = 220;

export const EASE_OUT = "cubic-bezier(0.22, 1, 0.36, 1)";
export const EASE_SPRING = "cubic-bezier(0.34, 1.3, 0.64, 1)";

export type OverlayPhase = "closed" | "opening" | "open" | "closing";

type Classy = {
  classList: {
    add: (...tokens: string[]) => void;
    remove: (...tokens: string[]) => void;
    contains: (token: string) => boolean;
  };
};

export function prefersReducedMotion(mq?: { matches: boolean }): boolean {
  if (mq) return mq.matches;
  if (typeof matchMedia === "undefined") return false;
  return matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function motionDurationMs(baseMs: number, reduced?: boolean): number {
  return (reduced ?? prefersReducedMotion()) ? 0 : baseMs;
}

export function nextOverlayPhase(phase: OverlayPhase, event: "show" | "hide" | "opened" | "closed"): OverlayPhase {
  switch (event) {
    case "show":
      return "opening";
    case "opened":
      return phase === "opening" || phase === "open" ? "open" : phase;
    case "hide":
      return phase === "closed" ? "closed" : "closing";
    case "closed":
      return "closed";
  }
}

/** Remove hide classes and either prep for enter animation or open instantly. */
export function prepareOverlayOpen(el: Classy, reduced: boolean): void {
  el.classList.remove("hidden", "motion-out");
  if (reduced) {
    el.classList.remove("motion-prep");
    el.classList.add("motion-open");
    return;
  }
  el.classList.remove("motion-open");
  el.classList.add("motion-prep");
}

/**
 * Start close: either hide immediately (reduced) or swap to motion-out.
 * Caller waits for transition then calls finishOverlayClose.
 */
export function beginOverlayClose(el: Classy, reduced: boolean): "instant" | "animate" {
  if (reduced) {
    el.classList.add("hidden");
    el.classList.remove("motion-open", "motion-out", "motion-prep");
    return "instant";
  }
  el.classList.remove("motion-open", "motion-prep");
  el.classList.add("motion-out");
  return "animate";
}

export function finishOverlayClose(el: Classy): void {
  el.classList.add("hidden");
  el.classList.remove("motion-open", "motion-out", "motion-prep");
}
