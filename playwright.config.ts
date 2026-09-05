import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for Terminus E2E tests
 * Supports both web (vite preview) and Tauri runtime testing
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? 'github' : 'list',
  
  use: {
    baseURL: 'http://127.0.0.1:4173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  // Start preview server before running tests (serves built dist/)
  webServer: process.env.TAURI_E2E
    ? undefined
    : {
        command: 'VITE_E2E=1 npm run preview -- --port 4173 --host 127.0.0.1 --strictPort',
        url: 'http://127.0.0.1:4173',
        reuseExistingServer: !process.env.CI,
        timeout: 120000,
      },
});
