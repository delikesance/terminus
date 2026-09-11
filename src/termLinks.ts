/**
 * Terminal hyperlink detection (#153).
 * Click-to-open uses these helpers + Tauri shell `open` for http(s) only.
 */

export type TermLinkHit = {
  url: string;
  /** Inclusive start index in the line. */
  start: number;
  /** Exclusive end index in the line. */
  end: number;
};

const URL_RE = /https?:\/\/[^\s<>"'`]+/gi;

/** Strip common trailing punctuation that is not part of the URL. */
function trimUrlMatch(raw: string): string {
  let url = raw;
  while (/[.,;:!?)\]}>]$/.test(url)) {
    // Keep balanced ")" if an opening "(" appears earlier in the URL.
    if (url.endsWith(")") && (url.match(/\(/g)?.length ?? 0) > (url.match(/\)/g)?.length ?? 0) - 1) {
      break;
    }
    url = url.slice(0, -1);
  }
  return url;
}

export function isOpenableHttpUrl(url: string): boolean {
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Find an http(s) URL covering `col` (0-based index into `line`).
 * Returns null when the column is outside any openable URL.
 */
export function findUrlAt(line: string, col: number): TermLinkHit | null {
  if (!line || col < 0 || col >= line.length) return null;
  URL_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = URL_RE.exec(line)) !== null) {
    const raw = m[0];
    const url = trimUrlMatch(raw);
    if (!isOpenableHttpUrl(url)) continue;
    const start = m.index;
    const end = start + url.length;
    if (col >= start && col < end) {
      return { url, start, end };
    }
  }
  return null;
}

/** Whether a mouseup should try to open a link under the cell. */
export function shouldAttemptLinkOpen(opts: {
  button: number;
  shiftKey: boolean;
  mouseReporting: boolean;
  dragOccurred: boolean;
}): boolean {
  void opts.shiftKey;
  if (opts.button !== 0) return false;
  if (opts.dragOccurred) return false;
  // Mouse-mode apps own clicks (Shift still selects; never open).
  if (opts.mouseReporting) return false;
  return true;
}
