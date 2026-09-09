/**
 * Playwright E2E Test Bridge
 * Exposed as window.__terminusTest for test automation
 *
 * QA Contract: PR #6, Issues #2, #3, #4
 *
 * Active only when the app is built with VITE_E2E=1.
 */

import { invoke } from "@tauri-apps/api/core";

interface TerminusTestBridge {
  seedUngroupedHosts(count: number): Promise<void>;
  clearUngroupedHosts(): Promise<void>;
  clearAllHosts(): Promise<void>;
  groupsDelete(groupId: string): Promise<void>;
  restoreGroup(groupId: string): Promise<void>;
  sessionOpenSsh(hostId: string): Promise<string>;
  sessionClose(sessionId: string): Promise<void>;
  setConnection(hostId: string, state: string): Promise<void>;
  setSyncStatus(state: string, lastError?: string): Promise<void>;
  openVault(): Promise<void>;
  /** C9: seed a host + reset virtual SFTP tree */
  seedSftpHost(hostId?: string): Promise<string>;
  /** C9: force next sftp_* IPC to fail with typed JSON error string */
  sftpForceError(kind: string, message: string): Promise<void>;
  sftpReset(hostId?: string): Promise<void>;
  lastSftpOpen(): Promise<{ hostId: string; path: string; name: string } | null>;
  /** C9b: reset + seed the virtual local FS (local pane), optional new home path. */
  seedLocalDir(path?: string): Promise<string>;
  /** C9b: poll transfer op counters (uploads = local→remote, downloads = remote→local). */
  transferOps(): Promise<{ uploads: number; downloads: number }>;
  seedTofuHost(hostId?: string): Promise<string>;
  seedForwardHost(hostId?: string): Promise<string>;
  seedForward(opts: {
    id?: string;
    hostId: string;
    name?: string;
    bindPort?: number;
    destPort?: number;
  }): Promise<string>;
  forwardFailNext(message: string): Promise<void>;
  seedSlowConnectHost(hostId?: string, ms?: number): Promise<string>;
  seedAuthFailHost(hostId?: string): Promise<string>;
  hostsRuntime(): Promise<Array<{ host_id: string; connection: string; open_count: number }>>;
}

async function refreshUi(): Promise<void> {
  window.dispatchEvent(new Event("terminus-e2e-refresh"));
  // Allow React-less DOM refresh to flush.
  await new Promise((r) => requestAnimationFrame(() => r(undefined)));
}

