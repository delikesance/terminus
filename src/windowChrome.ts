/**
 * OS window chrome mode for the custom titlebar (#99).
 * "mac" = traffic lights left; "win" = caption symbols right (Windows + Linux).
 */

export type WindowChrome = "mac" | "win";

export type PlatformHint = {
  platform?: string;
  userAgent?: string;
  /** Optional override from tests / Tauri env. */
  force?: WindowChrome | null;
};

/** Resolve chrome style from platform / UA. Defaults to "win" when unknown. */
export function resolveWindowChrome(hint: PlatformHint = {}): WindowChrome {
  if (hint.force === "mac" || hint.force === "win") return hint.force;
  const platform = (hint.platform ?? "").toLowerCase();
  const ua = (hint.userAgent ?? "").toLowerCase();
  if (platform.includes("mac") || platform === "darwin" || ua.includes("mac os")) return "mac";
  return "win";
}

/** Apply `data-os-chrome` on the document element. */
export function applyWindowChrome(
  chrome: WindowChrome,
  root: { dataset: DOMStringMap } = document.documentElement,
): void {
  root.dataset.osChrome = chrome;
}
