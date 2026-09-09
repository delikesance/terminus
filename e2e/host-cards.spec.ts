import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

test.describe("Host Cards UX (#31)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
  });

  test("AC2: local dot is compact without halo box-shadow", async ({ page }) => {
    const dot = page.locator('[data-testid="host-local"] [data-testid="connection-dot"]');
    await expect(dot).toBeVisible();
    await expect(dot).toHaveAttribute("data-state", "local");
    const style = await dot.evaluate((el) => {
      const s = getComputedStyle(el);
      return {
        width: s.width,
        height: s.height,
        boxShadow: s.boxShadow,
      };
    });
    const w = parseFloat(style.width);
    expect(w).toBeGreaterThanOrEqual(6);
    expect(w).toBeLessThanOrEqual(8);
    expect(style.boxShadow === "none" || style.boxShadow === "").toBeTruthy();
  });

  test("AC3: + and … appear on hover; … opens context menu", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedUngroupedHosts(1);
    const card = page.locator(".host-card[data-host]").first();
    await expect(card).toBeVisible();

    const plus = card.locator('[data-testid="host-action-new"]');
    const more = card.locator('[data-testid="host-action-more"]');
    await expect(plus).toBeAttached();
    await expect(more).toBeAttached();

    const hiddenAtRest = await plus.evaluate((el) => getComputedStyle(el).opacity);
    expect(Number(hiddenAtRest)).toBeLessThan(0.1);

    await card.hover();
    await expect(plus).toBeVisible();
    await expect(more).toBeVisible();

    await more.click();
    await expect(page.locator("#ctx-menu")).not.toHaveClass(/hidden/);
    await expect(page.locator("#ctx-menu")).toContainText(/Edit|SFTP|Delete/);
  });

  test("AC4: active host has left accent border", async ({ page }) => {
    await page.locator("#btn-new-local").click();
    const local = page.locator('[data-testid="host-local"]');
    await expect(local).toHaveClass(/active-host/);
    const border = await local.evaluate((el) => {
      const s = getComputedStyle(el);
      return {
        width: s.borderLeftWidth,
        style: s.borderLeftStyle,
      };
    });
    expect(parseFloat(border.width)).toBeGreaterThanOrEqual(2);
    expect(border.style).not.toBe("none");
  });
});
