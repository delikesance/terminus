import { Page } from '@playwright/test';

/**
 * Wait for the test bridge to be initialized
 * The bridge is only available in builds with VITE_E2E=1
 */
export async function waitForTestBridge(page: Page, timeoutMs = 5000) {
  await page.waitForFunction(
    () => typeof (window as any).__terminusTest !== 'undefined',
    { timeout: timeoutMs }
  );
}

/**
 * Get the test bridge with type safety
 */
export function getTestBridge(page: Page) {
  return {
    // E2E-1: Ungrouped count
    seedUngroupedHosts: (count: number) => 
      page.evaluate((n) => (window as any).__terminusTest.seedUngroupedHosts(n), count),
    clearUngroupedHosts: () => 
      page.evaluate(() => (window as any).__terminusTest.clearUngroupedHosts()),
    clearAllHosts: () =>
      page.evaluate(() => (window as any).__terminusTest.clearAllHosts()),
    groupsDelete: (groupId: string) => 
      page.evaluate((id) => (window as any).__terminusTest.groupsDelete(id), groupId),
    restoreGroup: (groupId: string) => 
      page.evaluate((id) => (window as any).__terminusTest.restoreGroup(id), groupId),
    
    // E2E-2: Connection dots
    sessionOpenSsh: (hostId: string) => 
      page.evaluate((id) => (window as any).__terminusTest.sessionOpenSsh(id), hostId),
    sessionClose: (sessionId: string) => 
      page.evaluate((id) => (window as any).__terminusTest.sessionClose(id), sessionId),
    setConnection: (hostId: string, state: string) => 
      page.evaluate(({ id, s }) => (window as any).__terminusTest.setConnection(id, s), { id: hostId, s: state }),
    
    // E2E-3: SyncStatus
    setSyncStatus: (state: string, lastError?: string) => 
      page.evaluate(
        ({ s, err }) => (window as any).__terminusTest.setSyncStatus(s, err),
        { s: state, err: lastError }
      ),
    openVault: () => page.evaluate(() => (window as any).__terminusTest.openVault()),

    // C9: SFTP browser
    seedSftpHost: (hostId?: string) =>
      page.evaluate((id) => (window as any).__terminusTest.seedSftpHost(id), hostId),
    sftpForceError: (kind: string, message: string) =>
      page.evaluate(
        ({ k, m }) => (window as any).__terminusTest.sftpForceError(k, m),
        { k: kind, m: message },
      ),
    sftpReset: (hostId?: string) =>
      page.evaluate((id) => (window as any).__terminusTest.sftpReset(id), hostId),
    lastSftpOpen: () =>
      page.evaluate(() => (window as any).__terminusTest.lastSftpOpen()),
    seedTofuHost: (hostId?: string) =>
      page.evaluate((id) => (window as any).__terminusTest.seedTofuHost(id), hostId),

    seedForwardHost: (hostId?: string) =>
      page.evaluate((id) => (window as any).__terminusTest.seedForwardHost(id), hostId),
    seedForward: (opts: {
      id?: string;
      hostId: string;
      name?: string;
      bindPort?: number;
      destPort?: number;
    }) => page.evaluate((o) => (window as any).__terminusTest.seedForward(o), opts),
    forwardFailNext: (message: string) =>
      page.evaluate((m) => (window as any).__terminusTest.forwardFailNext(m), message),

    seedSlowConnectHost: (hostId?: string, ms?: number) =>
      page.evaluate(
        ({ id, delay }) => (window as any).__terminusTest.seedSlowConnectHost(id, delay),
        { id: hostId, delay: ms },
      ),
    seedAuthFailHost: (hostId?: string) =>
      page.evaluate((id) => (window as any).__terminusTest.seedAuthFailHost(id), hostId),
    hostsRuntime: () =>
      page.evaluate(() => (window as any).__terminusTest.hostsRuntime()),

    // C9b: bidirectional SFTP (local pane + transfer)
    seedLocalDir: (path?: string) =>
      page.evaluate((p) => (window as any).__terminusTest.seedLocalDir(p), path),
    transferOps: () =>
      page.evaluate(() => (window as any).__terminusTest.transferOps()),
  };
}
