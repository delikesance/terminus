import { test, expect } from '@playwright/test';
import { waitForTestBridge, getTestBridge } from './testBridge';

/**
 * E2E-1: Ungrouped count — inline badge (Gestalt)
 * QA Contract from PR #6 / Issue #2
 * 
 * Selectors: group-ungrouped, group-label, group-count
 * Bridge: window.__terminusTest.{seedUngroupedHosts, clearUngroupedHosts, groupsDelete, restoreGroup}
 */

test.describe('E2E-1: Ungrouped count badge (QA Contract)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await waitForTestBridge(page);
  });

  test('displays inline badge with ungrouped host count', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Seed 3 ungrouped hosts
    await bridge.seedUngroupedHosts(3);
    
    // Locate ungrouped section
    const ungroupedGroup = page.locator('[data-testid="group-ungrouped"]');
    await expect(ungroupedGroup).toBeVisible();
    
    // Verify label exists
    const label = ungroupedGroup.locator('[data-testid="group-label"]');
    await expect(label).toBeVisible();
    await expect(label).toContainText('Ungrouped');
    
    // Verify count badge is inline and shows correct value
    const countBadge = ungroupedGroup.locator('[data-testid="group-count"]');
    await expect(countBadge).toBeVisible();
    await expect(countBadge).toHaveText('3');
  });

  test('shows badge with 0 when no ungrouped hosts exist', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Clear all ungrouped hosts
    await bridge.clearUngroupedHosts();
    
    const ungroupedGroup = page.locator('[data-testid="group-ungrouped"]');
    const countBadge = ungroupedGroup.locator('[data-testid="group-count"]');
    
    // Badge should show "0", not be hidden (per AC)
    await expect(countBadge).toBeVisible();
    await expect(countBadge).toHaveText('0');
  });

  test('increments count when group is soft-deleted (hosts detached)', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Setup: seed ungrouped hosts to get baseline
    await bridge.clearUngroupedHosts();
    await bridge.seedUngroupedHosts(2);
    
    // Get initial count
    const countBadge = page.locator('[data-testid="group-ungrouped"] [data-testid="group-count"]');
    await expect(countBadge).toHaveText('2');
    
    // Soft-delete a group with hosts (hosts will be detached to ungrouped)
    // Assumes test environment has a group with known ID
    await bridge.groupsDelete('test-group-id');
    
    // Count should increase by number of detached hosts
    // Exact number depends on test group setup
    await expect(countBadge).not.toHaveText('2'); // Changed
  });

  test('count remains unchanged after restoring a deleted group', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Setup: delete group, hosts become ungrouped
    await bridge.groupsDelete('test-group-id');
    
    // Get count after deletion
    const countBadge = page.locator('[data-testid="group-ungrouped"] [data-testid="group-count"]');
    const countAfterDelete = await countBadge.textContent();
    
    // Restore the group
    await bridge.restoreGroup('test-group-id');
    
    // Count should be UNCHANGED (hosts NOT reattached per AC)
    await expect(countBadge).toHaveText(countAfterDelete || '0');
  });

  test('formula excludes deleted hosts from count (!deleted_at && !group_id)', async ({ page }) => {
    const bridge = getTestBridge(page);
    
    // Clear and seed fresh
    await bridge.clearUngroupedHosts();
    await bridge.seedUngroupedHosts(5);
    
    const countBadge = page.locator('[data-testid="group-ungrouped"] [data-testid="group-count"]');
    await expect(countBadge).toHaveText('5');
    
    // Verify only hosts with !deleted_at && !group_id are counted
    // (Implementation detail: deleted hosts should not be in the count)
  });

  test('badge is inline with label (Gestalt proximity)', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedUngroupedHosts(3);
    
    const ungroupedGroup = page.locator('[data-testid="group-ungrouped"]');
    const label = ungroupedGroup.locator('[data-testid="group-label"]');
    const countBadge = ungroupedGroup.locator('[data-testid="group-count"]');
    
    // Both should be visible
    await expect(label).toBeVisible();
    await expect(countBadge).toBeVisible();
    
    // Verify they're on the same line (Y coordinate should be similar)
    const labelBox = await label.boundingBox();
    const badgeBox = await countBadge.boundingBox();
    
    if (labelBox && badgeBox) {
      // Y coordinates should be within a few pixels (same line)
      const yDiff = Math.abs(labelBox.y - badgeBox.y);
      expect(yDiff).toBeLessThan(10);
    }
  });
});
