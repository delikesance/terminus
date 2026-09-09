#!/usr/bin/env bash
# Native-comparable perf gates (#57 / epic #47).
# Preview E2E proves interactive FPS/heap/virtualization.
# Full Tauri WebView + real PTY flood still needs TAURI_E2E=1 locally.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== CPU harness =="
npm run test:perf-load

echo "== Timing / virtual list units =="
npm run test:perf-timing
npm run test:sftp-virtual-list

echo "== Playwright perf (Chromium preview) =="
npm run test:e2e:perf

if [[ "${TAURI_E2E:-}" == "1" ]]; then
  echo "== Tauri WebView E2E (native) =="
  npm run test:e2e:perf
else
  echo "Skip native Tauri run (set TAURI_E2E=1 + running app to enable)."
fi

echo "Gates (epic #47): FPS≥55, heap growth<50MB, virtual DOM rows<120 — see artifacts/perf-e2e-*.json"
