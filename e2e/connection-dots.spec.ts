import { test, expect, Page } from '@playwright/test';

/**
 * E2E-2: Connection dots — distinct from open_count
 * QA Contract from PR #6 / Issue #3
 * 
 * Selectors: host-{id}, host-local, connection-dot[data-state], open-count-pill
 * Bridge: window.__terminusTest.{sessionOpenSsh, sessionClose}
 */

const testBridge = (page: Page) => ({
  sessionOpenSsh: (hostId: string) => 
    page.evaluate((id) => (window as any).__terminusTest.sessionOpenSsh(id), hostId),
  sessionClose: (sessionId: string) => 
    page.evaluate((id) => (window as any).__terminusTest.sessionClose(id), sessionId),
  setConnection: (hostId: string, state: string) => 
    page.evaluate(({ id, s }) => (window as any).__terminusTest.setConnection(id, s), { id: hostId, s: state }),
});

test.describe('E2E-2: Connection dots distinct from open_count (QA Contract)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('This computer shows local connection dot', async ({ page }) => {
    const localItem = page.locator('[data-testid="host-local"]');
    await expect(localItem).toBeVisible();
    
    // Connection dot should exist with state="local"
    const connectionDot = localItem.locator('[data-testid="connection-dot"]');
    await expect(connectionDot).toBeVisible();
    await expect(connectionDot).toHaveAttribute('data-state', 'local');
  });

  test('connection dot and open-count pill are distinct elements', async ({ page }) => {
    const localItem = page.locator('[data-testid="host-local"]');
    
    // Connection dot should always be present
    const connectionDot = localItem.locator('[data-testid="connection-dot"]');
    await expect(connectionDot).toBeVisible();
    
    // Open count pill (if visible, depends on open sessions)
    const openCountPill = localItem.locator('[data-testid="open-count-pill"]');
    const pillVisible = await openCountPill.isVisible();
    
    // If pill is visible, verify they are separate elements
    if (pillVisible) {
      const dotBox = await connectionDot.boundingBox();
      const pillBox = await openCountPill.boundingBox();
      
      // They should not have identical positions
      expect(dotBox?.x !== pillBox?.x || dotBox?.y !== pillBox?.y).toBeTruthy();
    }
  });

  test('CRITICAL: close last shell → connection=connected + pill absent', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Setup: set host connection to "connected" (no docker/sshd needed)
    const testHostId = 'test-host-critical';
    await bridge.setConnection(testHostId, 'connected');
    
    // Open session on the host
    const sessionId = await bridge.sessionOpenSsh(testHostId);
    
    // Verify connection dot shows "connected"
    const hostItem = page.locator(`[data-testid="host-${testHostId}"]`);
    const connectionDot = hostItem.locator('[data-testid="connection-dot"]');
    await expect(connectionDot).toHaveAttribute('data-state', 'connected');
    
    // Verify pill shows count ≥ 1
    const openCountPill = hostItem.locator('[data-testid="open-count-pill"]');
    await expect(openCountPill).toBeVisible();
    
    // Close the last session
    await bridge.sessionClose(sessionId);
    
    // CRITICAL AC: connection dot remains "connected", pill disappears
    await expect(connectionDot).toHaveAttribute('data-state', 'connected');
    await expect(openCountPill).not.toBeVisible();
  });

  test('host with open sessions shows both dot and pill', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    const testHostId = 'test-host-both-indicators';
    
    // Setup connection state
    await bridge.setConnection(testHostId, 'connected');
    
    // Open session
    await bridge.sessionOpenSsh(testHostId);
    
    const hostItem = page.locator(`[data-testid="host-${testHostId}"]`);
    
    // Both should be visible
    const connectionDot = hostItem.locator('[data-testid="connection-dot"]');
    const openCountPill = hostItem.locator('[data-testid="open-count-pill"]');
    
    await expect(connectionDot).toBeVisible();
    await expect(connectionDot).toHaveAttribute('data-state', 'connected');
    await expect(openCountPill).toBeVisible();
  });

  test('all connection states are distinct: local, connected, disconnected, connecting, error', async ({ page }) => {
    const bridge = getTestBridge(page);
    const states: Array<[string, string]> = [
      ['connected', 'test-host-conn-connected'],
      ['disconnected', 'test-host-conn-disconnected'],
      ['connecting', 'test-host-conn-connecting'],
      ['error', 'test-host-conn-error'],
    ];
    
    // Set each connection state and verify
    for (const [state, hostId] of states) {
      await bridge.setConnection(hostId, state);
      
      const hostItem = page.locator(`[data-testid="host-${hostId}"]`);
      const connectionDot = hostItem.locator('[data-testid="connection-dot"]');
      
      await expect(connectionDot).toHaveAttribute('data-state', state);
    }
    
    // Local state is always present for "This computer"
    const localDot = page.locator('[data-testid="host-local"] [data-testid="connection-dot"]');
    await expect(localDot).toHaveAttribute('data-state', 'local');
  });

  test('connection dot has accessible attributes', async ({ page }) => {
    const localItem = page.locator('[data-testid="host-local"]');
    const connectionDot = localItem.locator('[data-testid="connection-dot"]');
    
    await expect(connectionDot).toBeVisible();
    
    // Should have data-state for styling/testing
    const state = await connectionDot.getAttribute('data-state');
    expect(state).toBeTruthy();
    expect(['local', 'connected', 'disconnected', 'connecting', 'error']).toContain(state);
  });

  test('disconnected host shows gray dot, no pill if open_count=0', async ({ page }) => {
    const bridge = getTestBridge(page);
    const testHostId = 'test-host-disconnected-no-sessions';
    
    // Setup: host is disconnected with no open sessions
    await bridge.setConnection(testHostId, 'disconnected');
    
    const hostItem = page.locator(`[data-testid="host-${testHostId}"]`);
    const connectionDot = hostItem.locator('[data-testid="connection-dot"]');
    const pill = hostItem.locator('[data-testid="open-count-pill"]');
    
    // Verify disconnected state
    await expect(connectionDot).toBeVisible();
    await expect(connectionDot).toHaveAttribute('data-state', 'disconnected');
    
    // No pill when open_count=0
    await expect(pill).not.toBeVisible();
  });
});
