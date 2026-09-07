import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

test.describe("TOFU Trust & Connect", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("Trust & Connect saves the key and does not reopen the sheet", async ({ page }) => {
    const bridge = getTestBridge(page);
    const hostId = await bridge.seedTofuHost("tofu-loop-host");

    await page.locator(`[data-testid="host-${hostId}"]`).click();
    await expect(page.locator("#sheet-tofu")).toBeVisible();
    await expect(page.locator('[data-testid="tofu-primary"]')).toHaveText("Trust & Connect");

    await page.locator('[data-testid="tofu-primary"]').click();

    await expect(page.locator("#modal")).toHaveClass(/hidden/);
    await expect(page.locator('[data-testid="tofu-primary"]')).toHaveCount(0);
    await expect(page.locator("#sheet-tofu")).toHaveCount(0);
    await expect(page.locator("h2", { hasText: "SSH failed" })).toHaveCount(0);
    await expect(page.locator(`[data-testid="host-${hostId}"] [data-testid="open-count-pill"]`)).toBeVisible();
  });
});
