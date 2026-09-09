/** View-model helpers for the Port Forwarding sidebar panel (#28). */

export type ForwardRuntimeState = "running" | "stopped";

export type ForwardRecord = {
  id: string;
  name: string;
  bind_host: string;
  bind_port: number;
  dest_host?: string | null;
  dest_port?: number | null;
  host_id: string;
};

export type ForwardListItem = {
  id: string;
  name: string;
  bindHost: string;
  bindPort: number;
  destHost: string;
  destPort: number | null;
  hostId: string;
  active: boolean;
  state: ForwardRuntimeState;
};

export type ForwardFormInput = {
  hostId: string;
  localPort: number | string;
  remoteHost: string;
  remotePort: number | string;
  name?: string;
  bindHost?: string;
};

export type ForwardFormValid = {
  ok: true;
  hostId: string;
  bindHost: string;
  bindPort: number;
  destHost: string;
  destPort: number;
  name: string;
};

export type ForwardFormInvalid = { ok: false; error: string };

export type ToggleAction = "start" | "stop";

function parsePort(value: number | string): number | null {
  const n = typeof value === "number" ? value : Number(String(value).trim());
  if (!Number.isInteger(n) || n < 1 || n > 65535) return null;
  return n;
}

export function buildForwardRows(
  forwards: ForwardRecord[],
  runningIds: Iterable<string>,
): ForwardListItem[] {
  const running = runningIds instanceof Set ? runningIds : new Set(runningIds);
  return forwards.map((f) => {
    const active = running.has(f.id);
    const destPort: number | null = f.dest_port == null ? null : f.dest_port;
    const item: ForwardListItem = {
      id: f.id,
      name: f.name,
      bindHost: f.bind_host,
      bindPort: f.bind_port,
      destHost: f.dest_host || "127.0.0.1",
      destPort,
      hostId: f.host_id,
      active,
      state: active ? "running" : "stopped",
    };
    return item;
  });
}

export function toggleActionFromChecked(wantActive: boolean): ToggleAction {
  return wantActive ? "start" : "stop";
}

export function applyToggleSuccess(
  running: Set<string>,
  id: string,
  action: ToggleAction,
): Set<string> {
  const next = new Set(running);
  if (action === "start") next.add(id);
  else next.delete(id);
  return next;
}

export function applyToggleFailure(
  running: Set<string>,
  id: string,
  action: ToggleAction,
): Set<string> {
  const next = new Set(running);
  if (action === "start") next.delete(id);
  return next;
}

export function validateForwardForm(input: ForwardFormInput): ForwardFormValid | ForwardFormInvalid {
  const hostId = (input.hostId ?? "").trim();
  if (!hostId) return { ok: false, error: "SSH host is required." };

  const bindPort = parsePort(input.localPort);
  if (bindPort == null) return { ok: false, error: "Local port must be 1–65535." };

  const destHost = (input.remoteHost ?? "").trim();
  if (!destHost) return { ok: false, error: "Remote host is required." };

  const destPort = parsePort(input.remotePort);
  if (destPort == null) return { ok: false, error: "Remote port must be 1–65535." };

  const bindHost = (input.bindHost ?? "").trim() || "127.0.0.1";
  const name =
    (input.name ?? "").trim() || `${bindHost}:${bindPort} → ${destHost}:${destPort}`;

  return { ok: true, hostId, bindHost, bindPort, destHost, destPort, name };
}
