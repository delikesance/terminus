import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * C7 smoke: empty states (hosts / search / snips / history / sftp)
 * Onboarding is suppressed under VITE_E2E so existing suites stay green.
 */

test.describe("C7: empty states smoke", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("shows No hosts yet empty with Add host + Import when 0 hosts", async ({ page }) => {
    await expect(page.locator('[data-testid="host-local"]')).toBeVisible();
    await expect(page.locator('[data-testid="group-ungrouped"]')).toBeVisible();
    const empty = page.locator('[data-testid="empty-hosts"]');
    await expect(empty).toBeVisible();
    await expect(empty).toContainText("No hosts yet");
    await expect(page.locator('[data-testid="empty-add-host"]')).toBeVisible();
    await expect(page.locator('[data-testid="empty-import-hosts"]')).toBeVisible();
  });

  test("active search with 0 match shows No hosts match (not zero-host empty)", async ({ page }) => {
    await page.locator("#host-filter").fill("zzz-no-such-host");
    await expect(page.locator('[data-testid="empty-hosts-match"]')).toBeVisible();
    await expect(page.locator('[data-testid="empty-hosts-match"]')).toContainText("No hosts match");
    await expect(page.locator('[data-testid="empty-hosts"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="empty-add-host"]')).toHaveCount(0);
  });

  test("snippets / history / sftp dedicated empties", async ({ page }) => {
    await page.locator('.side-nav button[data-panel="snippets"]').click();
    await expect(page.locator('[data-testid="empty-snippets"]')).toBeVisible();
    await expect(page.locator('[data-testid="empty-snippets"]')).toContainText("No snippets yet");

    await page.locator('.side-nav button[data-panel="history"]').click();
    await expect(page.locator('[data-testid="empty-history"]')).toBeVisible();
    await expect(page.locator('[data-testid="empty-history"]')).toContainText("No history yet");

    await page.locator('.side-nav button[data-panel="sftp"]').click();
    await expect(page.locator('[data-testid="empty-sftp"]')).toBeVisible();
    await expect(page.locator('[data-testid="empty-sftp"]')).toContainText("No host to browse");
  });

  test("Add host CTA opens host sheet", async ({ page }) => {
    await page.locator('[data-testid="empty-add-host"]').click();
    await expect(page.locator("#modal")).not.toHaveClass(/hidden/);
    await expect(page.locator("#modal-sheet h2")).toContainText("New host");
  });

  test("onboarding checklist once via e2e opt-in", async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.removeItem("terminus.onboarded");
      localStorage.setItem("terminus.e2e.showOnboard", "1");
    });
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    await expect(page.locator('[data-testid="onboard-steps"]')).toBeVisible();
    await expect(page.locator('[data-testid="onboard-go"]')).toBeVisible();
    await page.locator('[data-testid="onboard-skip"]').click();
    await expect(page.locator("#modal")).toHaveClass(/hidden/);
    const flagged = await page.evaluate(() => localStorage.getItem("terminus.onboarded"));
    expect(flagged).toBe("1");
  });
});
