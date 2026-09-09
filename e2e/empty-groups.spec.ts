import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

test.describe("Empty groups + host drag-drop", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
  });

  test("New group creates a visible empty group", async ({ page }) => {
    await page.locator("#btn-new-group").click();
    await expect(page.locator("#modal")).not.toHaveClass(/hidden/);
    await page.locator('[data-testid="group-name"]').fill("Staging empty");
    await page.locator('[data-testid="group-save"]').click();
    await expect(page.locator("#modal")).toHaveClass(/hidden/);

    const row = page.locator(".group-row").filter({ hasText: "Staging empty" });
    await expect(row).toBeVisible({ timeout: 5000 });
    await expect(row).toHaveClass(/expanded/);
    await expect(page.locator('[data-testid="group-empty"]')).toBeVisible();
    await expect(page.locator('[data-testid="group-empty"]')).toContainText("Drop hosts here");
  });

  test("drag host onto empty group assigns it", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedUngroupedHosts(1);
    await page.locator("#btn-new-group").click();
    await page.locator('[data-testid="group-name"]').fill("Drop target");
    await page.locator('[data-testid="group-save"]').click();
    await expect(page.locator(".group-row").filter({ hasText: "Drop target" })).toBeVisible({
      timeout: 5000,
    });

    const host = page.locator('[data-testid^="host-test-ungrouped-"]').first();
    const group = page.locator(".group-row").filter({ hasText: "Drop target" });
    await host.dragTo(group);
    await expect(page.locator(".group-children [data-testid^='host-test-ungrouped-']")).toHaveCount(1, {
      timeout: 5000,
    });
    await expect(page.locator("#panel-hosts > [data-testid^='host-test-ungrouped-']")).toHaveCount(0);
  });

  test("clicking a group row collapses and expands it", async ({ page }) => {
    await page.locator("#btn-new-group").click();
    await page.locator('[data-testid="group-name"]').fill("Collapsible");
    await page.locator('[data-testid="group-save"]').click();
    const row = page.locator(".group-row").filter({ hasText: "Collapsible" });
    await expect(row).toBeVisible({ timeout: 5000 });
    await expect(row).toHaveClass(/expanded/);
    await expect(row).toHaveAttribute("aria-expanded", "true");
    await expect(page.locator('[data-testid="group-empty"]')).toBeVisible();

    await row.click();
    await expect(row).not.toHaveClass(/expanded/);
    await expect(row).toHaveAttribute("aria-expanded", "false");
    await expect(page.locator('[data-testid="group-empty"]')).toHaveCount(0);

    await row.click();
    await expect(row).toHaveClass(/expanded/);
    await expect(page.locator('[data-testid="group-empty"]')).toBeVisible();
  });
});
