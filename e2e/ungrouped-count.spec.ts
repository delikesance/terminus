import { test, expect } from '@playwright/test';
import { waitForTestBridge, getTestBridge } from './testBridge';

/**
 * Hosts without a group sit in the main list.
 * A group row only appears for real groups.
 */

test.describe('Ungrouped hosts are a flat list', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await waitForTestBridge(page);
  });

  test('does not render an Ungrouped section', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedUngroupedHosts(3);

    await expect(page.locator('[data-testid="group-ungrouped"]')).toHaveCount(0);
    await expect(page.getByText('Ungrouped', { exact: true })).toHaveCount(0);
    await expect(page.locator('[data-testid^="host-test-ungrouped-"]')).toHaveCount(3);
  });

  test('flat hosts are not nested under a group', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.clearUngroupedHosts();
    await bridge.seedUngroupedHosts(2);

    const nested = page.locator('.group-children [data-testid^="host-test-ungrouped-"]');
    await expect(nested).toHaveCount(0);
    await expect(page.locator('#panel-hosts > [data-testid^="host-test-ungrouped-"]')).toHaveCount(2);
  });

  test('a real group still appears as its own row', async ({ page }) => {
    await expect(page.locator('.group-row').filter({ hasText: 'Test Group' })).toBeVisible();
    await expect(page.locator('[data-testid="group-ungrouped"]')).toHaveCount(0);
  });

  test('deleting a group returns its hosts to the flat list', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.clearUngroupedHosts();
    await bridge.groupsDelete('test-group-id');

    await expect(page.locator('.group-row').filter({ hasText: 'Test Group' })).toHaveCount(0);
    await expect(page.locator('[data-testid="group-ungrouped"]')).toHaveCount(0);
    await expect(page.locator('#panel-hosts > [data-testid="host-test-group-host-0"]')).toBeVisible();
    await expect(page.locator('#panel-hosts > [data-testid="host-test-group-host-1"]')).toBeVisible();
  });

  test('restoring a deleted group does not reattach hosts', async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.groupsDelete('test-group-id');
    await bridge.restoreGroup('test-group-id');

    await expect(page.locator('#panel-hosts > [data-testid="host-test-group-host-0"]')).toBeVisible();
    await expect(page.locator('.group-children [data-testid="host-test-group-host-0"]')).toHaveCount(0);
  });
});
