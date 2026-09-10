/**
 * Per-host SFTP pane load bookkeeping (#109).
 * Each remote keeps its own loadSeq so opening B never cancels A.
 */

export type SftpPaneError = { kind: string; message: string };

export type SftpPaneLive = {
  hostId: string;
  path: string;
  cwd: string;
  root: string;
  loadSeq: number;
  loading: boolean;
  /** True after at least one successful list (including empty). */
  listed: boolean;
  error: SftpPaneError | null;
};

export function createSftpPaneLive(hostId: string): SftpPaneLive {
  return {
    hostId,
    path: ".",
    cwd: "",
    root: "/",
    loadSeq: 0,
    loading: false,
    listed: false,
    error: null,
  };
}

/** Bump per-host seq and mark loading; returns the seq for this attempt. */
export function beginPaneLoad(live: SftpPaneLive, path: string): { live: SftpPaneLive; seq: number } {
  const seq = live.loadSeq + 1;
  return {
    seq,
    live: {
      ...live,
      path,
      loadSeq: seq,
      loading: true,
      error: null,
    },
  };
}

export function finishPaneLoad(
  live: SftpPaneLive,
  seq: number,
  patch: { path: string; cwd: string; root: string },
): SftpPaneLive | null {
  if (live.loadSeq !== seq) return null;
  return {
    ...live,
    path: patch.path,
    cwd: patch.cwd,
    root: patch.root,
    loading: false,
    listed: true,
    error: null,
  };
}

export function failPaneLoad(
  live: SftpPaneLive,
  seq: number,
  error: SftpPaneError,
): SftpPaneLive | null {
  if (live.loadSeq !== seq) return null;
  return { ...live, loading: false, error };
}

export function isPaneLoadCurrent(live: SftpPaneLive | undefined, seq: number): boolean {
  return !!live && live.loadSeq === seq;
}
