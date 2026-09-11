/**
 * Terminal paste coordination (#141 / #143).
 * Multiple paths can fire for one gesture (keydown, keybinding, DOM paste).
 * Only the first delivery within PASTE_SUPPRESS_MS should reach the PTY.
 */

export const PASTE_SUPPRESS_MS = 100;

/** Absolute timestamp until which further paste deliveries should be ignored. */
export function nextPasteSuppressUntil(now = Date.now()): number {
  return now + PASTE_SUPPRESS_MS;
}

export function shouldIgnorePaste(suppressUntil: number, now = Date.now()): boolean {
  return now < suppressUntil;
}

/** @deprecated use shouldIgnorePaste — kept for call-site clarity in DOM path */
export function shouldIgnoreDomPaste(suppressUntil: number, now = Date.now()): boolean {
  return shouldIgnorePaste(suppressUntil, now);
}

/**
 * Decide what a DOM `paste` handler should do after always calling preventDefault.
 * - ignore: another path already handled this gesture
 * - send: use clipboardData text (paste without keydown)
 * - fallback: empty clipboardData — caller should use Tauri pasteIntoPane
 */
export function decideDomPasteAction(opts: {
  suppressUntil: number;
  now?: number;
  clipboardText: string;
}): "ignore" | "fallback" | { send: string } {
  const now = opts.now ?? Date.now();
  if (shouldIgnorePaste(opts.suppressUntil, now)) return "ignore";
  if (opts.clipboardText) return { send: opts.clipboardText };
  return "fallback";
}

/**
 * Whether a keydown / keybinding paste should run.
 * Returns the new suppressUntil when it should deliver; null when skipped.
 */
export function claimPasteDelivery(suppressUntil: number, now = Date.now()): number | null {
  if (shouldIgnorePaste(suppressUntil, now)) return null;
  return nextPasteSuppressUntil(now);
}
