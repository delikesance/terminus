/**
 * Playwright E2E Test Bridge
 * Exposed as window.__terminusTest for test automation
 * 
 * QA Contract: PR #6, Issues #2, #3, #4
 */

import { invoke } from "@tauri-apps/api/core";

interface TerminusTestBridge {
  // E2E-1: Ungrouped count
  seedUngroupedHosts(count: number): Promise<void>;
  clearUngroupedHosts(): Promise<void>;
  groupsDelete(groupId: string): Promise<void>;
  restoreGroup(groupId: string): Promise<void>;
  
  // E2E-2: Connection dots
  sessionOpenSsh(hostId: string): Promise<string>;
  sessionClose(sessionId: string): Promise<void>;
  setConnection(hostId: string, state: string): Promise<void>;
  
  // E2E-3: SyncStatus
  setSyncStatus(state: string, lastError?: string): Promise<void>;
}

export function initTestBridge(): void {
  if (typeof window === 'undefined') return;
  
  const bridge: TerminusTestBridge = {
    async seedUngroupedHosts(count: number): Promise<void> {
      // Create N ungrouped hosts (no group_id, not deleted)
      for (let i = 0; i < count; i++) {
        await invoke('hosts_upsert', {
          host: {
            id: `test-ungrouped-${i}-${Date.now()}`,
            name: `Test Host ${i}`,
            hostname: `test-${i}.example.com`,
            port: 22,
            username: 'test',
            auth_method: 'key',
            password: null,
            identity_id: null,
            group_id: null, // UNGROUPED
            tags: [],
            notes: '',
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
            deleted_at: null, // NOT DELETED
          },
        });
      }
      // Trigger UI refresh
      await invoke('hosts_list');
    },

    async clearUngroupedHosts(): Promise<void> {
      // Delete all ungrouped test hosts
      const hosts = await invoke<any[]>('hosts_list');
      for (const host of hosts) {
        if (!host.group_id && host.name.startsWith('Test Host')) {
          await invoke('hosts_delete', { id: host.id });
        }
      }
    },

    async groupsDelete(groupId: string): Promise<void> {
      // Soft-delete group (sets deleted_at)
      const groups = await invoke<any[]>('groups_list');
      const group = groups.find((g: any) => g.id === groupId);
      if (group) {
        await invoke('groups_upsert', {
          group: {
            ...group,
            deleted_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        });
        
        // Detach hosts (set group_id = null)
        const hosts = await invoke<any[]>('hosts_list');
        for (const host of hosts.filter((h: any) => h.group_id === groupId)) {
          await invoke('hosts_upsert', {
            host: {
              ...host,
              group_id: null,
              updated_at: new Date().toISOString(),
            },
          });
        }
      }
    },

    async restoreGroup(groupId: string): Promise<void> {
      // Restore group (clear deleted_at)
      const groups = await invoke<any[]>('groups_list');
      const group = groups.find((g: any) => g.id === groupId);
      if (group) {
        await invoke('groups_upsert', {
          group: {
            ...group,
            deleted_at: null,
            updated_at: new Date().toISOString(),
          },
        });
        // NOTE: Per AC, hosts are NOT reattached
      }
    },

    async sessionOpenSsh(hostId: string): Promise<string> {
      // Open SSH session and return session ID
      const sessionInfo = await invoke<any>('session_open_ssh', {
        hostId,
        cols: 80,
        rows: 24,
        scale: 1,
      });
      return sessionInfo.id;
    },

    async sessionClose(sessionId: string): Promise<void> {
      await invoke('session_close', { id: sessionId });
    },

    async setConnection(hostId: string, state: string): Promise<void> {
      // Set connection state for a host without requiring real SSH/docker
      // Backend provides test_set_connection command
      await invoke('test_set_connection', {
        hostId,
        state, // local | connected | disconnected | connecting | error
      });
    },

    async setSyncStatus(state: string, lastError?: string): Promise<void> {
      // Mock sync status for testing
      // In real implementation, this would update the backend state
      // For now, trigger UI update with test state
      await invoke('test_set_sync_status', {
        status: {
          state,
          last_error: lastError || null,
          configured: state !== 'unconfigured',
          url: state !== 'unconfigured' ? 'postgres://test:test@localhost:5432/terminus' : null,
          last_sync: state === 'idle' ? new Date().toISOString() : null,
        },
      });
    },
  };

  (window as any).__terminusTest = bridge;
  console.log('[Terminus Test Bridge] Initialized for E2E tests');
}
