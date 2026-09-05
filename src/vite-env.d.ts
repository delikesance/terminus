/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Set to `"1"` to expose `window.__terminusTest` for Playwright. */
  readonly VITE_E2E?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
