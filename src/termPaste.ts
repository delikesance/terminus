/**
 * Terminal paste coordination (#141 / #143).
 * Multiple paths can fire for one gesture (keydown, keybinding, DOM paste).
 * Only the first delivery within PASTE_SUPPRESS_MS should reach the PTY.
 */

export const PASTE_SUPPRESS_MS = 100;

/** Bit 3 in terminal mode_flags indicates bracketed paste mode (DECSET 2004). */
export const MODE_BRACKETED_PASTE = 0b1000;

/** Check whether bracketed paste mode (DECSET 2004) is enabled in modeFlags. */
export function isBracketedPaste(modeFlags?: number): boolean {
  return Boolean(modeFlags && (modeFlags & MODE_BRACKETED_PASTE));
}

/**
 * Format text for delivery to a terminal PTY.
 * - Strips any bracketed paste escape sequences to avoid breakout injection.
 * - Strips at most one trailing newline to avoid unintended automatic execution.
 * - Normalizes remaining newlines (\r\n or \n) to carriage return (\r), the standard line break for PTY input.
 * - Wraps in \x1b[200~ ... \x1b[201~ if the application has bracketed paste mode enabled.
 */
export function formatTerminalPaste(
  text: string,
  modeFlags?: number,
  options?: { stripTrailingNewline?: boolean },
): string {
  if (!text) return "";
  const stripTrailing = options?.stripTrailingNewline ?? true;
  // Prevent bracketed paste injection by stripping existing markers
  let sanitized = text.replace(/\x1b\[20[01]~/g, "");
  if (stripTrailing) {
    sanitized = sanitized.replace(/(\r\n|\r|\n)$/, "");
  }
  const normalized = sanitized.replace(/\r\n/g, "\r").replace(/\n/g, "\r");
  if (isBracketedPaste(modeFlags)) {
    return `\x1b[200~${normalized}\x1b[201~`;
  }
  return normalized;
}

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

