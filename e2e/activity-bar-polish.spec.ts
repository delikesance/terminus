import { test, expect } from "@playwright/test";
import { waitForTestBridge } from "./testBridge";

test.describe("Activity Bar + terminal chrome polish (#29)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
  });

  test("AC1: terminal pane has padding 16px 20px", async ({ page }) => {
    await page.locator("#btn-new-local").click();
    const pane = page.locator(".pane.active");
    await expect(pane).toBeVisible();
    const pad = await pane.evaluate((el) => {
      const s = getComputedStyle(el);
      return {
        top: s.paddingTop,
        right: s.paddingRight,
        bottom: s.paddingBottom,
        left: s.paddingLeft,
      };
    });
    expect(pad).toEqual({
      top: "16px",
      right: "20px",
      bottom: "16px",
      left: "20px",
    });
  });

  test("AC2: Settings and Sync are pinned at bottom of activity bar", async ({ page }) => {
    const bar = page.locator('[data-testid="activity-bar"]');
    const settings = bar.locator("#btn-settings");
    const sync = bar.locator("#status-sync");
    await expect(settings).toBeVisible();
    await expect(sync).toBeVisible();

    const footer = bar.locator('[data-testid="activity-bar-footer"]');
    await expect(footer).toBeVisible();
    await expect(footer.locator("#btn-settings")).toHaveCount(1);
    await expect(footer.locator("#status-sync")).toHaveCount(1);

    const marginTop = await footer.evaluate((el) => {
      // CSSOM resolves flex `margin-top: auto` to a used pixel length; assert the
      // stylesheet still specifies `auto` (AC2 contract).
      for (const sheet of Array.from(document.styleSheets)) {
        let rules: CSSRuleList;
        try {
          rules = sheet.cssRules;
        } catch {
          continue;
        }
        for (const rule of Array.from(rules)) {
          if (!(rule instanceof CSSStyleRule)) continue;
          if (!rule.selectorText.includes("activity-bar-footer")) continue;
          const mt = rule.style.marginTop;
          if (mt) return mt;
        }
      }
      return getComputedStyle(el).marginTop;
    });
    expect(marginTop).toBe("auto");

    const barBox = await bar.boundingBox();
    const footBox = await footer.boundingBox();
    expect(barBox).toBeTruthy();
    expect(footBox).toBeTruthy();
    // Footer sits in the lower half of the rail
    expect(footBox!.y + footBox!.height / 2).toBeGreaterThan(barBox!.y + barBox!.height / 2);
  });

  test("AC3: first activity icon aligns with search input", async ({ page }) => {
    await expect(page.locator("#app")).toHaveClass(/sidebar-open/);
    const icon = page.locator('#activity-bar button[data-activity="hosts"]');
    const search = page.locator("#host-filter");
    await expect(icon).toBeVisible();
    await expect(search).toBeVisible();

    const iconBox = await icon.boundingBox();
    const searchBox = await search.boundingBox();
    expect(iconBox).toBeTruthy();
    expect(searchBox).toBeTruthy();
    const iconCenterY = iconBox!.y + iconBox!.height / 2;
    const searchCenterY = searchBox!.y + searchBox!.height / 2;
    expect(Math.abs(iconCenterY - searchCenterY)).toBeLessThanOrEqual(2);
  });
});
