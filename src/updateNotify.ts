/**
 * App update notification UX (#33) — badge on Settings, no floating toast for availability.
 */

export type UpdateNotifyPhase = "available" | "progress" | "error";

/** Floating corner toast is retired for update prompts (#33). */
export function shouldShowFloatingUpdateToast(_phase: UpdateNotifyPhase): boolean {
  return false;
}

export function settingsHasUpdateBadge(available: boolean): boolean {
  return available === true;
}
