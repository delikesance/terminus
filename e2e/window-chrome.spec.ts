import { test, expect } from "@playwright/test";
import { waitForTestBridge } from "./testBridge";

/**
 * #99 — OS-aware window chrome
 * Linux / Windows CI → symbolic caption buttons on the right.
 */
test.describe("OS-aware window chrome (#99)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
  });

  test("AC2: Linux/Windows chrome uses right-side caption symbols", async ({ page }) => {
    await expect(page.locator("html")).toHaveAttribute("data-os-chrome", "win");
    const metrics = await page.evaluate(() => {
      const bar = document.getElementById("titlebar")!;
      const controls = document.getElementById("window-controls")!;
      const close = document.getElementById("win-close")!;
      const min = document.getElementById("win-min")!;
      const max = document.getElementById("win-max")!;
      const br = bar.getBoundingClientRect();
      const cr = controls.getBoundingClientRect();
      const closeR = close.getBoundingClientRect();
      const minR = min.getBoundingClientRect();
      const maxR = max.getBoundingClientRect();
      const cs = getComputedStyle(close);
      return {
        controlsNearRight: cr.right > br.width * 0.7,
        orderMinMaxClose: minR.left < maxR.left && maxR.left < closeR.left,
        notCircle: parseFloat(cs.borderRadius) < 4 || cs.borderRadius === "0px",
        wideEnough: closeR.width >= 40,
      };
    });
    expect(metrics.controlsNearRight).toBeTruthy();
    expect(metrics.orderMinMaxClose).toBeTruthy();
    expect(metrics.notCircle).toBeTruthy();
    expect(metrics.wideEnough).toBeTruthy();
  });
});
