import { test, expect } from '@playwright/test';
import { waitForTestBridge, getTestBridge } from './testBridge';

/**
 * E2E-3: SyncStatus badge — 5 distinct states
 * QA Contract from PR #6 / Issue #4
 * 
 * Selectors: sync-badge[data-state], sync-detail-error
 * Bridge: window.__terminusTest.setSyncStatus(state, last_error?)
 */

test.describe('E2E-3: SyncStatus badge with 5 states (QA Contract)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await waitForTestBridge(page);
  });

  test('displays unconfigured state with "Sync non configuré" copy', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.setSyncStatus('unconfigured');
    
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toBeVisible();
    await expect(badge).toHaveAttribute('data-state', 'unconfigured');
    await expect(badge).toContainText('Sync non configuré');
  });

  test('displays offline state with "Hors ligne" copy (distinct from unconfigured)', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.setSyncStatus('offline');
    
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toHaveAttribute('data-state', 'offline');
    await expect(badge).toContainText('Hors ligne');
    
    // Verify it's different from unconfigured
    await expect(badge).not.toContainText('Sync non configuré');
  });

  test('displays idle state with "À jour" copy', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.setSyncStatus('idle');
    
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toHaveAttribute('data-state', 'idle');
    await expect(badge).toContainText('À jour');
  });

  test('displays syncing state with "Synchronisation..." copy', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.setSyncStatus('syncing');
    
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toHaveAttribute('data-state', 'syncing');
    await expect(badge).toContainText('Synchronisation');
  });

  test('displays error state with "Erreur de sync" copy', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.setSyncStatus('error', 'Auth failed: invalid credentials');
    
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toHaveAttribute('data-state', 'error');
    await expect(badge).toContainText('Erreur de sync');
  });

  test('all 5 states have distinct data-state attributes', async ({ page }) => {
    const bridge = getTestBridge(page);
    const states = ['unconfigured', 'idle', 'syncing', 'offline', 'error'];
    
    for (const state of states) {
      await bridge.setSyncStatus(state, state === 'error' ? 'Test error' : undefined);
      
      const badge = page.locator('[data-testid="sync-badge"]');
      await expect(badge).toHaveAttribute('data-state', state);
    }
  });

  test('syncing → idle transition clears last_error', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Start with error state
    await bridge.setSyncStatus('error', 'Previous error message');
    
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toHaveAttribute('data-state', 'error');
    
    // Transition to syncing
    await bridge.setSyncStatus('syncing');
    await expect(badge).toHaveAttribute('data-state', 'syncing');
    
    // Then to idle
    await bridge.setSyncStatus('idle');
    await expect(badge).toHaveAttribute('data-state', 'idle');
    
    // last_error should be cleared (no error detail visible)
    const errorDetail = page.locator('[data-testid="sync-detail-error"]');
    await expect(errorDetail).not.toBeVisible();
  });

  test('error state shows last_error in detail panel', async ({ page }) => {
    const bridge = getTestBridge(page);
    const errorMessage = 'Connection timeout: could not reach sync server';
    
    await bridge.setSyncStatus('error', errorMessage);
    
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toHaveAttribute('data-state', 'error');
    
    // Click badge to open detail (if clickable)
    await badge.click();
    
    // Error detail should be visible with the message
    const errorDetail = page.locator('[data-testid="sync-detail-error"]');
    await expect(errorDetail).toBeVisible();
    await expect(errorDetail).toContainText(errorMessage);
  });

  test('unconfigured ≠ offline: distinct copy validates', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Test unconfigured
    await bridge.setSyncStatus('unconfigured');
    let badge = page.locator('[data-testid="sync-badge"]');
    const unconfiguredText = await badge.textContent();
    expect(unconfiguredText).toContain('Sync non configuré');
    
    // Test offline
    await bridge.setSyncStatus('offline');
    badge = page.locator('[data-testid="sync-badge"]');
    const offlineText = await badge.textContent();
    expect(offlineText).toContain('Hors ligne');
    
    // Verify they are different
    expect(unconfiguredText).not.toEqual(offlineText);
  });

  test('badge is clickable (role="button" or interactive)', async ({ page }) => {
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toBeVisible();
    
    // Should be clickable (verify it doesn't throw)
    await badge.click();
    
    // Detail panel or modal should appear (depends on implementation)
    // At minimum, click should be allowed
  });

  test('mid-sync error sets state to error with last_error', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Start sync
    await bridge.setSyncStatus('syncing');
    const badge = page.locator('[data-testid="sync-badge"]');
    await expect(badge).toHaveAttribute('data-state', 'syncing');
    
    // Simulate mid-sync failure
    await bridge.setSyncStatus('error', 'Sync interrupted: network failure');
    
    await expect(badge).toHaveAttribute('data-state', 'error');
    
    // Click to see error
    await badge.click();
    const errorDetail = page.locator('[data-testid="sync-detail-error"]');
    await expect(errorDetail).toBeVisible();
    await expect(errorDetail).toContainText('network failure');
  });
});
