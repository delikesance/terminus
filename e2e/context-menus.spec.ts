import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * #77 — Scoped custom right-click menus
 * Terminal + text fields + global native-menu suppression; SFTP covered in sftp-browser.spec.ts.
 */

async function ensureActivePane(page: import("@playwright/test").Page) {
  await page.locator(".pane.active").first().waitFor({ state: "visible", timeout: 4000 }).catch(() => {});
  if (!(await page.locator(".pane.active").count())) {
    await page.locator('[data-testid="host-local"]').click();
    await page.locator(".pane.active").first().waitFor({ timeout: 4000 });
  }
}

test.describe("Scoped context menus (#77)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("AC1/AC2: terminal right-click shows Copy/Cut/Paste/Select all/Clear selection", async ({ page }) => {
    await ensureActivePane(page);
    const pane = page.locator(".pane.active").first();
    await pane.click();
    await pane.click({ button: "right", position: { x: 40, y: 40 } });

    const menu = page.locator("#ctx-menu");
    await expect(menu).toBeVisible({ timeout: 5000 });
    await expect(menu).not.toHaveClass(/hidden/);
    await expect(menu.locator("button", { hasText: "Copy" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Cut" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Paste" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Select all" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Clear selection" })).toBeVisible();

    // No selection yet → copy/cut/clear disabled; Paste enabled for live session
    await expect(menu.locator("button", { hasText: "Copy" })).toBeDisabled();
    await expect(menu.locator("button", { hasText: "Cut" })).toBeDisabled();
    await expect(menu.locator("button", { hasText: "Clear selection" })).toBeDisabled();
    await expect(menu.locator("button", { hasText: "Paste" })).toBeEnabled();
    await expect(menu.locator("button", { hasText: "Select all" })).toBeEnabled();
  });

  test("AC1: Select all enables Copy; actions target the right-clicked pane", async ({ page }) => {
    await ensureActivePane(page);
    const pane = page.locator(".pane.active").first();
    await pane.click();
    await pane.click({ button: "right", position: { x: 40, y: 40 } });

    const menu = page.locator("#ctx-menu");
    await expect(menu).toBeVisible({ timeout: 5000 });
    await menu.locator("button", { hasText: "Select all" }).click();
    await expect(menu).toHaveClass(/hidden/);

    const hasSel = await pane.evaluate((el) => {
      const layer = el.querySelector(".term-selection") as HTMLElement | null;
      return Boolean(layer && layer.style.display !== "none");
    });
    expect(hasSel).toBeTruthy();

    await pane.click({ button: "right", position: { x: 40, y: 40 } });
    await expect(menu).toBeVisible({ timeout: 5000 });
    await expect(menu.locator("button", { hasText: "Copy" })).toBeEnabled();
    await expect(menu.locator("button", { hasText: "Clear selection" })).toBeEnabled();
  });

  test("AC3: Ctrl+V paste does not inject a literal key into the PTY path", async ({ page }) => {
    await ensureActivePane(page);
    const pane = page.locator(".pane.active").first();
    await pane.click();

    // Grant clipboard + seed text; paste handler should consume Ctrl+V.
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.evaluate(async () => {
      await navigator.clipboard.writeText("paste-e2e-marker");
    });

    const before = await page.evaluate(() => (window as any).__pasteIntoPaneCalls ?? 0);
    await page.keyboard.press("Control+v");
    await page.waitForTimeout(150);

    const after = await page.evaluate(() => ({
      calls: (window as any).__pasteIntoPaneCalls ?? 0,
      last: (window as any).__pasteIntoPaneLast ?? null,
    }));
    // Production exposes E2E counters when VITE_E2E=1.
    expect(after.calls).toBeGreaterThan(before);
    expect(after.last).toBe("paste-e2e-marker");
  });

  test("AC3b: Ctrl+Shift+V pastes via terminal.paste keybinding", async ({ page }) => {
    await ensureActivePane(page);
    const pane = page.locator(".pane.active").first();
    await pane.click();

    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.evaluate(async () => {
      await navigator.clipboard.writeText("paste-shift-e2e-marker");
    });

    const before = await page.evaluate(() => (window as any).__pasteIntoPaneCalls ?? 0);
    await page.keyboard.press("Control+Shift+v");
    await page.waitForTimeout(150);

    const after = await page.evaluate(() => ({
      calls: (window as any).__pasteIntoPaneCalls ?? 0,
      last: (window as any).__pasteIntoPaneLast ?? null,
    }));
    expect(after.calls).toBeGreaterThan(before);
    expect(after.last).toBe("paste-shift-e2e-marker");
  });

  test("AC5: right-click anywhere prevents the native context menu", async ({ page }) => {
    await ensureActivePane(page);

    // Activity bar / chrome: no custom menu required, but default must be cancelled.
    const prevented = await page.locator("#activity-bar").evaluate((el) => {
      const ev = new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: 8,
        clientY: 8,
      });
      el.dispatchEvent(ev);
      return ev.defaultPrevented;
    });
    expect(prevented).toBe(true);

    const panePrevented = await page.locator(".pane.active").first().evaluate((el) => {
      const ev = new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: 40,
        clientY: 40,
      });
      el.dispatchEvent(ev);
      return ev.defaultPrevented;
    });
    expect(panePrevented).toBe(true);
  });

  test("AC6: editable text field right-click shows Cut/Copy/Paste/Select all", async ({ page }) => {
    const input = page.locator("#host-filter");
    await expect(input).toBeVisible();
    await input.fill("hello");
    await input.evaluate((el: HTMLInputElement) => {
      el.focus();
      el.setSelectionRange(0, 5);
    });
    await input.click({ button: "right" });

    const menu = page.locator("#ctx-menu");
    await expect(menu).toBeVisible({ timeout: 5000 });
    await expect(menu.locator("button", { hasText: "Cut" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Copy" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Paste" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Select all" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Cut" })).toBeEnabled();
    await expect(menu.locator("button", { hasText: "Copy" })).toBeEnabled();
  });

  test("AC4 smoke: SFTP row still opens custom file menu (not native)", async ({ page }) => {
    const bridge = getTestBridge(page);
    await bridge.seedSftpHost("sftp-ctx-host");
    await page.locator('#activity-bar button[data-activity="sftp"]').click();
    await expect(page.locator('[data-testid="sftp-toolbar"]')).toBeVisible({ timeout: 5000 });
    const notes = page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" });
    await expect(notes).toBeVisible({ timeout: 5000 });
    await notes.click({ button: "right" });
    const menu = page.locator("#ctx-menu");
    await expect(menu).toBeVisible({ timeout: 5000 });
    await expect(menu.locator("button", { hasText: "Copy" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Paste" })).toBeVisible();
    await expect(menu.locator("button", { hasText: "Delete" })).toBeVisible();
  });
});
