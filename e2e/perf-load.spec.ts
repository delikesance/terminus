import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";
import fs from "node:fs";
import path from "node:path";

/**
 * #45 — Performance crash / load tests
 * Stress multi-pane canvas paint (CPU+GPU upload), SFTP bulk filter UI, assert no crash
 * and bounded timings / heap growth when Chrome exposes performance.memory.
 */

const ARTIFACTS = path.join(process.cwd(), "artifacts");

test.describe("#45: perf crash/load", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
  });

  test("multi-pane canvas stress stays alive with bounded paint time", async ({ page }) => {
    const bridge = getTestBridge(page);
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(String(err)));
    page.on("crash", () => errors.push("page crashed"));

    // Boot already opens one local pane; open more until we have several canvases.
    for (let i = 0; i < 5; i++) {
      await page.locator("#btn-new-local").click();
      await page.waitForTimeout(80);
    }
    await expect.poll(async () => (await bridge.perfSnapshot()).canvases).toBeGreaterThanOrEqual(4);

    const before = await bridge.perfSnapshot();
    const stress = await bridge.stressCanvases({ iterations: 24, width: 120, height: 60 });
    const after = await bridge.perfSnapshot();
    const fps = await bridge.measureFps(1000);

    expect(errors, `page errors: ${errors.join(" | ")}`).toEqual([]);
    expect(stress.canvases).toBeGreaterThanOrEqual(4);
    expect(stress.paints).toBeGreaterThan(0);
    expect(stress.ms).toBeLessThan(20_000);
    expect(fps.frames).toBeGreaterThanOrEqual(20);
    expect(fps.fps).toBeGreaterThanOrEqual(55);
    expect(page.isClosed()).toBe(false);

    if (before.heap && after.heap) {
      const growth = after.heap.usedMb - before.heap.usedMb;
      expect(growth, `heap grew ${growth}MB`).toBeLessThan(50);
    }

    fs.mkdirSync(ARTIFACTS, { recursive: true });
    fs.writeFileSync(
      path.join(ARTIFACTS, "perf-e2e-canvas.json"),
      JSON.stringify({ before, stress, after, fps, webglOk: after.webglOk }, null, 2),
    );
  });

  test("SFTP bulk listing filter + split thrash does not crash", async ({ page }) => {
    const bridge = getTestBridge(page);
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(String(err)));
    page.on("crash", () => errors.push("page crashed"));

    const hostId = await bridge.seedSftpHost("perf-sftp-host");
    const bulk = await bridge.seedSftpBulk(hostId, 800);
    expect(bulk.count).toBe(800);

    await page.locator('#activity-bar button[data-activity="sftp"]').click();
    await expect(page.locator('[data-testid="sftp-toolbar"]')).toBeVisible();
    // Virtual list: only ~viewport rows should be mounted for bulk listings (#50).
    const rowCount = await page.locator('[data-testid="sftp-row"]').count();
    expect(rowCount).toBeGreaterThan(0);
    expect(rowCount).toBeLessThan(120);

    const before = await bridge.perfSnapshot();
    const t0 = Date.now();

    // Filter thrash (CPU + DOM re-render)
    for (const q of ["bulk-00", "bulk-12", "bulk-99", "zzz", ""]) {
      await page.locator("#host-filter").fill(q);
      await page.waitForTimeout(40);
    }

    // Hidden toggle thrash
    for (let i = 0; i < 6; i++) {
      await page.locator('[data-testid="sftp-toggle-hidden"]').click();
      await page.waitForTimeout(30);
    }

    // Split open/close thrash
    for (let i = 0; i < 4; i++) {
      await page.locator('[data-testid="sftp-split-btn"]').click();
      await expect(page.locator('[data-testid="sftp-pane-b"]')).toBeVisible();
      await page.locator('[data-testid="sftp-close-split-btn"]').click();
      await expect(page.locator('[data-testid="sftp-pane-b"]')).toHaveCount(0);
    }

    const elapsed = Date.now() - t0;
    const after = await bridge.perfSnapshot();

    expect(errors, `page errors: ${errors.join(" | ")}`).toEqual([]);
    expect(elapsed).toBeLessThan(30_000);
    await expect(page.locator('[data-testid="sftp-toolbar"]')).toBeVisible();
    expect(page.isClosed()).toBe(false);

    if (before.heap && after.heap) {
      expect(after.heap.usedMb - before.heap.usedMb).toBeLessThan(50);
    }

    fs.mkdirSync(ARTIFACTS, { recursive: true });
    fs.writeFileSync(
      path.join(ARTIFACTS, "perf-e2e-sftp.json"),
      JSON.stringify({ bulk, elapsedMs: elapsed, before, after }, null, 2),
    );
  });
});
