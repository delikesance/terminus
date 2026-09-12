/**
 * Pure helper functions and logic for tab renaming.
 */

export type PaneLike = {
  id?: string;
  customTitle?: string | null;
  kind?: string | null;
  sessionTitle?: string | null;
  pendingTitle?: string | null;
};

export type MenuItemDescriptor = {
  label?: string;
  testId?: string;
  danger?: boolean;
  hidden?: boolean;
  disabled?: boolean;
  sep?: boolean;
  action?: string;
};

/**
 * Sanitize and clean a user-entered tab title.
 * Returns `undefined` if empty or whitespace-only (indicating reset to default title).
 * Clamps title to a maximum of 100 characters.
 */
export function sanitizeTabTitle(raw: string | null | undefined): string | undefined {
  if (!raw) return undefined;
  const trimmed = raw.trim();
  if (!trimmed) return undefined;
  return trimmed.slice(0, 100);
}

/**
 * Determine the default base title for a pane without any duplicate index suffix.
 */
export function defaultPaneBaseTitle(pane?: PaneLike | null): string {
  if (!pane) return "";
  const kind = pane.kind;
  if (kind === "local") return "This computer";
  return pane.sessionTitle || pane.pendingTitle || "Shell";
}

/**
 * Resolve the display title for a pane, taking into account:
 * 1. User custom title (if set, always used directly)
 * 2. Default base title ("This computer" / session title / "Shell")
 * 3. Disambiguation suffix (`· 2`, `· 3`) only among panes sharing the same default title without a custom title.
 */
export function resolveTabTitle(
  pane?: PaneLike | null,
  allPanes: PaneLike[] = [],
): string {
  if (!pane) return "";
  if (pane.customTitle && pane.customTitle.trim()) {
    return pane.customTitle.trim();
  }

  const base = defaultPaneBaseTitle(pane);
  // Panes that don't have custom titles and share the same base
  const same = allPanes.filter((p) => {
    if (p.customTitle && p.customTitle.trim()) return false;
    return defaultPaneBaseTitle(p) === base;
  });

  if (same.length < 2) return base;
  const idx = same.indexOf(pane);
  return idx >= 0 ? `${base} · ${idx + 1}` : base;
}

/**
 * Generate tab context menu items.
 */
export function buildTabContextMenu(opts: {
  paneId: string;
  hasCustomTitle: boolean;
  isExited: boolean;
  totalPanes: number;
}): MenuItemDescriptor[] {
  const items: MenuItemDescriptor[] = [
    { label: "Rename", testId: "tab-rename", action: "rename" },
  ];

  if (opts.hasCustomTitle) {
    items.push({
      label: "Reset name",
      testId: "tab-reset-name",
      action: "reset-name",
    });
  }

  items.push(
    { sep: true },
    { label: "Close", testId: "tab-close", action: "close" },
    {
      label: "Close others",
      testId: "tab-close-others",
      action: "close-others",
      hidden: opts.totalPanes < 2,
    },
    {
      label: "Close all",
      testId: "tab-close-all",
      action: "close-all",
      hidden: opts.totalPanes < 1,
    },
    {
      label: opts.isExited ? "Reconnect" : "New session",
      testId: opts.isExited ? "tab-reconnect" : "tab-duplicate",
      action: opts.isExited ? "reconnect" : "duplicate",
    },
  );

  return items;
}

/**
 * Handle keydown events on the inline tab rename input.
 * Returns true if the key was handled (Enter or Escape).
 */
export function handleRenameInputKeydown(
  key: string,
  callbacks: {
    onCommit: () => void;
    onCancel: () => void;
  },
): boolean {
  if (key === "Enter") {
    callbacks.onCommit();
    return true;
  }
  if (key === "Escape") {
    callbacks.onCancel();
    return true;
  }
  return false;
}

/**
 * State manager for tab renaming lifecycle.
 */
export class TabRenameManager {
  private editingPaneId: string | null = null;

  getEditingPaneId(): string | null {
    return this.editingPaneId;
  }

  isEditing(paneId: string): boolean {
    return this.editingPaneId === paneId;
  }

  start(paneId: string): { started: boolean; previousPaneId: string | null } {
    const previous = this.editingPaneId;
    this.editingPaneId = paneId;
    return { started: true, previousPaneId: previous };
  }

  commit(paneId: string, newTitle: string): { committed: boolean; sanitizedTitle: string | undefined } {
    if (this.editingPaneId !== paneId) return { committed: false, sanitizedTitle: undefined };
    this.editingPaneId = null;
    return { committed: true, sanitizedTitle: sanitizeTabTitle(newTitle) };
  }

  cancel(paneId: string): boolean {
    if (this.editingPaneId !== paneId) return false;
    this.editingPaneId = null;
    return true;
  }

  reset(paneId: string): boolean {
    if (this.editingPaneId === paneId) {
      this.editingPaneId = null;
    }
    return true;
  }
}
