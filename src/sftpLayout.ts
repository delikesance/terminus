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

/**
 * Place a newly opened remote so an already-visible remote stays on screen (#103).
 * - single remote A → split A|B
 * - split remote|local → replace local with B
 * - split local|remote → replace local with B
 * - split A|B + open C → replace B (A stays)
 * - already visible → no-op
 */
export function placeAdditionalRemote(
  state: SftpBrowserState,
  newRemote: SftpEndpointId,
): SftpBrowserState {
  if (!newRemote || isLocalEndpoint(newRemote)) return state;
  if (state.paneA === newRemote || state.paneB === newRemote) return state;

  if (state.mode === "single") {
    if (!isLocalEndpoint(state.paneA)) {
      return { mode: "split", paneA: state.paneA, paneB: newRemote };
    }
    return openSingle(state, newRemote);
  }

  if (isLocalEndpoint(state.paneA)) {
    return setPaneEndpoint(state, "a", newRemote);
  }
  if (state.paneB != null && isLocalEndpoint(state.paneB)) {
    return setPaneEndpoint(state, "b", newRemote);
  }
  return setPaneEndpoint(state, "b", newRemote);
}

/** Focus a remote without replacing another visible remote pane (#103). */
export function focusRemoteInLayout(
  state: SftpBrowserState,
  hostId: SftpEndpointId,
): SftpBrowserState {
  if (!hostId || isLocalEndpoint(hostId)) return state;
  if (state.paneA === hostId || state.paneB === hostId) return state;
  return placeAdditionalRemote(state, hostId);
}

export function shouldShowTransferUi(state: SftpBrowserState): boolean {
  return state.mode === "split" && state.paneB != null;
}

/**
 * Transfers allowed for local↔remote or distinct remote↔remote (#105).
 * Rejects null B, identical endpoints, and local|local.
 */
export function canTransferBetween(
  a: SftpEndpointId,
  b: SftpEndpointId | null,
): boolean {
  if (b == null || a === b) return false;
  const aLocal = isLocalEndpoint(a);
  const bLocal = isLocalEndpoint(b);
  if (aLocal && bLocal) return false;
  return true;
}

/** Local↔remote only (null when both remotes or both local). */
export function transferDirection(
  a: SftpEndpointId,
  b: SftpEndpointId,
): { localSlot: SftpPaneSlot; remoteSlot: SftpPaneSlot; remoteHostId: string } | null {
  const aLocal = isLocalEndpoint(a);
  const bLocal = isLocalEndpoint(b);
  if (aLocal === bLocal) return null;
  if (aLocal) {
    return { localSlot: "a", remoteSlot: "b", remoteHostId: b };
  }
  return { localSlot: "b", remoteSlot: "a", remoteHostId: a };
}

export type TransferLane =
  | {
      kind: "local-remote";
      localSlot: SftpPaneSlot;
      remoteSlot: SftpPaneSlot;
      remoteHostId: string;
    }
  | {
      kind: "remote-remote";
      hostA: string;
      hostB: string;
    };

/** Describes how split panes exchange files (#105). */
export function transferLane(
  a: SftpEndpointId,
  b: SftpEndpointId | null,
): TransferLane | null {
  if (!canTransferBetween(a, b) || b == null) return null;
  if (isLocalEndpoint(a) || isLocalEndpoint(b)) {
    const dir = transferDirection(a, b);
    if (!dir) return null;
    return { kind: "local-remote", ...dir };
  }
  return { kind: "remote-remote", hostA: a, hostB: b };
}

export function paneTestId(slot: SftpPaneSlot): string {
  return slot === "a" ? "sftp-pane-a" : "sftp-pane-b";
}
