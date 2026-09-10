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
  openSftp(hostId: string): Promise<void>;
  /** C9: force next sftp_* IPC to fail with typed JSON error string */
  sftpForceError(kind: string, message: string): Promise<void>;
  sftpReset(hostId?: string): Promise<void>;
  lastSftpOpen(): Promise<{ hostId: string; path: string; name: string } | null>;
  /** C9b: reset + seed the virtual local FS (local pane), optional new home path. */
  seedLocalDir(path?: string): Promise<string>;
  /** C9b: poll transfer op counters (uploads = local→remote, downloads = remote→local). */
  transferOps(): Promise<{ uploads: number; downloads: number }>;
  /** #45: seed N files into the SFTP home for listing load tests. */
  seedSftpBulk(hostId: string, count: number): Promise<{ count: number; path: string }>;
  /** #45: DOM/heap/webgl snapshot for crash-load assertions. */
  perfSnapshot(): Promise<{
    at: number;
    panes: number;
    canvases: number;
    heap: { usedMb: number; totalMb: number; limitMb: number } | null;
    webglOk: boolean;
  }>;
  /** #45: stress canvas 2D paint (CPU+GPU upload path). */
  stressCanvases(opts: {
    iterations: number;
    width: number;
    height: number;
  }): Promise<{ paints: number; ms: number; canvases: number }>;
  /** #45: sample rAF cadence for ~durationMs. */
  measureFps(durationMs: number): Promise<{ frames: number; fps: number; ms: number }>;
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
  setUpdateAvailable(version: string): Promise<void>;
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
          name: hostId.startsWith("sftp-host-") ? `SFTP ${hostId.slice("sftp-host-".length)}` : "SFTP Lab",
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

    async openSftp(hostId: string): Promise<void> {
      window.dispatchEvent(new CustomEvent("terminus-e2e-open-sftp", { detail: { hostId } }));
      await refreshUi();
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

    async seedSftpBulk(hostId: string, count: number): Promise<{ count: number; path: string }> {
      return invoke("test_sftp_bulk", { hostId, count });
    },

    async perfSnapshot() {
      const mem = (performance as any).memory as
        | { usedJSHeapSize: number; totalJSHeapSize: number; jsHeapSizeLimit: number }
        | undefined;
      const mb = (b: number) => Math.round((b / (1024 * 1024)) * 10) / 10;
      let webglOk = false;
      try {
        const c = document.createElement("canvas");
        webglOk = !!(
          c.getContext("webgl2", { failIfMajorPerformanceCaveat: true }) ||
          c.getContext("webgl", { failIfMajorPerformanceCaveat: true })
        );
      } catch {
        webglOk = false;
      }
      return {
        at: performance.now(),
        panes: document.querySelectorAll(".pane").length,
        canvases: document.querySelectorAll("canvas.term-canvas").length,
        heap: mem
          ? {
              usedMb: mb(mem.usedJSHeapSize),
              totalMb: mb(mem.totalJSHeapSize),
              limitMb: mb(mem.jsHeapSizeLimit),
            }
          : null,
        webglOk,
        activeRenderer: (window as any).__terminusActiveRenderer ?? null,
      };
    },

    async stressCanvases(opts: {
      iterations: number;
      width: number;
      height: number;
    }): Promise<{ paints: number; ms: number; canvases: number }> {
      const canvases = [...document.querySelectorAll("canvas.term-canvas")] as HTMLCanvasElement[];
      const w = Math.max(8, Math.min(320, opts.width | 0));
      const h = Math.max(8, Math.min(180, opts.height | 0));
      const iterations = Math.max(1, Math.min(200, opts.iterations | 0));
      const t0 = performance.now();
      let paints = 0;
      for (let i = 0; i < iterations; i++) {
        for (const canvas of canvases) {
          const ctx = canvas.getContext("2d", { alpha: false });
          if (!ctx) continue;
          if (canvas.width !== w || canvas.height !== h) {
            canvas.width = w;
            canvas.height = h;
          }
          const image = ctx.createImageData(w, h);
          const data = image.data;
          const base = (i * 37) & 255;
          for (let p = 0; p < data.length; p += 16) {
            data[p] = base;
            data[p + 1] = (p >> 2) & 255;
            data[p + 2] = 90;
            data[p + 3] = 255;
            data[p + 4] = base;
            data[p + 5] = ((p >> 2) + 40) & 255;
            data[p + 6] = 110;
            data[p + 7] = 255;
            data[p + 8] = base;
            data[p + 9] = ((p >> 2) + 80) & 255;
            data[p + 10] = 130;
            data[p + 11] = 255;
            data[p + 12] = base;
            data[p + 13] = ((p >> 2) + 120) & 255;
            data[p + 14] = 150;
            data[p + 15] = 255;
          }
          try {
            const bitmap = await createImageBitmap(image);
            ctx.drawImage(bitmap, 0, 0);
            bitmap.close();
          } catch {
            ctx.putImageData(image, 0, 0);
          }
          paints += 1;
        }
        await new Promise((r) => requestAnimationFrame(() => r(undefined)));
      }
      return { paints, ms: performance.now() - t0, canvases: canvases.length };
    },

    async measureFps(durationMs: number): Promise<{ frames: number; fps: number; ms: number }> {
      const budget = Math.max(200, Math.min(5000, durationMs));
      return new Promise((resolve) => {
        let frames = 0;
        const t0 = performance.now();
        const tick = (now: number) => {
          frames += 1;
          if (now - t0 < budget) requestAnimationFrame(tick);
          else {
            const ms = now - t0;
            resolve({ frames, ms, fps: ms > 0 ? (frames / ms) * 1000 : 0 });
          }
        };
        requestAnimationFrame(tick);
      });
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

    async setUpdateAvailable(version: string): Promise<void> {
      const api = (window as any).__terminusUpdateTest;
      if (typeof api?.setPendingAppUpdate === "function") {
        api.setPendingAppUpdate(version);
      }
    },
  };

  (window as any).__terminusTest = bridge;
  console.log("[Terminus Test Bridge] Initialized for E2E tests");
}
