import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

test.describe("Polish finish (#33)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
  });

  test("AC1/AC2: status dots share X; hover + does not shift dots", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
    await bridge.seedUngroupedHosts(2);

    const cards = page.locator(".host-card[data-host]");
    await expect(cards).toHaveCount(2);

    const xsAtRest = await cards.evaluateAll((els) =>
      els.map((el) => {
        const dot = el.querySelector('[data-testid="connection-dot"]');
        const r = dot!.getBoundingClientRect();
        return r.x + r.width / 2;
      }),
    );
    expect(Math.max(...xsAtRest) - Math.min(...xsAtRest)).toBeLessThanOrEqual(1);

    const first = cards.first();
    const plus = first.locator('[data-testid="host-action-new"]');
    const opacityRest = await plus.evaluate((el) => Number(getComputedStyle(el).opacity));
    expect(opacityRest).toBeLessThan(0.1);

    const xBefore = await first.locator('[data-testid="connection-dot"]').evaluate((el) => {
      const r = el.getBoundingClientRect();
      return r.x + r.width / 2;
    });
    await first.hover();
    await expect(plus).toBeVisible();
    const xAfter = await first.locator('[data-testid="connection-dot"]').evaluate((el) => {
      const r = el.getBoundingClientRect();
      return r.x + r.width / 2;
    });
    expect(Math.abs(xAfter - xBefore)).toBeLessThanOrEqual(1);
  });

  test("AC3: terminal pane padding remains 16px 20px", async ({ page }) => {
    await page.locator("#btn-new-local").click();
    const pane = page.locator(".pane.active");
    await expect(pane).toBeVisible();
    const pad = await pane.evaluate((el) => {
      const s = getComputedStyle(el);
      return { top: s.paddingTop, right: s.paddingRight, bottom: s.paddingBottom, left: s.paddingLeft };
    });
    expect(pad).toEqual({ top: "16px", right: "20px", bottom: "16px", left: "20px" });
  });

  test("AC4: update available → badge on Settings, no floating toast; click opens modal", async ({
    page,
  }) => {
    const bridge = getTestBridge(page);
    await bridge.setUpdateAvailable("9.9.9");

    await expect(page.locator("#update-toast")).toBeHidden();
    const settings = page.locator("#btn-settings");
    await expect(settings).toHaveAttribute("data-update-available", "true");
    await expect(settings.locator('[data-testid="settings-update-badge"]')).toBeVisible();

    await settings.click();
    await expect(page.locator("#modal")).not.toHaveClass(/hidden/);
    await expect(page.locator("#modal-sheet")).toContainText(/9\.9\.9|Restart|Later|update/i);
  });
});
