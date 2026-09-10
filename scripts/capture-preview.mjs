/**
 * Capture marketing screenshots for docs/media (VITE_E2E preview).
 *
 * Usage:
 *   VITE_E2E=1 nix develop -c npm run build
 *   nix develop -c npm run capture:preview
 *
 * Note: the e2e mock returns empty terminal frames, so sidebar/chrome shots are
 * useful for layout checks. Prefer a real Tauri window capture (or the checked-in
 * docs/screenshots/*.png) for the README hero product shot.
 */
import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdir, access } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const outDir = path.join(root, "docs", "media");
const port = 4173;
const base = `http://127.0.0.1:${port}`;

async function waitForServer(url, ms = 120_000) {
  const start = Date.now();
  while (Date.now() - start < ms) {
    try {
      const res = await fetch(url);
      if (res.ok || res.status === 404) return;
    } catch {
      /* retry */
    }
    await new Promise((r) => setTimeout(r, 400));
  }
  throw new Error(`Server not ready: ${url}`);
}

async function main() {
  await mkdir(outDir, { recursive: true });
  await access(path.join(root, "dist", "index.html")).catch(() => {
    throw new Error("Missing dist/ — run: VITE_E2E=1 nix develop -c npm run build");
  });

  const preview = spawn(
    "npm",
    ["run", "preview", "--", "--port", String(port), "--host", "127.0.0.1", "--strictPort"],
    {
      cwd: root,
      env: { ...process.env, VITE_E2E: "1" },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  preview.stderr?.on("data", () => {});
  try {
    await waitForServer(base);
    const browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    await page.goto(base + "/", { waitUntil: "networkidle" });
    await page.waitForFunction(() => typeof window.__terminusTest !== "undefined", null, {
      timeout: 15_000,
    });

    await page.evaluate(async () => {
      const t = window.__terminusTest;
      t.clearAllHosts();
      t.seedUngroupedHosts(3);
      await t.sessionOpenSsh("test-ungrouped-0");
    });
    await page.waitForTimeout(800);
    await page.locator(".pane.active").first().waitFor({ state: "visible", timeout: 8_000 }).catch(() => {});

    await page.screenshot({
      path: path.join(outDir, "app-main.png"),
      type: "png",
    });

    await page.locator('[data-testid="nav-hosts"], #btn-hosts, [aria-label="Hosts"]').first().click().catch(() => {});
    await page.waitForTimeout(300);
    await page.screenshot({
      path: path.join(outDir, "app-hosts.png"),
      type: "png",
      clip: { x: 0, y: 0, width: 360, height: 800 },
    });

    await browser.close();
    console.log("Wrote docs/media/app-main.png and app-hosts.png");
  } finally {
    preview.kill("SIGTERM");
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
