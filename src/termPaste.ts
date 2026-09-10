/**
 * Terminal paste coordination (#141).
 * Ctrl/Cmd+V uses Tauri clipboard via pasteIntoPane; the DOM `paste` event
 * often still fires afterward (WebView2/WebKit) and must not double-insert.
 */

export const PASTE_SUPPRESS_MS = 100;

/** Absolute timestamp until which DOM paste events should be ignored. */
export function nextPasteSuppressUntil(now = Date.now()): number {
  return now + PASTE_SUPPRESS_MS;
}

export function shouldIgnoreDomPaste(suppressUntil: number, now = Date.now()): boolean {
  return now < suppressUntil;
}

/**
 * Decide what a DOM `paste` handler should do after always calling preventDefault.
 * - ignore: keydown paste already handled this gesture
 * - send: use clipboardData text (paste without keydown)
 * - fallback: empty clipboardData — caller should use Tauri pasteIntoPane
 */
export function decideDomPasteAction(opts: {
  suppressUntil: number;
  now?: number;
  clipboardText: string;
}): "ignore" | "fallback" | { send: string } {
  const now = opts.now ?? Date.now();
  if (shouldIgnoreDomPaste(opts.suppressUntil, now)) return "ignore";
  if (opts.clipboardText) return { send: opts.clipboardText };
  return "fallback";
}
