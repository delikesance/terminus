import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * #107 / #109 — Dual remote loading without canceling or re-listing the first pane
 */
test.describe("Dual remote SFTP loading (#107/#109)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("AC1: after A loaded, opening B does not re-list A", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");

    await bridge.openSftp("sftp-host-a");
    await expect(page.locator('[data-testid="sftp-row"]').first()).toBeVisible({ timeout: 8000 });
    const countsAfterA = await bridge.sftpListCounts();
    expect(countsAfterA["sftp-host-a"] ?? 0).toBeGreaterThanOrEqual(1);
    const aListsBeforeB = countsAfterA["sftp-host-a"] ?? 0;

    await bridge.openSftp("sftp-host-b");

    const paneA = page.locator('[data-testid="sftp-pane-a"]');
    const paneB = page.locator('[data-testid="sftp-pane-b"]');
    await expect(paneA).toHaveAttribute("data-endpoint", "sftp-host-a");
    await expect(paneB).toHaveAttribute("data-endpoint", "sftp-host-b");

    await expect(paneA.locator('[data-testid="sftp-loading"]')).toHaveCount(0, { timeout: 10000 });
    await expect(paneB.locator('[data-testid="sftp-loading"]')).toHaveCount(0, { timeout: 10000 });
    await expect(paneA.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible({
      timeout: 10000,
    });
    await expect(paneB.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible({
      timeout: 10000,
    });

    const countsAfterB = await bridge.sftpListCounts();
    expect(countsAfterB["sftp-host-a"] ?? 0).toBe(aListsBeforeB);
  });

  test("AC2: opening B while A list is in flight does not start a second A list", async ({
    page,
  }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");

    await bridge.sftpListCountsReset();
    await bridge.sftpSlowList("sftp-host-a", 1200);
    await bridge.openSftp("sftp-host-a");
    // Open B before A's list finishes — must not cancel or re-fetch A (#109).
    await bridge.openSftp("sftp-host-b");

    const paneA = page.locator('[data-testid="sftp-pane-a"]');
    const paneB = page.locator('[data-testid="sftp-pane-b"]');
    await expect(paneA).toHaveAttribute("data-endpoint", "sftp-host-a");
    await expect(paneB).toHaveAttribute("data-endpoint", "sftp-host-b");

    await expect(paneB.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible({
      timeout: 10000,
    });
    await expect(paneA.locator('[data-testid="sftp-loading"]')).toHaveCount(0, { timeout: 15000 });
    await expect(paneA.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible({
      timeout: 15000,
    });

    const counts = await bridge.sftpListCounts();
    expect(counts["sftp-host-a"] ?? 0).toBe(1);
  });
});
