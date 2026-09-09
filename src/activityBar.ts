/**
 * Activity Bar + contextual sidebar navigation state (#27 / #29).
 * Pure helpers — no DOM.
 */

export type ActivityId = "hosts" | "snippets" | "history" | "forwards" | "sftp";

export const ACTIVITIES: readonly ActivityId[] = [
  "hosts",
  "snippets",
  "history",
  "forwards",
  "sftp",
] as const;

/** Fixed Activity Bar width in CSS pixels. */
export const ACTIVITY_BAR_WIDTH_PX = 48;

/** Activity button hit target (matches `#activity-bar button`). */
export const ACTIVITY_BAR_ITEM_SIZE_PX = 36;

/**
 * Top padding of the Activity Bar so the first icon center aligns with
 * the sidebar search input center (#29).
 * searchCenter = SEARCH_PAD_TOP + SEARCH_INPUT_HEIGHT/2
 * iconCenter   = TOP_PADDING + ITEM_SIZE/2
 */
export const SEARCH_PAD_TOP_PX = 14;
export const SEARCH_INPUT_HEIGHT_PX = 32;
export const ACTIVITY_BAR_TOP_PADDING_PX =
  SEARCH_PAD_TOP_PX + SEARCH_INPUT_HEIGHT_PX / 2 - ACTIVITY_BAR_ITEM_SIZE_PX / 2;

/** Terminal `.pane` padding (#29): `16px 20px`. */
export const TERMINAL_PANE_PADDING_Y_PX = 16;
export const TERMINAL_PANE_PADDING_X_PX = 20;

export function searchInputCenterY(): number {
  return SEARCH_PAD_TOP_PX + SEARCH_INPUT_HEIGHT_PX / 2;
}

export function activityBarFirstIconCenterY(): number {
  return ACTIVITY_BAR_TOP_PADDING_PX + ACTIVITY_BAR_ITEM_SIZE_PX / 2;
}

export type NavState = {
  active: ActivityId;
  sidebarOpen: boolean;
};

function isActivityId(id: string): id is ActivityId {
  return (ACTIVITIES as readonly string[]).includes(id);
}

export function createNavState(initial?: Partial<NavState>): NavState {
  const active =
    initial?.active && isActivityId(initial.active) ? initial.active : "hosts";
  return {
    active,
    sidebarOpen: initial?.sidebarOpen ?? true,
  };
}

/**
 * Click an Activity Bar item.
 * - Same activity → toggle sidebar open/closed.
 * - Other activity → switch active and open sidebar.
 * - Unknown id → no-op.
 */
export function clickActivity(state: NavState, id: string): NavState {
  if (!isActivityId(id)) return { ...state };
  if (state.active === id) {
    return { ...state, sidebarOpen: !state.sidebarOpen };
  }
  return { active: id, sidebarOpen: true };
}

/** Titlebar / Ctrl+B toggle — does not change the active activity. */
export function toggleSidebarOpen(state: NavState, force?: boolean): NavState {
  if (force === true) return { ...state, sidebarOpen: true };
  if (force === false) return { ...state, sidebarOpen: false };
  return { ...state, sidebarOpen: !state.sidebarOpen };
}

/** A panel is visible only when the sidebar is open and it is the active activity. */
export function isPanelVisible(state: NavState, panel: ActivityId): boolean {
  return state.sidebarOpen && state.active === panel;
}
