import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * #101 — Multiple open remote SFTP sessions
 */
test.describe("Multi-remote SFTP sessions (#101)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("AC1/AC2: opening B keeps A in open remotes; switch restores A", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");

    await bridge.openSftp("sftp-host-a");
    await expect(page.locator('[data-testid="sftp-toolbar"]')).toBeVisible({ timeout: 8000 });
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-a"]')).toHaveClass(/active/);

    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue(/docs/, { timeout: 5000 });

    await bridge.openSftp("sftp-host-b");
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-a"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-b"]')).toHaveClass(/active/);
    // #103: first remote stays on screen (split A|B), not only parked in the sidebar
    await expect(page.locator('[data-testid="sftp-pane-a"]')).toHaveAttribute(
      "data-endpoint",
      "sftp-host-a",
    );
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toHaveAttribute(
      "data-endpoint",
      "sftp-host-b",
    );
    await expect(
      page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-toolbar"]'),
    ).toBeVisible();
    await expect(
      page.locator('[data-testid="sftp-pane-b"] [data-testid="sftp-toolbar"]'),
    ).toBeVisible();

    await page.locator('[data-testid="sftp-open-remote-sftp-host-a"] [data-activate-remote]').click();
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-a"]')).toHaveClass(/active/);
    await expect(
      page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-path"]'),
    ).toHaveValue(/docs/, { timeout: 5000 });
    // Activating A must not collapse the split / drop B
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toHaveAttribute(
      "data-endpoint",
      "sftp-host-b",
    );
  });

  test("AC3: close removes remote from open list", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-host-a");
    await bridge.seedSftpHost("sftp-host-b");
    await bridge.openSftp("sftp-host-a");
    await bridge.openSftp("sftp-host-b");
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-a"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-b"]')).toBeVisible({ timeout: 5000 });

    await page.locator('[data-testid="sftp-open-remote-sftp-host-a"] [data-close-remote]').click();
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-a"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="sftp-open-remote-sftp-host-b"]')).toBeVisible();
  });
});
