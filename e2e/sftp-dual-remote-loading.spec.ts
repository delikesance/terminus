import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * #107 — Dual remote must not leave a pane stuck on Loading
 */
test.describe("Dual remote SFTP loading (#107)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("AC1: after open A then B, both panes list files (no stuck loading)", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");

    await bridge.openSftp("sftp-host-a");
    await expect(page.locator('[data-testid="sftp-row"]').first()).toBeVisible({ timeout: 8000 });
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
  });

  test("AC2: opening B while A list is slow still fills both panes", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");

    // Next sftp_list for A is slow — open B before it finishes (#107 race).
    await bridge.sftpSlowList("sftp-host-a", 1200);
    await bridge.openSftp("sftp-host-a");
    // Do not wait for A's rows — open B immediately to cancel/supersede A's load.
    await bridge.openSftp("sftp-host-b");

    const paneA = page.locator('[data-testid="sftp-pane-a"]');
    const paneB = page.locator('[data-testid="sftp-pane-b"]');
    await expect(paneA).toHaveAttribute("data-endpoint", "sftp-host-a");
    await expect(paneB).toHaveAttribute("data-endpoint", "sftp-host-b");

    await expect(paneB.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible({
      timeout: 10000,
    });
    // A must not remain on permanent Loading skeleton
    await expect(paneA.locator('[data-testid="sftp-loading"]')).toHaveCount(0, { timeout: 15000 });
    await expect(paneA.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible({
      timeout: 15000,
    });
  });
});
