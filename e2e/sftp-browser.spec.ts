import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";

/**
 * C9 — SFTP browser (full-FS, Termius-style)
 * P0b — listing in #workspace (.sftp-view) ≥60% width
 * AC: full-FS browse (absolute paths + .. allowed, no sandbox) · confirm before delete · typed I/O errors · navigate/up/open/rename
 */

async function openFilesPanel(page: import("@playwright/test").Page) {
  await page.locator('#activity-bar button[data-activity="sftp"]').click();
  await expect(page.locator('[data-testid="sftp-toolbar"]')).toBeVisible();
  await expect(page.locator("#workspace .sftp-view.active")).toBeVisible();
}

test.describe("C9: SFTP browser v1", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await waitForTestBridge(page);
    const bridge = getTestBridge(page);
    await bridge.clearAllHosts();
    await bridge.seedSftpHost("sftp-e2e-host");
  });

  test("lists files in workspace with toolbar + table (name · size · mtime · ⋯)", async ({ page }) => {
    await openFilesPanel(page);
    const table = page.locator("#workspace [data-testid='sftp-table']");
    await expect(table).toBeVisible();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab");
    await expect(page.locator('[data-testid="sftp-row"]')).toHaveCount(3);
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "remote-only.txt" })).toBeVisible();
    await expect(page.locator('[data-testid="sftp-more"]').first()).toBeVisible();
    await expect(page.locator(".sftp-size").first()).toBeVisible();
    await expect(page.locator(".sftp-mtime").first()).toBeVisible();
    await expect(page.locator('[data-testid="sftp-side"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-side-host"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-side-status"]')).toHaveAttribute("data-state", "connected");
    await expect(page.locator('[data-testid="sftp-side-status"]')).toContainText("Connected");
  });

  test("P0b: listing lives in #workspace at ≥60% of stage width", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator("#workspace [data-testid='sftp-table']")).toBeVisible();
    const metrics = await page.evaluate(() => {
      const stage = document.getElementById("stage")!;
      const workspace = document.getElementById("workspace")!;
      const view = workspace.querySelector(".sftp-view.active") as HTMLElement;
      const table = workspace.querySelector('[data-testid="sftp-table"]');
      return {
        stageW: stage.clientWidth,
        viewW: view?.clientWidth ?? 0,
        tableInWorkspace: Boolean(table && workspace.contains(table)),
        tableInSidebar: Boolean(document.getElementById("panel-sftp")?.querySelector('[data-testid="sftp-table"]')),
      };
    });
    expect(metrics.tableInWorkspace).toBe(true);
    expect(metrics.tableInSidebar).toBe(false);
    expect(metrics.viewW / metrics.stageW).toBeGreaterThanOrEqual(0.6);
  });

  test("leaving Files restores terminal panes", async ({ page }) => {
    await page.locator("#btn-new-local").click();
    await expect(page.locator(".pane.active")).toBeVisible({ timeout: 10000 });
    await openFilesPanel(page);
    await expect(page.locator("#workspace.sftp-mode")).toHaveCount(1);
    await page.locator('#activity-bar button[data-activity="hosts"]').click();
    await expect(page.locator(".sftp-view.active")).toHaveCount(0);
    await expect(page.locator("#workspace.sftp-mode")).toHaveCount(0);
    await expect(page.locator(".pane.active")).toBeVisible();
  });

  test("navigate into dir, Up returns, refresh keeps path", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab/docs");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible();

    await page.locator('[data-testid="sftp-up"]').click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab");

    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await page.locator('[data-testid="sftp-refresh"]').click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab/docs");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible();
  });

  test("clicking a file opens it with the default app", async ({ page }) => {
    const bridge = getTestBridge(page);
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" }).click();
    await expect.poll(async () => (await bridge.lastSftpOpen())?.name).toBe("notes.txt");
    await expect(page.locator('[data-testid="sftp-error"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab");
  });

  test("editable path bar navigates; empty folder shows .sftp-empty", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-path"]').fill("/home/lab/docs");
    await page.locator('[data-testid="sftp-path"]').press("Enter");
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab/docs");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible();

    // Delete readme → empty
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "readme.txt" })
      .locator('[data-testid="sftp-more"]')
      .click();
    await page.locator("#ctx-menu button.danger").click();
    await expect(page.locator('[data-testid="sftp-del-confirm"]')).toBeVisible();
    await page.locator('[data-testid="sftp-del-confirm"]').click();
    await expect(page.locator('[data-testid="sftp-empty"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-empty"]')).toContainText("empty");
  });

  test("full-FS: absolute path + .. navigate (sandbox removed)", async ({ page }) => {
    await openFilesPanel(page);
    // absolute path above home is now allowed
    await page.locator('[data-testid="sftp-path"]').fill("/home");
    await page.locator('[data-testid="sftp-path"]').press("Enter");
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "lab" })).toBeVisible();
    // drill back into home
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "lab" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible();
    // `..` steps up one dir
    await page.locator('[data-testid="sftp-path"]').fill("..");
    await page.locator('[data-testid="sftp-path"]').press("Enter");
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home");
  });

  test("AC: typed network/io errors surface in .sftp-error", async ({ page }) => {
    const bridge = getTestBridge(page);
    await openFilesPanel(page);
    await bridge.sftpForceError("SftpNetwork", "connection reset by peer");
    await page.locator('[data-testid="sftp-refresh"]').click();
    const err = page.locator('[data-testid="sftp-error"]');
    await expect(err).toBeVisible();
    await expect(err).toHaveAttribute("data-kind", "SftpNetwork");
    await expect(err).toContainText(/connection reset/i);
  });

  test("AC: delete requires confirm sheet with danger button", async ({ page }) => {
    await openFilesPanel(page);
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "notes.txt" })
      .locator('[data-testid="sftp-more"]')
      .click();
    await page.locator("#ctx-menu button.danger").click();
    await expect(page.locator("#modal")).not.toHaveClass(/hidden/);
    const danger = page.locator('[data-testid="sftp-del-confirm"]');
    await expect(danger).toBeVisible();
    await expect(danger).toHaveClass(/danger/);
    // Cancel keeps file
    await page.locator('[data-testid="sftp-del-cancel"]').click();
    await expect(page.locator("#modal")).toHaveClass(/hidden/);
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible();
    // Confirm deletes
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "notes.txt" })
      .locator('[data-testid="sftp-more"]')
      .click();
    await page.locator("#ctx-menu button.danger").click();
    await danger.click();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toHaveCount(0);
  });

  test("rename via ⋯ menu updates listing", async ({ page }) => {
    await openFilesPanel(page);
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "notes.txt" })
      .locator('[data-testid="sftp-more"]')
      .click();
    await page.locator("#ctx-menu button", { hasText: "Rename" }).click();
    await expect(page.locator('[data-testid="sftp-rename-input"]')).toBeVisible();
    await page.locator('[data-testid="sftp-rename-input"]').fill("renamed.txt");
    await page.locator('[data-testid="sftp-rename-ok"]').click();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "renamed.txt" })).toBeVisible();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toHaveCount(0);
  });

  test("upload via native file picker creates row", async ({ page }) => {
    await openFilesPanel(page);
    const tmp = path.join(os.tmpdir(), `terminus-c9-upload-${Date.now()}.txt`);
    fs.writeFileSync(tmp, "uploaded-by-e2e");
    const [chooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.locator('[data-testid="sftp-upload"]').click(),
    ]);
    await chooser.setFiles(tmp);
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: path.basename(tmp) })).toBeVisible({
      timeout: 10000,
    });
    fs.unlinkSync(tmp);
  });

  test("download file triggers native save / download", async ({ page }) => {
    await openFilesPanel(page);
    const downloadPromise = page.waitForEvent("download", { timeout: 8000 }).catch(() => null);
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "notes.txt" })
      .locator('[data-testid="sftp-more"]')
      .click();
    await page.locator("#ctx-menu button", { hasText: "Download" }).click();
    const download = await downloadPromise;
    // Chromium may use <a download> → download event; File System Access API may not.
    if (download) {
      expect(download.suggestedFilename()).toBe("notes.txt");
    } else {
      // Fallback: ensure menu action did not throw into sftp-error
      await expect(page.locator('[data-testid="sftp-error"]')).toHaveCount(0);
    }
  });

  test("C9b: dual-pane shell renders local + remote + transfer arrows", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator('[data-testid="sftp-pane-local"]')).toBeVisible();
    await expect(page.locator('[data-testid="local-toolbar"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-pane-remote"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-arrows"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-tx-up"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-tx-down"]')).toBeVisible();
  });

  test("C9b: local pane lists home with selection cells + stays after nav", async ({ page }) => {
    await openFilesPanel(page);
    const localTable = page.locator('[data-testid="local-table"]');
    await expect(localTable).toBeVisible();
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "local-upload.txt" })).toBeVisible();
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "docs" })).toBeVisible();
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "subdir" })).toBeVisible();
    await expect(page.locator('[data-testid="local-select"]').first()).toBeVisible();
    // navigate a remote dir → local pane persists
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab/docs");
    await expect(page.locator('[data-testid="local-table"]')).toBeVisible();
  });

  test("C9b: upload selected local file → appears in remote pane + transfers", async ({ page }) => {
    const bridge = getTestBridge(page);
    await openFilesPanel(page);
    await page
      .locator('[data-testid="local-row"]')
      .filter({ hasText: "local-upload.txt" })
      .locator('[data-testid="local-select"]')
      .click();
    await expect(page.locator('[data-testid="sftp-tx-up"]')).toBeEnabled();
    await page.locator('[data-testid="sftp-tx-up"]').click();
    const remoteRow = page.locator('[data-testid="sftp-row"]').filter({ hasText: "local-upload.txt" });
    await expect(remoteRow).toBeVisible({ timeout: 10000 });
    await expect.poll(async () => (await bridge.transferOps()).uploads).toBeGreaterThanOrEqual(1);
    await expect(page.locator('[data-testid="sftp-error"]')).toHaveCount(0);
  });

  test("C9b: download selected remote file → appears in local pane + transfers", async ({ page }) => {
    const bridge = getTestBridge(page);
    await openFilesPanel(page);
    await page.locator("[data-testid='sftp-view']").waitFor();
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "remote-only.txt" })
      .locator('[data-testid="sftp-select"]')
      .click();
    await expect(page.locator('[data-testid="sftp-tx-down"]')).toBeEnabled();
    await page.locator('[data-testid="sftp-tx-down"]').click();
    await expect.poll(async () => (await bridge.transferOps()).downloads).toBeGreaterThanOrEqual(1);
    const localRow = page.locator('[data-testid="local-row"]').filter({ hasText: "remote-only.txt" });
    await expect(localRow).toBeVisible({ timeout: 10000 });
    await expect(page.locator('[data-testid="sftp-error"]')).toHaveCount(0);
  });

  test("C9b: upload a whole folder recursively (subdir → remote)", async ({ page }) => {
    await openFilesPanel(page);
    await page
      .locator('[data-testid="local-row"]')
      .filter({ hasText: "subdir" })
      .locator('[data-testid="local-select"]')
      .click();
    await page.locator('[data-testid="sftp-tx-up"]').click();
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "subdir" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab/subdir");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "deep.txt" })).toBeVisible({ timeout: 10000 });
  });
});
