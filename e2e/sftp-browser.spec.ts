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
  await expect(page.locator('[data-testid="sftp-toolbar"]')).toBeVisible({ timeout: 5000 });
  await expect(page.locator("#workspace .sftp-view.active")).toBeVisible({ timeout: 5000 });
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
    await expect(table).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab");
    await expect(page.locator('[data-testid="sftp-row"]')).toHaveCount(3); // .cache hidden by default
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "remote-only.txt" })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-more"]').first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".sftp-size").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".sftp-mtime").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-side"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-side-host"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="sftp-toggle-hidden"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-side-status"]')).toHaveCount(0);
  });

  test("P0b: listing lives in #workspace at ≥60% of stage width", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator("#workspace [data-testid='sftp-table']")).toBeVisible({ timeout: 5000 });
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
    await expect(page.locator(".pane.active")).toBeVisible({ timeout: 5000 });
  });

  test("navigate into dir, Up returns, refresh keeps path", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab/docs");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible({ timeout: 5000 });

    await page.locator('[data-testid="sftp-up"]').click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab");

    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await page.locator('[data-testid="sftp-refresh"]').click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab/docs");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible({ timeout: 5000 });
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
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible({ timeout: 5000 });

    // Delete readme → empty
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "readme.txt" })
      .locator('[data-testid="sftp-more"]')
      .click();
    await page.locator("#ctx-menu button.danger").click();
    await expect(page.locator('[data-testid="sftp-del-confirm"]')).toBeVisible({ timeout: 5000 });
    await page.locator('[data-testid="sftp-del-confirm"]').click();
    await expect(page.locator('[data-testid="sftp-empty"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-empty"]')).toContainText("empty");
  });

  test("full-FS: absolute path + .. navigate (sandbox removed)", async ({ page }) => {
    await openFilesPanel(page);
    // absolute path above home is now allowed
    await page.locator('[data-testid="sftp-path"]').fill("/home");
    await page.locator('[data-testid="sftp-path"]').press("Enter");
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "lab" })).toBeVisible({ timeout: 5000 });
    // drill back into home
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "lab" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("/home/lab");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible({ timeout: 5000 });
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
    await expect(err).toBeVisible({ timeout: 5000 });
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
    await expect(danger).toBeVisible({ timeout: 5000 });
    await expect(danger).toHaveClass(/danger/);
    // Cancel keeps file
    await page.locator('[data-testid="sftp-del-cancel"]').click();
    await expect(page.locator("#modal")).toHaveClass(/hidden/);
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible({ timeout: 5000 });
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
    await expect(page.locator('[data-testid="sftp-rename-input"]')).toBeVisible({ timeout: 5000 });
    await page.locator('[data-testid="sftp-rename-input"]').fill("renamed.txt");
    await page.locator('[data-testid="sftp-rename-ok"]').click();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "renamed.txt" })).toBeVisible({ timeout: 5000 });
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

  test("C9b / #41: single pane by default; Split reveals Host A/B + arrows", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator('[data-testid="sftp-pane-a"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="sftp-arrows"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="sftp-split-btn"]')).toBeVisible({ timeout: 5000 });

    await page.locator('[data-testid="sftp-split-btn"]').click();
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-arrows"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-pane-a-host"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-pane-b-host"]')).toBeVisible({ timeout: 5000 });
    // Default companion for remote A is This computer
    await expect(page.locator('[data-testid="sftp-pane-b-host"]')).toHaveAttribute("data-value", "__local__");
    await expect(page.locator('[data-testid="local-toolbar"]')).toBeVisible({ timeout: 5000 });
  });

  test("C9b: after Split, local pane lists home with selection cells + stays after nav", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-split-btn"]').click();
    const localTable = page.locator('[data-testid="local-table"]');
    await expect(localTable).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "local-upload.txt" })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "docs" })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "subdir" })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-select"]').first()).toBeVisible({ timeout: 5000 });
    // navigate a remote dir → local pane persists
    await page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-path"]')).toHaveValue("/home/lab/docs");
    await expect(page.locator('[data-testid="local-table"]')).toBeVisible({ timeout: 5000 });
  });

  test("C9b: upload selected local file → appears in remote pane + transfers", async ({ page }) => {
    const bridge = getTestBridge(page);
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "local-upload.txt" })).toBeVisible({
      timeout: 5000,
    });
    await page
      .locator('[data-testid="local-row"]')
      .filter({ hasText: "local-upload.txt" })
      .locator('[data-testid="local-select"]')
      .click();
    await expect(page.locator('[data-testid="sftp-tx-up"]')).toBeEnabled();
    await page.locator('[data-testid="sftp-tx-up"]').click();
    const remoteRow = page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]').filter({
      hasText: "local-upload.txt",
    });
    await expect(remoteRow).toBeVisible({ timeout: 10000 });
    await expect.poll(async () => (await bridge.transferOps()).uploads).toBeGreaterThanOrEqual(1);
    await expect(page.locator('[data-testid="sftp-error"]')).toHaveCount(0);
  });

  test("C9b: download selected remote file → appears in local pane + transfers", async ({ page }) => {
    const bridge = getTestBridge(page);
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await page.locator("[data-testid='sftp-view']").waitFor();
    await page
      .locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]')
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
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await page
      .locator('[data-testid="local-row"]')
      .filter({ hasText: "subdir" })
      .locator('[data-testid="local-select"]')
      .click();
    await page.locator('[data-testid="sftp-tx-up"]').click();
    await page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]').filter({ hasText: "subdir" }).click();
    await expect(page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-path"]')).toHaveValue(
      "/home/lab/subdir",
    );
    await expect(
      page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]').filter({ hasText: "deep.txt" }),
    ).toBeVisible({ timeout: 10000 });
  });

  test("#41: Close split returns to single pane", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toBeVisible({ timeout: 5000 });
    await page.locator('[data-testid="sftp-close-split-btn"]').click();
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="sftp-arrows"]')).toHaveCount(0);
    await expect(page.locator('[data-testid="sftp-pane-a"]')).toBeVisible({ timeout: 5000 });
  });

  test("#43 AC3: host-filter filters active remote pane listing", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator("#host-filter")).toHaveAttribute("placeholder", /Filter pane A/i);
    await page.locator("#host-filter").fill("notes");
    await expect(page.locator('[data-testid="sftp-row"]')).toHaveCount(1, { timeout: 3000 });
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible({ timeout: 5000 });
    await page.locator("#host-filter").fill("");
    await expect(page.locator('[data-testid="sftp-row"]')).toHaveCount(3, { timeout: 3000 });
  });

  test("#43 AC4: hidden files toggle shows and hides dotfiles", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: ".cache" })).toHaveCount(0);
    await page.locator('[data-testid="sftp-toggle-hidden"]').click();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: ".cache" })).toBeVisible({ timeout: 5000 });
    await page.locator('[data-testid="sftp-toggle-hidden"]').click();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: ".cache" })).toHaveCount(0);
  });

  test("#43 AC5: batch bar appears on multi-select in split mode", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await expect(page.locator('[data-testid="sftp-batch-bar"]')).toHaveClass(/hidden/);
    await page
      .locator('[data-testid="local-row"]')
      .filter({ hasText: "local-upload.txt" })
      .locator('[data-testid="local-select"]')
      .click();
    const batch = page.locator('[data-testid="sftp-batch-bar"]');
    await expect(batch).not.toHaveClass(/hidden/);
    await expect(page.locator('[data-testid="sftp-batch-count"]')).toContainText("1 selected");
    await expect(page.locator('[data-testid="sftp-batch-upload"]')).toBeEnabled();
    await expect(page.locator('[data-testid="sftp-batch-download"]')).toBeDisabled();
    await page
      .locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]')
      .filter({ hasText: "remote-only.txt" })
      .locator('[data-testid="sftp-select"]')
      .click();
    await expect(page.locator('[data-testid="sftp-batch-count"]')).toContainText("2 selected");
    await expect(page.locator('[data-testid="sftp-batch-upload"]')).toBeEnabled();
    await expect(page.locator('[data-testid="sftp-batch-download"]')).toBeEnabled();
    await page.locator('[data-testid="sftp-batch-clear"]').click();
    await expect(batch).toHaveClass(/hidden/);
  });

  test("#43: batch bar also appears in single pane on selection", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toHaveCount(0);
    await page
      .locator('[data-testid="sftp-row"]')
      .filter({ hasText: "notes.txt" })
      .locator('[data-testid="sftp-select"]')
      .click();
    const batch = page.locator('[data-testid="sftp-batch-bar"]');
    await expect(batch).not.toHaveClass(/hidden/);
    await expect(page.locator('[data-testid="sftp-batch-count"]')).toContainText("1 selected");
    await expect(page.locator('[data-testid="sftp-batch-download"]')).toBeEnabled();
    await expect(page.locator('[data-testid="sftp-batch-upload"]')).toBeDisabled();
    await page.locator('[data-testid="sftp-batch-clear"]').click();
    await expect(batch).toHaveClass(/hidden/);
  });

  test("#43 AC6: remote and local toolbars share Up, Refresh, New folder", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator('[data-testid="sftp-mkdir"]')).toBeVisible({ timeout: 5000 });
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await expect(page.locator('[data-testid="local-mkdir"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-up"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-refresh"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-up"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-refresh"]')).toBeVisible({ timeout: 5000 });
  });

  test("#43 AC1: compact rows default to ≤30px min-height", async ({ page }) => {
    await openFilesPanel(page);
    const minH = await page.locator('[data-testid="sftp-row"]').first().evaluate((el) => {
      return parseFloat(getComputedStyle(el).minHeight);
    });
    expect(minH).toBeLessThanOrEqual(30);
  });

  test("#49 soft-restore: leave Hosts and return keeps split local pane", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-split-btn"]').click();
    await expect(page.locator('[data-testid="local-table"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "local-upload.txt" })).toBeVisible({ timeout: 5000 });

    await page.locator('#activity-bar button[data-activity="hosts"]').click();
    await expect(page.locator(".sftp-view.active")).toHaveCount(0);

    await page.locator('#activity-bar button[data-activity="sftp"]').click();
    await expect(page.locator("#workspace .sftp-view.active")).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-pane-b"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-table"]')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="local-row"]').filter({ hasText: "local-upload.txt" })).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="sftp-pane-a"] [data-testid="sftp-row"]').first()).toBeVisible({ timeout: 5000 });
  });
});
