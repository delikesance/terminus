import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * #105 — Dual remote SFTP panes with cross-host transfer
 */
test.describe("Dual remote SFTP transfer (#105)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("AC1/AC2: two remotes browse independently in split", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");

    await bridge.openSftp("sftp-host-a");
    await bridge.openSftp("sftp-host-b");

    const paneA = page.locator('[data-testid="sftp-pane-a"]');
    const paneB = page.locator('[data-testid="sftp-pane-b"]');
    await expect(paneA).toHaveAttribute("data-endpoint", "sftp-host-a");
    await expect(paneB).toHaveAttribute("data-endpoint", "sftp-host-b");

    await paneA.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(paneA.locator('[data-testid="sftp-path"]')).toHaveValue(/docs/, { timeout: 5000 });
    // B stays at home while A navigates
    await expect(paneB.locator('[data-testid="sftp-path"]')).toHaveValue(/\/home\/lab$/, {
      timeout: 5000,
    });

    await paneB.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(paneB.locator('[data-testid="sftp-path"]')).toHaveValue(/docs/, { timeout: 5000 });
    // A keeps its docs path after B navigates
    await expect(paneA.locator('[data-testid="sftp-path"]')).toHaveValue(/docs/, { timeout: 5000 });
  });

  test("AC3: transfer selected file from remote A to remote B via ↑", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");

    await bridge.openSftp("sftp-host-a");
    await bridge.openSftp("sftp-host-b");

    const paneA = page.locator('[data-testid="sftp-pane-a"]');
    const paneB = page.locator('[data-testid="sftp-pane-b"]');
    await expect(paneA).toHaveAttribute("data-endpoint", "sftp-host-a");
    await expect(paneB).toHaveAttribute("data-endpoint", "sftp-host-b");

    await paneA
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "remote-only.txt" })
      .locator('[data-testid="sftp-select"]')
      .click();

    const up = page.locator('[data-testid="sftp-tx-up"]');
    await expect(up).toBeEnabled({ timeout: 5000 });
    await up.click();

    await expect(
      paneB.locator('[data-testid="sftp-row"]').filter({ hasText: "remote-only.txt" }),
    ).toBeVisible({ timeout: 10000 });
    await expect.poll(async () => (await bridge.transferOps()).uploads).toBeGreaterThanOrEqual(1);
    await expect(page.locator('[data-testid="sftp-error"]')).toHaveCount(0);
  });

  test("AC6: local↔remote upload still works beside dual-remote support", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-e2e-host");
    await bridge.seedLocalDir();

    await page.locator('#activity-bar button[data-activity="sftp"]').click();
    await expect(page.locator('[data-testid="sftp-toolbar"]')).toBeVisible({ timeout: 5000 });
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "local-upload.txt" })).toBeVisible({
      timeout: 5000,
    });
    await page
      .locator('[data-testid="local-row"]')
      .filter({ hasText: "local-upload.txt" })
      .locator('[data-testid="local-select"]')
      .click();
    await expect(page.locator('[data-testid="sftp-tx-up"]')).toBeEnabled();
    await page.locator('[data-testid="sftp-tx-up"]').click();
    await expect(
      page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]').filter({
        hasText: "local-upload.txt",
      }),
    ).toBeVisible({ timeout: 10000 });
    await expect.poll(async () => (await bridge.transferOps()).uploads).toBeGreaterThanOrEqual(1);
  });
});
