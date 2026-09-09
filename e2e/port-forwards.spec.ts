import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

test.describe("Port Forwarding sidebar view (#28)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("AC1: Activity Bar opens forwards panel empty state", async ({ page }) => {
    await page.locator('#activity-bar button[data-activity="forwards"]').click();
    await expect(page.locator("#panel-forwards")).toBeVisible();
    await expect(page.locator("#panel-hosts")).toBeHidden();
    await expect(page.locator('[data-testid="empty-forwards"]')).toBeVisible();
    await expect(page.locator('[data-testid="empty-add-forward"]')).toBeVisible();
  });

  test("AC3/AC4/AC5: inline form create + switch toggle on/off", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedForwardHost("fwd-host-1");

    await page.locator('#activity-bar button[data-activity="forwards"]').click();
    await expect(page.locator('[data-testid="forward-form"]')).toBeVisible();

    await page.locator('[data-testid="fwd-local-port"]').fill("15432");
    await page.locator('[data-testid="fwd-remote-host"]').fill("127.0.0.1");
    await page.locator('[data-testid="fwd-remote-port"]').fill("5432");
    await page.locator('[data-testid="fwd-form-submit"]').click();

    const row = page.locator(".forward-item").first();
    await expect(row).toBeVisible();
    await expect(row.locator(".forward-state")).toHaveAttribute("data-state", "stopped");

    const toggle = row.locator('[data-testid^="forward-toggle-"]');
    await expect(toggle).toBeVisible();
    await expect(toggle).not.toBeChecked();

    await toggle.check();
    await expect(row.locator(".forward-state")).toHaveAttribute("data-state", "running");
    await expect(toggle).toBeChecked();

    await toggle.uncheck();
    await expect(row.locator(".forward-state")).toHaveAttribute("data-state", "stopped");
    await expect(toggle).not.toBeChecked();
  });

  test("AC6: toggle ON failure stays stopped with message", async ({ page }) => {
    const bridge = getTestBridge(page);
    const hostId = await bridge.seedForwardHost("fwd-host-err");
    await bridge.seedForward({
      id: "fwd-err-1",
      hostId,
      name: "Bad bind",
      bindPort: 1,
      destPort: 80,
    });
    await bridge.forwardFailNext("Address already in use");

    await page.locator('#activity-bar button[data-activity="forwards"]').click();
    const toggle = page.locator('[data-testid="forward-toggle-fwd-err-1"]');
    await toggle.check();
    await expect(page.locator("h2", { hasText: "Forward failed" })).toBeVisible();
    await page.locator("#fwd-err-ok").click();
    await expect(page.locator('[data-testid="forward-state-fwd-err-1"]')).toHaveAttribute(
      "data-state",
      "stopped",
    );
    await expect(toggle).not.toBeChecked();
  });
});
