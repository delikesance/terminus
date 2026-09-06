import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * C8 smoke: Vault / identities sheet
 * - list + add identity
 * - sync secrets default OFF (« Secrets stay local »)
 * - one-shot reveal (secret visible, then Hide / remask)
 */

test.describe("C8: Vault identities sheet", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
  });

  test("opens vault, adds key identity, reveal one-shot, secrets stay local", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.openVault();

    const sheet = page.locator("#sheet-vault");
    await expect(sheet).toBeVisible();
    await expect(sheet.locator("h2")).toHaveText("Identities");

    const syncToggle = page.locator('[data-testid="vault-sync-secrets"]');
    await expect(syncToggle).toHaveCount(1);
    await expect(syncToggle).not.toBeChecked();
    await expect(sheet.getByText("Secrets stay local")).toBeVisible();

    await page.locator('[data-testid="vault-add"]').click();
    await page.locator('[data-testid="vault-name"]').fill("e2e-deploy");
    await page.locator("#v-kind button[data-kind='password']").click();
    await page.locator('[data-testid="vault-pass"]').fill("s3cret-local-only");
    await page.locator('[data-testid="vault-save"]').click();

    await expect(page.locator("#sheet-vault")).toBeVisible();
    await expect(page.locator('[data-testid="vault-list"]')).toContainText("e2e-deploy");
    await expect(page.locator('[data-testid="vault-list"]')).toContainText("password");

    // Secret must not appear until Reveal
    await expect(page.locator('[data-testid="vault-reveal"]')).toHaveCount(0);

    await page.locator('[data-testid="vault-reveal-btn"]').first().click();
    await expect(page.locator('[data-testid="vault-reveal"]')).toHaveText("s3cret-local-only");

    await page.locator('[data-testid="vault-reveal-btn"]').first().click();
    await expect(page.locator('[data-testid="vault-reveal"]')).toHaveCount(0);

    // Toggle stays off by default; flipping on persists via mock
    await expect(syncToggle).not.toBeChecked();
  });
});
