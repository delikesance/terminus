import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";

/**
 * C10 — terminal copy (from the canvas renderer)
 * AC: drag-select a region + Ctrl+C → session_selection_text invoked → clipboard text captured.
 * Note: the terminal is a <canvas> rendered from a Rust VT bitmap, so text selection is a
 * custom rectangle selection, not native DOM selection.
 */
test("C10: selecting terminal text + Ctrl+C copies it", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  await waitForTestBridge(page);
  const bridge = getTestBridge(page);
  await bridge.clearAllHosts();

  // The app auto-opens a local shell on boot; if none is active yet, click the local card.
  await page.locator(".pane.active").first().waitFor({ state: "visible", timeout: 4000 }).catch(() => {});
  if (!(await page.locator(".pane.active").count())) {
    await page.locator('[data-testid="host-local"]').click();
    await page.locator(".pane.active").first().waitFor({ timeout: 4000 });
  }

  const pane = page.locator(".pane.active").first();
  await pane.click();
  const canvas = pane.locator("canvas.term-canvas");
  await expect(canvas).toBeVisible();
  const box = await canvas.boundingBox();
  expect(box).toBeTruthy();
  const cx = box!.x, cy = box!.y;

  // Drag a rectangle inside the canvas (coords relative to canvas, not padded pane).
  await page.mouse.move(cx + 40, cy + 30);
  await page.mouse.down();
  await page.mouse.move(cx + 200, cy + 120, { steps: 8 });
  await page.mouse.up();
  await page.waitForTimeout(120);

  // Selection overlay must share the canvas origin (not the padded .pane box).
  const aligned = await page.evaluate(() => {
    const p = document.querySelector(".pane.active") as HTMLElement | null;
    const c = p?.querySelector("canvas.term-canvas") as HTMLElement | null;
    const s = p?.querySelector(".term-selection") as HTMLElement | null;
    const v = p?.querySelector(".term-viewport") as HTMLElement | null;
    if (!p || !c || !s || !v) return { ok: false, reason: "missing nodes" };
    if (s.style.display === "none") return { ok: false, reason: "no selection" };
    const cr = c.getBoundingClientRect();
    const sr = s.getBoundingClientRect();
    // Overlay top/left should be on/after the canvas top/left (never above by a full padding).
    const dy = sr.top - cr.top;
    const dx = sr.left - cr.left;
    return { ok: dy >= -1 && dx >= -1 && dy < 40, dy, dx, reason: "ok" };
  });
  expect(aligned.ok, JSON.stringify(aligned)).toBeTruthy();

  await page.keyboard.press("Control+c");
  await page.waitForTimeout(150);

  const vars = await page.evaluate(() => ({
    lastSelCmd: (window as any).__lastSelCmd ?? null,
    copyLast: (window as any).__copyLast ?? null,
  }));
  expect(vars.lastSelCmd).toMatch(/^sel-\d+-\d+-\d+-\d+$/);
  // copyTerminalSelection stores the exact extracted text in __copyLast (E2E hook).
  expect(vars.copyLast).toBe(vars.lastSelCmd);
});
