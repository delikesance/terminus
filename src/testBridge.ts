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
  };

  (window as any).__terminusTest = bridge;
  console.log("[Terminus Test Bridge] Initialized for E2E tests");
}
