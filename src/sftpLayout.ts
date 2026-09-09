/**
 * SFTP browser layout: single pane by default, optional free Host A / Host B split (#41).
 */

/** Sentinel id for This computer (local filesystem). */
export const SFTP_LOCAL_ID = "__local__";

export type SftpEndpointId = typeof SFTP_LOCAL_ID | (string & {});

export type SftpLayoutMode = "single" | "split";

export type SftpPaneSlot = "a" | "b";

export type SftpBrowserState = {
  mode: SftpLayoutMode;
  /** Primary pane (single) / left pane (split). */
  paneA: SftpEndpointId;
  /** Right pane when split; null in single mode. */
  paneB: SftpEndpointId | null;
};

export function createSftpBrowserState(
  initial?: Partial<SftpBrowserState>,
): SftpBrowserState {
  return {
    mode: initial?.mode ?? "single",
    paneA: initial?.paneA ?? SFTP_LOCAL_ID,
    paneB: initial?.mode === "split" ? (initial.paneB ?? SFTP_LOCAL_ID) : (initial?.paneB ?? null),
  };
}

export function isLocalEndpoint(id: SftpEndpointId | null | undefined): boolean {
  return id === SFTP_LOCAL_ID;
}

export function openSingle(
  _state: SftpBrowserState,
  endpoint: SftpEndpointId,
): SftpBrowserState {
  return { mode: "single", paneA: endpoint, paneB: null };
}

/** Companion for split: prefer local if A is remote, else first SSH host, else local. */
export function defaultSplitCompanion(
  paneA: SftpEndpointId,
  hostIds: string[],
): SftpEndpointId {
  if (!isLocalEndpoint(paneA)) return SFTP_LOCAL_ID;
  const other = hostIds.find((id) => id !== paneA);
  return other ?? SFTP_LOCAL_ID;
}

export function enterSplit(
  state: SftpBrowserState,
  hostIds: string[],
  paneB?: SftpEndpointId,
): SftpBrowserState {
  const b = paneB ?? defaultSplitCompanion(state.paneA, hostIds);
  return { mode: "split", paneA: state.paneA, paneB: b };
}

export function exitSplit(state: SftpBrowserState): SftpBrowserState {
  return { mode: "single", paneA: state.paneA, paneB: null };
}

export function setPaneEndpoint(
  state: SftpBrowserState,
  slot: SftpPaneSlot,
  endpoint: SftpEndpointId,
): SftpBrowserState {
  if (slot === "a") return { ...state, paneA: endpoint };
  if (state.mode !== "split") return state;
  return { ...state, paneB: endpoint };
}

export function shouldShowTransferUi(state: SftpBrowserState): boolean {
  return state.mode === "split" && state.paneB != null;
}

/** Transfers need exactly one local side and one remote SSH side. */
export function canTransferBetween(
  a: SftpEndpointId,
  b: SftpEndpointId | null,
): boolean {
  if (b == null) return false;
  const aLocal = isLocalEndpoint(a);
  const bLocal = isLocalEndpoint(b);
  return aLocal !== bLocal;
}

export function transferDirection(
  a: SftpEndpointId,
  b: SftpEndpointId,
): { localSlot: SftpPaneSlot; remoteSlot: SftpPaneSlot; remoteHostId: string } | null {
  if (!canTransferBetween(a, b)) return null;
  if (isLocalEndpoint(a)) {
    return { localSlot: "a", remoteSlot: "b", remoteHostId: b };
  }
  return { localSlot: "b", remoteSlot: "a", remoteHostId: a };
}

export function paneTestId(slot: SftpPaneSlot): string {
  return slot === "a" ? "sftp-pane-a" : "sftp-pane-b";
}
