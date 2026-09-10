/**
 * Multi-remote SFTP session bookkeeping (#101).
 * Parks path/cwd/selection per host so opening B does not wipe A.
 */

export type SftpSessionSnap = {
  path: string;
  cwd: string;
  root: string;
  selected: string[];
};

export type SftpSessions = {
  /** Open remotes in tab order (oldest → newest). */
  order: string[];
  byId: Record<string, SftpSessionSnap>;
};

export function createSftpSessions(): SftpSessions {
  return { order: [], byId: {} };
}

export function defaultSessionSnap(): SftpSessionSnap {
  return { path: ".", cwd: "", root: "/", selected: [] };
}

/** Ensure host is open; create default snap if new. Does not change other sessions. */
export function openOrFocusRemote(sessions: SftpSessions, hostId: string): SftpSessions {
  if (!hostId) return sessions;
  const order = sessions.order.includes(hostId) ? [...sessions.order] : [...sessions.order, hostId];
  const byId = { ...sessions.byId };
  if (!byId[hostId]) byId[hostId] = defaultSessionSnap();
  return { order, byId };
}

export function rememberRemote(
  sessions: SftpSessions,
  hostId: string,
  snap: SftpSessionSnap,
): SftpSessions {
  if (!hostId) return sessions;
  const withHost = openOrFocusRemote(sessions, hostId);
  return {
    order: withHost.order,
    byId: { ...withHost.byId, [hostId]: { ...snap, selected: [...snap.selected] } },
  };
}

export function closeRemote(
  sessions: SftpSessions,
  hostId: string,
  activeId: string | null,
): { sessions: SftpSessions; nextActive: string | null } {
  const order = sessions.order.filter((id) => id !== hostId);
  const byId = { ...sessions.byId };
  delete byId[hostId];
  let nextActive = activeId;
  if (activeId === hostId) {
    nextActive = order.length ? order[order.length - 1]! : null;
  }
  return { sessions: { order, byId }, nextActive };
}

export function openRemoteIds(sessions: SftpSessions): string[] {
  return [...sessions.order];
}

export function hasRemote(sessions: SftpSessions, hostId: string): boolean {
  return sessions.order.includes(hostId);
}
