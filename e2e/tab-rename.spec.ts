import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

async function ensureActivePane(page: import("@playwright/test").Page) {
  await page.locator(".pane.active").first().waitFor({ state: "visible", timeout: 4000 }).catch(() => {});
  if (!(await page.locator(".pane.active").count())) {
    await page.locator('[data-testid="host-local"]').click();
    await page.locator(".pane.active").first().waitFor({ timeout: 4000 });
  }
}

test.describe("Tab renaming", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("Tab context menu contains Rename action and inline input renames tab", async ({ page }) => {
    await ensureActivePane(page);
    const tab = page.locator("#tabs [data-tab]").first();
    await expect(tab).toBeVisible();

    // Right-click tab
    await tab.click({ button: "right" });
    const menu = page.locator("#ctx-menu");
    await expect(menu).toBeVisible();
    const renameBtn = menu.locator('[data-testid="tab-rename"]');
    await expect(renameBtn).toBeVisible();
    await renameBtn.click();

    // The rename input should now be visible within the tab
    const input = tab.locator('[data-testid="tab-rename-input"]');
    await expect(input).toBeVisible();
    await expect(input).toBeFocused();

    // Type a new name and press Enter
    await input.fill("Custom Server Name");
    await input.press("Enter");

    // The tab label and title should now display the custom name
    await expect(input).not.toBeVisible();
    const label = tab.locator(".label");
    await expect(label).toHaveText("Custom Server Name");
    await expect(tab).toHaveAttribute("title", "Custom Server Name");

    // Context menu should now also contain 'Reset name'
    await tab.click({ button: "right" });
    await expect(menu).toBeVisible();
    const resetBtn = menu.locator('[data-testid="tab-reset-name"]');
    await expect(resetBtn).toBeVisible();
    await resetBtn.click();

    // After reset, title goes back to default ('This computer')
    await expect(label).toHaveText("This computer");
  });

  test("Double-clicking a tab initiates rename mode", async ({ page }) => {
    await ensureActivePane(page);
    const tab = page.locator("#tabs [data-tab]").first();
    await expect(tab).toBeVisible();

    // Double click the tab
    await tab.dblclick();

    const input = tab.locator('[data-testid="tab-rename-input"]');
    await expect(input).toBeVisible();
    await expect(input).toBeFocused();

    // Press Escape to cancel
    await input.fill("Should Not Save");
    await input.press("Escape");

    await expect(input).not.toBeVisible();
    const label = tab.locator(".label");
    await expect(label).toHaveText("This computer");
  });

  test("F2 shortcut initiates tab rename when terminal is focused", async ({ page }) => {
    await ensureActivePane(page);
    const pane = page.locator(".pane.active").first();
    await pane.click();

    await page.keyboard.press("F2");

    const tab = page.locator("#tabs [data-tab]").first();
    const input = tab.locator('[data-testid="tab-rename-input"]');
    await expect(input).toBeVisible();
    await expect(input).toBeFocused();

    await input.fill("Renamed Via F2");
    await input.press("Enter");

    const label = tab.locator(".label");
    await expect(label).toHaveText("Renamed Via F2");
  });
});