export function initTestBridge(): void {
  if (typeof window === "undefined") return;

  const bridge: TerminusTestBridge = {
    async seedUngroupedHosts(count: number): Promise<void> {
      for (let i = 0; i < count; i++) {
        await invoke("hosts_upsert", {
          host: {
            id: `test-ungrouped-${i}-${Date.now()}`,
            name: `Test Host ${i}`,
            hostname: `test-${i}.example.com`,
            port: 22,
            username: "test",
            auth_method: "key",
            password: null,
            identity_id: null,
            group_id: null,
            tags: [],
            notes: "",
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
            deleted_at: null,
          },
        });
      }
      await refreshUi();
    },

    async clearUngroupedHosts(): Promise<void> {
      const hosts = await invoke<any[]>("hosts_list");
      for (const host of hosts) {
        if (!host.group_id && String(host.name).startsWith("Test Host")) {
          await invoke("hosts_delete", { id: host.id });
        }
      }
      await refreshUi();
    },

    async clearAllHosts(): Promise<void> {
      const hosts = await invoke<any[]>("hosts_list");
      for (const host of hosts) {
        await invoke("hosts_delete", { id: host.id });
      }
      const forwards = await invoke<any[]>("forwards_list").catch(() => []);
      for (const fwd of forwards) {
        await invoke("forwards_delete", { id: fwd.id }).catch(() => undefined);
      }
      await refreshUi();
    },

    async groupsDelete(groupId: string): Promise<void> {
      const groups = await invoke<any[]>("groups_list");
      const group = groups.find((g: any) => g.id === groupId);
      if (group) {
        await invoke("groups_upsert", {
          group: {
            ...group,
            deleted_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        });
        const hosts = await invoke<any[]>("hosts_list");
        for (const host of hosts.filter((h: any) => h.group_id === groupId)) {
          await invoke("hosts_upsert", {
            host: {
              ...host,
              group_id: null,
              updated_at: new Date().toISOString(),
            },
          });
        }
      }
      await refreshUi();
    },

    async restoreGroup(groupId: string): Promise<void> {
      const groups = await invoke<any[]>("groups_list");
      const group = groups.find((g: any) => g.id === groupId);
      if (group) {
        await invoke("groups_upsert", {
          group: {
            ...group,
            deleted_at: null,
            updated_at: new Date().toISOString(),
          },
        });
      }
      await refreshUi();
    },

    async sessionOpenSsh(hostId: string): Promise<string> {
      const sessionInfo = await invoke<any>("session_open_ssh", {
        hostId,
        cols: 80,
        rows: 24,
        scale: 1,
      });
      await refreshUi();
      return sessionInfo.id;
    },

    async sessionClose(sessionId: string): Promise<void> {
      await invoke("session_close", { id: sessionId });
      await refreshUi();
    },

    async setConnection(hostId: string, state: string): Promise<void> {
      // Backend command compiled only when TERMINUS_E2E=1 (see src-tauri/build.rs).
      await invoke("test_set_host_connection", {
        hostId,
        connectionState: state,
      });
      await refreshUi();
    },

    async setSyncStatus(state: string, lastError?: string): Promise<void> {
      await invoke("test_set_sync_status", {
        status: {
          state,
          last_error: lastError || null,
          configured: state !== "unconfigured",
          url: state !== "unconfigured" ? "postgres://test:test@localhost:5432/terminus" : null,
          last_sync: state === "idle" ? new Date().toISOString() : null,
        },
      });
      await refreshUi();
    },

    async openVault(): Promise<void> {
      window.dispatchEvent(new Event("terminus-e2e-open-vault"));
      await new Promise((r) => requestAnimationFrame(() => r(undefined)));
    },

    async seedSftpHost(hostId = `sftp-host-${Date.now()}`): Promise<string> {
      await invoke("hosts_upsert", {
        host: {
          id: hostId,
          name: "SFTP Lab",
          hostname: "sftp.example.com",
          port: 22,
          username: "lab",
          auth_method: "key",
          password: null,
          identity_id: null,
          group_id: null,
          tags: [],
          notes: "",
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          deleted_at: null,
        },
      });
      await invoke("test_sftp_reset", { hostId }).catch(() => undefined);
      await refreshUi();
      return hostId;
    },

    async sftpForceError(kind: string, message: string): Promise<void> {
      await invoke("test_sftp_force_error", {
        error: JSON.stringify({ kind, message }),
      });
    },

    async sftpReset(hostId?: string): Promise<void> {
      await invoke("test_sftp_reset", { hostId: hostId ?? "" });
    },

    async lastSftpOpen(): Promise<{ hostId: string; path: string; name: string } | null> {
      return invoke("test_sftp_last_open");
    },

    async seedLocalDir(path?: string): Promise<string> {
      const home = await invoke<string>("test_local_reset", { path: path ?? "" });
      await refreshUi();
      return home;
    },

    async transferOps(): Promise<{ uploads: number; downloads: number }> {
      return invoke("test_transfer_ops");
    },

    async seedTofuHost(hostId = `tofu-host-${Date.now()}`): Promise<string> {
      await invoke("hosts_upsert", {
        host: {
          id: hostId,
          name: "TOFU Lab",
          hostname: "tofu.example.com",
          port: 22,
          username: "lab",
          auth_method: "key",
          password: null,
          identity_id: null,
          group_id: null,
          tags: [],
          notes: "",
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          deleted_at: null,
        },
      });
      await invoke("test_require_tofu", { hostId }).catch(() => undefined);
      await refreshUi();
      return hostId;
    },

    async seedForwardHost(hostId = `fwd-host-${Date.now()}`): Promise<string> {
      await invoke("hosts_upsert", {
        host: {
          id: hostId,
          name: "Forward Lab",
          hostname: "fwd.example.com",
          port: 22,
          username: "lab",
          auth_method: "key",
          password: null,
          identity_id: null,
          group_id: null,
          tags: [],
          notes: "",
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          deleted_at: null,
        },
      });
      await refreshUi();
      return hostId;
    },

    async seedForward(opts: {
      id?: string;
      hostId: string;
      name?: string;
      bindPort?: number;
      destPort?: number;
    }): Promise<string> {
      const id = opts.id ?? `fwd-${Date.now()}`;
      await invoke("forwards_upsert", {
        forward: {
          id,
          host_id: opts.hostId,
          kind: "local",
          name: opts.name ?? "Tunnel",
          bind_host: "127.0.0.1",
          bind_port: opts.bindPort ?? 18080,
          dest_host: "127.0.0.1",
          dest_port: opts.destPort ?? 80,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          deleted_at: null,
        },
      });
      await refreshUi();
      return id;
    },

    async forwardFailNext(message: string): Promise<void> {
      await invoke("test_forward_fail_next", { message });
    },

    async seedSlowConnectHost(
      hostId = `slow-host-${Date.now()}`,
      ms = 500,
    ): Promise<string> {
      await invoke("hosts_upsert", {
        host: {
          id: hostId,
          name: "Slow Connect",
          hostname: "slow.example.com",
          port: 22,
          username: "lab",
          auth_method: "key",
          password: null,
          identity_id: null,
          group_id: null,
          tags: [],
          notes: "",
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          deleted_at: null,
        },
      });
      await invoke("test_slow_connect", { hostId, ms }).catch(() => undefined);
      await refreshUi();
      return hostId;
    },

    async seedAuthFailHost(hostId = `auth-fail-${Date.now()}`): Promise<string> {
      await invoke("hosts_upsert", {
        host: {
          id: hostId,
          name: "Auth Fail",
          hostname: "authfail.example.com",
          port: 22,
          username: "lab",
          auth_method: "password",
          password: "bad",
          identity_id: null,
          group_id: null,
          tags: [],
          notes: "",
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          deleted_at: null,
        },
      });
      await invoke("test_auth_fail", { hostId }).catch(() => undefined);
      await refreshUi();
      return hostId;
    },

    async hostsRuntime(): Promise<Array<{ host_id: string; connection: string; open_count: number }>> {
      return invoke("hosts_runtime");
    },
  };

  (window as any).__terminusTest = bridge;
  console.log("[Terminus Test Bridge] Initialized for E2E tests");
}
