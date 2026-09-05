# Terminus E2E Testing with Playwright

This directory contains end-to-end tests for Terminus P0 UX issues using Playwright.

## QA Contract Compliance

Tests align with exact QA specifications from **PR #6** and issues **#2, #3, #4**.

### data-testid Selectors (Exact)
| Zone | Selector | Usage |
|------|----------|-------|
| Ungrouped | `group-ungrouped` | Ungrouped group container |
| Ungrouped | `group-label` | "Ungrouped" label text |
| Ungrouped | `group-count` | Inline count badge |
| Host | `host-{id}` | Individual host items (dynamic ID) |
| Host | `host-local` | "This computer" item |
| Host | `connection-dot[data-state]` | Connection status dot (local/connected/disconnected/connecting/error) |
| Host | `open-count-pill` | Session count badge |
| Sync | `sync-badge[data-state]` | Sync status badge (unconfigured/idle/syncing/offline/error) |
| Sync | `sync-detail-error` | Error detail panel |

### Test Bridge API

Tests use `window.__terminusTest` bridge (auto-initialized in dev/test mode):

```typescript
interface TerminusTestBridge {
  // E2E-1: Ungrouped count
  seedUngroupedHosts(count: number): Promise<void>;
  clearUngroupedHosts(): Promise<void>;
  groupsDelete(groupId: string): Promise<void>;
  restoreGroup(groupId: string): Promise<void>;
  
  // E2E-2: Connection dots
  sessionOpenSsh(hostId: string): Promise<string>;
  sessionClose(sessionId: string): Promise<void>;
  setConnection(hostId: string, state: string): Promise<void>;  // ⭐ NEW
  
  // E2E-3: SyncStatus
  setSyncStatus(state: string, lastError?: string): Promise<void>;
}
```

**Key Method**: `setConnection(hostId, state)` allows testing all 5 connection states without docker/sshd. Backend provides `test_set_connection` command.

## Test Suites

### E2E-1: Ungrouped Count Badge (Issue #2)
- **File**: `ungrouped-count.spec.ts`
- **Coverage**: Inline badge, formula `!deleted_at && !group_id`, edge case 0, restore behavior
- **Status**: ✅ Selectors implemented, bridge wired, tests executable

### E2E-2: Connection Dots (Issue #3)
- **File**: `connection-dots.spec.ts`
- **Coverage**: 5 connection states distinct from open_count pill, close last shell → pill=0
- **Status**: ✅ Selectors implemented, bridge wired, **no docker/sshd dependency** (uses `setConnection`)
- **CRITICAL**: Close last shell → connection unchanged + pill absent

### E2E-3: SyncStatus Badge (Issue #4)
- **File**: `sync-status.spec.ts`
- **Coverage**: 5 states (unconfigured ≠ offline), French copy, error handling
- **Status**: ✅ Selectors implemented, bridge wired, tests executable

## Running Tests

### Prerequisites
```bash
npm install
npm run build  # Build frontend for preview server
```

### Execute Tests
```bash
# Headless (CI mode)
npm run test:e2e

# UI mode (interactive)
npm run test:e2e:ui

# Debug mode
npm run test:e2e:debug

# Headed (see browser)
npm run test:e2e:headed
```

### CI Integration

GitHub Actions workflow `.github/workflows/ci.yml` includes `e2e-playwright` job:
- Installs Playwright browsers (chromium)
- Builds frontend with `npm run build`
- Runs tests against `vite preview` server
- Uploads failure reports as artifacts (7-day retention)

**Status**: ✅ Job is **required** for PR gating

## Implementation Details

### Frontend Integration
- **Bridge**: `src/testBridge.ts` (auto-initialized in dev/test mode via `import.meta.env.DEV`)
- **Selectors**: Implemented in `src/main.ts` with exact QA contract IDs
- **Mode**: Bridge only loads in development or test builds (not production)
- **Dependencies**: Tests run without docker/sshd using `setConnection` method

### Test Structure
Each spec file:
1. Imports test bridge helper
2. Uses exact `data-testid` selectors from QA contract
3. Calls bridge methods to manipulate backend state (e.g., `setConnection` for connection states)
4. Asserts UI behavior matches acceptance criteria

### Known Limitations
- Bridge relies on backend `test_*` commands: `test_set_connection`, `test_set_sync_status`
- Tests run against web preview; full Tauri runtime integration is optional enhancement

## Next Steps

1. ✅ **Done**: Playwright scaffolding, QA contract selectors, bridge API
2. ✅ **Done**: CI job `e2e-playwright` wired and required
3. ⏳ **Optional**: Add backend `test_*` commands if any bridge methods fail
4. ⏳ **Ready**: Run full test suite and verify all AC pass

## References

- [PR #6](https://github.com/delikesance/terminus/pull/6): Frontend UX fixes with QA contracts
- [Issue #2](https://github.com/delikesance/terminus/issues/2): Ungrouped count
- [Issue #3](https://github.com/delikesance/terminus/issues/3): Connection dots
- [Issue #4](https://github.com/delikesance/terminus/issues/4): SyncStatus states
