/**
 * xterm mouse reporting (#148) — SGR and legacy CSI encodings.
 * modeFlags bits from the frame header:
 *   bit4 0b010000 = any MOUSE_MODE (click/drag/motion)
 *   bit5 0b100000 = SGR_MOUSE
 *   bit6 0b1000000 = MOUSE_DRAG or MOUSE_MOTION (send drag/move reports)
 */

export const MODE_MOUSE = 0b010000;
export const MODE_SGR_MOUSE = 0b100000;
export const MODE_MOUSE_DRAG = 0b1000000;

export type MouseAction = "press" | "release" | "motion";

export function mouseReportingEnabled(modeFlags: number): boolean {
  return (modeFlags & MODE_MOUSE) !== 0;
}

export function mouseDragReportsEnabled(modeFlags: number): boolean {
  return (modeFlags & MODE_MOUSE_DRAG) !== 0;
}

/** DOM button 0/1/2 → xterm button code (wheels use 64/65). */
export function xtermButtonCode(domButton: number, wheel?: "up" | "down"): number {
  if (wheel === "up") return 64;
  if (wheel === "down") return 65;
  if (domButton === 0) return 0;
  if (domButton === 1) return 1;
  if (domButton === 2) return 2;
  return 0;
}

function modsMask(opts: { shift?: boolean; alt?: boolean; ctrl?: boolean }): number {
  return (opts.shift ? 4 : 0) + (opts.alt ? 8 : 0) + (opts.ctrl ? 16 : 0);
}

/**
 * Encode a mouse event. `col`/`row` are 0-based cell coords from the UI;
 * sequences use 1-based xterm cells.
 */
export function encodeTermMouse(opts: {
  modeFlags: number;
  col: number;
  row: number;
  button: number;
  action: MouseAction;
  shift?: boolean;
  alt?: boolean;
  ctrl?: boolean;
}): string | null {
  if (!mouseReportingEnabled(opts.modeFlags)) return null;
  if (opts.action === "motion" && !mouseDragReportsEnabled(opts.modeFlags)) return null;

  const col = Math.max(0, opts.col | 0) + 1;
  const row = Math.max(0, opts.row | 0) + 1;
  let cb = opts.button | 0;
  if (opts.action === "motion") cb |= 32;
  cb |= modsMask(opts);

  const sgr = (opts.modeFlags & MODE_SGR_MOUSE) !== 0;
  if (sgr) {
    const final = opts.action === "release" ? "m" : "M";
    return `\x1b[<${cb};${col};${row}${final}`;
  }

  // Legacy X10/UTF-8-ish: release always uses button 3; coords capped.
  const legacyBtn = opts.action === "release" ? 3 : cb;
  const b = 32 + (legacyBtn & 0xff);
  const x = 32 + Math.min(col, 223);
  const y = 32 + Math.min(row, 223);
  return `\x1b[M${String.fromCharCode(b)}${String.fromCharCode(x)}${String.fromCharCode(y)}`;
}
