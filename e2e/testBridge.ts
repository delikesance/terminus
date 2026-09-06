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
  };
}
