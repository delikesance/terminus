import { test, expect } from "@playwright/test";
import { waitForTestBridge } from "./testBridge";

test.describe("Activity Bar + contextual sidebar (#27)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
  });

  test("AC1: activity bar is 48px with five activities", async ({ page }) => {
    const bar = page.locator('[data-testid="activity-bar"]');
    await expect(bar).toBeVisible();
    const box = await bar.boundingBox();
    expect(box?.width).toBe(48);
    await expect(bar.locator("[data-activity]")).toHaveCount(5);
  });

  test("AC2: switching activity shows contextual panel", async ({ page }) => {
    await page.locator('#activity-bar button[data-activity="snippets"]').click();
    await expect(page.locator("#panel-snippets")).toBeVisible();
    await expect(page.locator("#panel-hosts")).toBeHidden();
    await expect(page.locator('#activity-bar button[data-activity="snippets"]')).toHaveClass(/active/);
  });

  test("AC3/AC4: re-click collapses; other click reopens", async ({ page }) => {
    const app = page.locator("#app");
    await expect(app).toHaveClass(/sidebar-open/);
    await page.locator('#activity-bar button[data-activity="hosts"]').click();
    await expect(app).not.toHaveClass(/sidebar-open/);
    await page.locator('#activity-bar button[data-activity="history"]').click();
    await expect(app).toHaveClass(/sidebar-open/);
    await expect(page.locator("#panel-history")).toBeVisible();
  });
});
