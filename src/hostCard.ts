/**
 * Host Card view-model helpers (#31) — pure, no DOM.
 */

export type ConnectionVisualState =
  | "local"
  | "connected"
  | "disconnected"
  | "connecting"
  | "error";

/** Title: 13px / semi-bold. */
export const HOST_TITLE_FONT_SIZE_PX = 13;
export const HOST_TITLE_FONT_WEIGHT = 600;

/** Subtitle: muted 11–12px. */
export const HOST_SUBTITLE_FONT_SIZE_PX = 12;

/** Status indicator diameter (6–8px). */
export const CONNECTION_DOT_SIZE_PX = 7;

/** Contract colors for connection dots (no halo). */
export const CONNECTION_DOT_COLORS: Record<ConnectionVisualState, string> = {
  connected: "#10B981",
  connecting: "#F59E0B",
  error: "#EF4444",
  disconnected: "#6B7280",
  local: "#8B5CF6",
};

export type HostCardKind = "host" | "local";

export type HostCardFlags = {
  openCount: number;
  active: boolean;
  kind: HostCardKind;
};

export function hostCardClassList(flags: HostCardFlags): string {
  const parts = ["item", "host-card"];
  if (flags.kind === "local") parts.push("pinned");
  if (flags.openCount > 0) parts.push("open");
  if (flags.active) parts.push("active-host");
  return parts.join(" ");
}

export function connectionDotClassList(state: string): string {
  const s = normalizeConnectionState(state);
  const parts = ["connection-dot", `state-${s}`];
  if (s === "connecting") parts.push("is-pulsed");
  return parts.join(" ");
}

export function shouldShowSessionCount(openCount: number): boolean {
  return openCount > 1;
}

export function sessionCountLabel(openCount: number): string {
  return `×${openCount}`;
}

export function hostCardAriaStatus(state: string): string {
  switch (normalizeConnectionState(state)) {
    case "connected":
      return "Connected";
    case "connecting":
      return "Connecting";
    case "error":
      return "Connection error";
    case "local":
      return "Local computer";
    case "disconnected":
    default:
      return "Disconnected";
  }
}

export function connectionDotColor(state: string): string {
  return CONNECTION_DOT_COLORS[normalizeConnectionState(state)];
}

function normalizeConnectionState(state: string): ConnectionVisualState {
  if (
    state === "local" ||
    state === "connected" ||
    state === "disconnected" ||
    state === "connecting" ||
    state === "error"
  ) {
    return state;
  }
  return "disconnected";
}
