/**
 * Unit tests for tab renaming logic (#tab-rename).
 */

import {
  sanitizeTabTitle,
  defaultPaneBaseTitle,
  resolveTabTitle,
  buildTabContextMenu,
  handleRenameInputKeydown,
  TabRenameManager,
} from "./tabRename.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  // 1. sanitizeTabTitle
  {
    checks.push(
      check(
        "sanitizeTabTitle trims valid string",
        sanitizeTabTitle("  Production Server  ") === "Production Server",
      ),
    );
    checks.push(
      check(
        "sanitizeTabTitle returns undefined for empty string",
        sanitizeTabTitle("") === undefined,
      ),
    );
    checks.push(
      check(
        "sanitizeTabTitle returns undefined for whitespace only",
        sanitizeTabTitle("    \t  \n  ") === undefined,
      ),
    );
    checks.push(
      check(
        "sanitizeTabTitle returns undefined for null/undefined",
        sanitizeTabTitle(null) === undefined && sanitizeTabTitle(undefined) === undefined,
      ),
    );
    const longString = "A".repeat(150);
    const sanitized = sanitizeTabTitle(longString);
    checks.push(
      check(
        "sanitizeTabTitle clamps to 100 characters",
        sanitized?.length === 100 && sanitized === "A".repeat(100),
      ),
    );
  }

  // 2. defaultPaneBaseTitle
  {
    checks.push(
      check(
        "defaultPaneBaseTitle local pane",
        defaultPaneBaseTitle({ kind: "local" }) === "This computer",
      ),
    );
    checks.push(
      check(
        "defaultPaneBaseTitle remote session title",
        defaultPaneBaseTitle({ kind: "ssh", sessionTitle: "prod-db" }) === "prod-db",
      ),
    );
    checks.push(
      check(
        "defaultPaneBaseTitle pending title",
        defaultPaneBaseTitle({ kind: "ssh", pendingTitle: "connecting-host" }) === "connecting-host",
      ),
    );
    checks.push(
      check(
        "defaultPaneBaseTitle empty fallback",
        defaultPaneBaseTitle({ kind: "ssh" }) === "Shell",
      ),
    );
  }

  // 3. resolveTabTitle
  {
    const p1 = { id: "p1", kind: "local" };
    const p2 = { id: "p2", kind: "local" };
    const p3 = { id: "p3", kind: "ssh", sessionTitle: "bastion" };

    // Single pane
    checks.push(
      check(
        "resolveTabTitle single local tab has no suffix",
        resolveTabTitle(p1, [p1, p3]) === "This computer",
      ),
    );

    // Duplicate panes get numbered
    checks.push(
      check(
        "resolveTabTitle multiple local tabs get suffix",
        resolveTabTitle(p1, [p1, p2, p3]) === "This computer · 1" &&
          resolveTabTitle(p2, [p1, p2, p3]) === "This computer · 2",
      ),
    );

    // Custom title overrides default title
    p1.customTitle = "My Local Box";
    checks.push(
      check(
        "resolveTabTitle custom title overrides default",
        resolveTabTitle(p1, [p1, p2, p3]) === "My Local Box",
      ),
    );

    // After p1 has custom title, p2 is the only remaining 'This computer', so no suffix needed!
    checks.push(
      check(
        "resolveTabTitle remaining pane loses suffix when duplicate count drops to 1",
        resolveTabTitle(p2, [p1, p2, p3]) === "This computer",
      ),
    );

    // Resetting custom title restores numbering
    p1.customTitle = undefined;
    checks.push(
      check(
        "resolveTabTitle resetting restores duplicate numbering",
        resolveTabTitle(p1, [p1, p2, p3]) === "This computer · 1" &&
          resolveTabTitle(p2, [p1, p2, p3]) === "This computer · 2",
      ),
    );
  }

  // 4. buildTabContextMenu
  {
    const menuWithoutCustom = buildTabContextMenu({
      paneId: "p1",
      hasCustomTitle: false,
      isExited: false,
      totalPanes: 2,
    });
    const renameItem = menuWithoutCustom.find((i) => i.action === "rename");
    const resetItem = menuWithoutCustom.find((i) => i.action === "reset-name");
    const closeItem = menuWithoutCustom.find((i) => i.action === "close");
    const closeOthersItem = menuWithoutCustom.find((i) => i.action === "close-others");
    const sessionItem = menuWithoutCustom.find((i) => i.action === "duplicate");

    checks.push(
      check(
        "buildTabContextMenu includes Rename with testId",
        renameItem !== undefined && renameItem.testId === "tab-rename",
      ),
    );
    checks.push(
      check(
        "buildTabContextMenu omits Reset name when no custom title",
        resetItem === undefined,
      ),
    );
    checks.push(
      check(
        "buildTabContextMenu includes Close and Close others",
        closeItem !== undefined && closeOthersItem !== undefined && !closeOthersItem.hidden,
      ),
    );
    checks.push(
      check(
        "buildTabContextMenu sessionItem is New session when not exited",
        sessionItem !== undefined && sessionItem.label === "New session",
      ),
    );

    const menuWithCustom = buildTabContextMenu({
      paneId: "p1",
      hasCustomTitle: true,
      isExited: true,
      totalPanes: 1,
    });
    const resetCustomItem = menuWithCustom.find((i) => i.action === "reset-name");
    const reconnectItem = menuWithCustom.find((i) => i.action === "reconnect");
    const closeOthersHidden = menuWithCustom.find((i) => i.action === "close-others");

    checks.push(
      check(
        "buildTabContextMenu includes Reset name when hasCustomTitle is true",
        resetCustomItem !== undefined && resetCustomItem.testId === "tab-reset-name",
      ),
    );
    checks.push(
      check(
        "buildTabContextMenu shows Reconnect when isExited is true",
        reconnectItem !== undefined && reconnectItem.label === "Reconnect",
      ),
    );
    checks.push(
      check(
        "buildTabContextMenu hides Close others when only 1 pane",
        closeOthersHidden !== undefined && closeOthersHidden.hidden === true,
      ),
    );
  }

  // 5. handleRenameInputKeydown
  {
    let committed = false;
    let cancelled = false;
    const callbacks = {
      onCommit: () => {
        committed = true;
      },
      onCancel: () => {
        cancelled = true;
      },
    };

    const enterHandled = handleRenameInputKeydown("Enter", callbacks);
    checks.push(
      check(
        "handleRenameInputKeydown handles Enter by committing",
        enterHandled === true && committed === true && cancelled === false,
      ),
    );

    committed = false;
    cancelled = false;
    const escapeHandled = handleRenameInputKeydown("Escape", callbacks);
    checks.push(
      check(
        "handleRenameInputKeydown handles Escape by cancelling",
        escapeHandled === true && committed === false && cancelled === true,
      ),
    );

    committed = false;
    cancelled = false;
    const keyHandled = handleRenameInputKeydown("a", callbacks);
    checks.push(
      check(
        "handleRenameInputKeydown ignores other keys",
        keyHandled === false && committed === false && cancelled === false,
      ),
    );
  }

  // 6. TabRenameManager
  {
    const mgr = new TabRenameManager();
    checks.push(check("TabRenameManager initial editingPaneId is null", mgr.getEditingPaneId() === null));

    const s1 = mgr.start("p1");
    checks.push(
      check(
        "TabRenameManager start p1 sets editingPaneId",
        s1.started && s1.previousPaneId === null && mgr.isEditing("p1") && mgr.getEditingPaneId() === "p1",
      ),
    );

    const s2 = mgr.start("p2");
    checks.push(
      check(
        "TabRenameManager switching to p2 returns previousPaneId p1",
        s2.started && s2.previousPaneId === "p1" && mgr.isEditing("p2"),
      ),
    );

    // Commit on wrong pane id does nothing
    const cWrong = mgr.commit("p1", "Ignored");
    checks.push(
      check(
        "TabRenameManager commit with non-matching pane id does not commit",
        !cWrong.committed && mgr.isEditing("p2"),
      ),
    );

    // Commit on matching pane id commits and cleans
    const cRight = mgr.commit("p2", "  New Name  ");
    checks.push(
      check(
        "TabRenameManager commit with matching pane id commits and sanitizes",
        cRight.committed && cRight.sanitizedTitle === "New Name" && mgr.getEditingPaneId() === null,
      ),
    );

    // Cancel
    mgr.start("p3");
    checks.push(check("TabRenameManager cancel resets state", mgr.cancel("p3") && mgr.getEditingPaneId() === null));

    // Reset
    mgr.start("p4");
    checks.push(check("TabRenameManager reset resets state", mgr.reset("p4") && mgr.getEditingPaneId() === null));
  }

  return checks;
}

const results = runTests();
let failed = 0;
for (const r of results) {
  const mark = r.ok ? "ok" : "FAIL";
  if (!r.ok) failed += 1;
  console.log(`${mark}  ${r.name}${r.detail ? ` — ${r.detail}` : ""}`);
}
console.log(`\n${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
