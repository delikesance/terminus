import { test, expect } from '@playwright/test';
import { waitForTestBridge, getTestBridge } from './testBridge';

/**
 * E2E-2: Connection dots — distinct from open_count
 * QA Contract from PR #6 / Issue #3
 * 
 * Selectors: host-{id}, host-local, connection-dot[data-state], open-count-pill
 * Bridge: window.__terminusTest.{sessionOpenSsh, sessionClose, setConnection}
 */

test.describe('E2E-2: Connection dots distinct from open_count (QA Contract)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await waitForTestBridge(page);
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

  test('CRITICAL: close last shell → connection=disconnected + pill absent', async ({ page }) => {
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
    
    // Verify pill is hidden for a single session (dot is enough)
    const openCountPill = hostItem.locator('[data-testid="open-count-pill"]');
    await expect(openCountPill).toHaveCount(0);
    
    // Close the last session
    await bridge.sessionClose(sessionId);
    
    // Last shell gone → gray disconnected dot, no count pill
    await expect(connectionDot).toHaveAttribute('data-state', 'disconnected');
    await expect(openCountPill).toHaveCount(0);
  });

  test('host with multiple sessions shows a muted count, single session does not', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    const testHostId = 'test-host-both-indicators';
    
    await bridge.setConnection(testHostId, 'connected');
    await bridge.sessionOpenSsh(testHostId);
    
    const hostItem = page.locator(`[data-testid="host-${testHostId}"]`);
    const connectionDot = hostItem.locator('[data-testid="connection-dot"]');
    const openCountPill = hostItem.locator('[data-testid="open-count-pill"]');
    
    await expect(connectionDot).toBeVisible();
    await expect(connectionDot).toHaveAttribute('data-state', 'connected');
    await expect(openCountPill).toHaveCount(0);

    await bridge.sessionOpenSsh(testHostId);
    await expect(openCountPill).toBeVisible();
    await expect(openCountPill).toHaveText('×2');
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

  test('AC1: connecting is visible during slow SSH handshake via hosts://runtime', async ({ page }) => {
    const bridge = getTestBridge(page);
    const hostId = await bridge.seedSlowConnectHost('test-host-ac1-connecting', 800);

    const connectPromise = page.locator(`[data-testid="host-${hostId}"]`).click();
    const connectionDot = page.locator(
      `[data-testid="host-${hostId}"] [data-testid="connection-dot"]`,
    );

    // Backend emits connecting mid-flight; UI must apply hosts://runtime (Red until listener exists).
    await expect(connectionDot).toHaveAttribute('data-state', 'connecting', { timeout: 600 });
    await connectPromise;
    await expect(connectionDot).toHaveAttribute('data-state', 'connected');
  });

  test('AC2: auth failure leaves sticky error on hosts_runtime', async ({ page }) => {
    const bridge = getTestBridge(page);
    const hostId = await bridge.seedAuthFailHost('test-host-ac2-error');

    await page.locator(`[data-testid="host-${hostId}"]`).click();
    await expect(page.locator('h2', { hasText: 'SSH failed' })).toBeVisible();
    await page.locator('#ssh-fail-ok').click();

    const runtimes = await bridge.hostsRuntime();
    const rt = runtimes.find((r) => r.host_id === hostId);
    expect(rt?.connection).toBe('error');

    const connectionDot = page.locator(
      `[data-testid="host-${hostId}"] [data-testid="connection-dot"]`,
    );
    // Dot must reflect sticky error (Red until UI listens / refreshes runtime).
    await expect(connectionDot).toHaveAttribute('data-state', 'error');
  });

  test('AC4: TOFU cancel must leave host disconnected not error', async ({ page }) => {
    const bridge = getTestBridge(page);
    const hostId = await bridge.seedTofuHost('test-host-ac4-tofu-cancel');

    await page.locator(`[data-testid="host-${hostId}"]`).click();
    await expect(page.locator('#sheet-tofu')).toBeVisible();
    await page.locator('[data-testid="tofu-cancel"]').click();
    await expect(page.locator('#sheet-tofu')).toHaveCount(0);

    const runtimes = await bridge.hostsRuntime();
    const rt = runtimes.find((r) => r.host_id === hostId);
    // Mock currently mirrors Rust bug (HostKey → error). Desired: disconnected.
    expect(rt?.connection).toBe('disconnected');
  });
});
