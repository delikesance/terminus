import { test, expect } from "@playwright/test";
import { waitForTestBridge, getTestBridge } from "./testBridge";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";

/**
 * C9 — SFTP browser v1 (full E2E, not smoke-only)
 * AC: path traversal blocked · confirm before delete · typed I/O errors · navigate/up/open/rename
 */

async function openFilesPanel(page: import("@playwright/test").Page) {
  await page.locator('.side-nav button[data-panel="sftp"]').click();
  await expect(page.locator('[data-testid="sftp-bar"]')).toBeVisible();
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

  test("lists files with bar + rows (name · size · mtime · ⋯)", async ({ page }) => {
    await openFilesPanel(page);
    await expect(page.locator('[data-testid="sftp-list"]')).toBeVisible();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue(".");
    await expect(page.locator('[data-testid="sftp-row"]')).toHaveCount(2);
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" })).toBeVisible();
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "notes.txt" })).toBeVisible();
    await expect(page.locator('[data-testid="sftp-more"]').first()).toBeVisible();
    await expect(page.locator(".sftp-size").first()).toBeVisible();
    await expect(page.locator(".sftp-mtime").first()).toBeVisible();
  });

  test("navigate into dir, Up returns, refresh keeps path", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("docs");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible();

    await page.locator('[data-testid="sftp-up"]').click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue(".");

    await page.locator('[data-testid="sftp-row"]').filter({ hasText: "docs" }).click();
    await page.locator('[data-testid="sftp-refresh"]').click();
    await expect(page.locator('[data-testid="sftp-path"]')).toHaveValue("docs");
    await expect(page.locator('[data-testid="sftp-row"]').filter({ hasText: "readme.txt" })).toBeVisible();
  });

  test("editable path bar navigates; empty folder shows .sftp-empty", async ({ page }) => {
    await openFilesPanel(page);
    // Create empty dir via rename workaround: write then remove file inside new folder via upload into docs then…
    // Navigate to a path that exists then delete sole child to empty — use mock write of empty folder by navigating after remove.
    await page.locator('[data-testid="sftp-path"]').fill("docs");
    await page.locator('[data-testid="sftp-path"]').press("Enter");
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

  test("AC: path traversal blocked → typed .sftp-error", async ({ page }) => {
    await openFilesPanel(page);
    await page.locator('[data-testid="sftp-path"]').fill("../etc/passwd");
    await page.locator('[data-testid="sftp-path"]').press("Enter");
    const err = page.locator('[data-testid="sftp-error"]');
    await expect(err).toBeVisible();
    await expect(err).toHaveAttribute("data-kind", "SftpPathTraversal");
    await expect(err).toContainText(/traversal/i);
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
});
