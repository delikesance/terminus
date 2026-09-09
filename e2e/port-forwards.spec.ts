import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

test.describe("Port Forwarding panel UX (#35)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("AC1: Activity Bar opens forwards; form hidden; add btn shown when hosts exist", async ({
    page,
  }) => {
    const bridge = getTestBridge(page);
    await bridge.seedForwardHost("fwd-host-1");
    await page.locator('#activity-bar button[data-activity="forwards"]').click();
    await expect(page.locator("#panel-forwards")).toBeVisible();
    await expect(page.locator('[data-testid="forward-form"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="forward-add-btn"]')).toBeVisible();
    await expect(page.locator("#btn-new-host")).toBeHidden();
    await expect(page.locator("#btn-new-group")).toBeHidden();
  });

  test("AC2/AC3/AC4: + opens form; create; mapping subtitle; toggle", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedForwardHost("fwd-host-1");

    await page.locator('#activity-bar button[data-activity="forwards"]').click();
    await page.locator('[data-testid="forward-add-btn"]').click();
    await expect(page.locator('[data-testid="forward-form"]')).toBeVisible();

    await page.locator('[data-testid="fwd-local-port"]').fill("15432");
    await expect(page.locator('[data-testid="fwd-remote-host"]')).toHaveValue("127.0.0.1");
    await page.locator('[data-testid="fwd-remote-port"]').fill("5432");
    await page.locator('[data-testid="fwd-form-submit"]').click();

    const row = page.locator(".forward-item").first();
    await expect(row).toBeVisible();
    await expect(row.locator(".host-subtitle, small").first()).toContainText(
      /localhost:15432\s*→\s*127\.0\.0\.1:5432\s*via/i,
    );
    await expect(row.locator(".forward-state")).toHaveAttribute("data-state", "stopped");

    const toggle = row.locator('[data-testid^="forward-toggle-"]');
    await toggle.check();
    await expect(row.locator(".forward-state")).toHaveAttribute("data-state", "running");
    await toggle.uncheck();
    await expect(row.locator(".forward-state")).toHaveAttribute("data-state", "stopped");
  });

  test("AC5: hosts activity shows New host / New group again", async ({ page }) => {
    await page.locator('#activity-bar button[data-activity="forwards"]').click();
    await expect(page.locator("#btn-new-host")).toBeHidden();
    await page.locator('#activity-bar button[data-activity="hosts"]').click();
    await expect(page.locator("#btn-new-host")).toBeVisible();
    await expect(page.locator("#btn-new-group")).toBeVisible();
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
