/**
 * Windowed SFTP file list (#50).
 */

import { SFTP_ROW_COMPACT_MIN_PX } from "./sftpUx.js";

export const SFTP_VIRTUAL_ROW_GAP_PX = 1;
export const SFTP_VIRTUAL_ROW_STRIDE_PX = SFTP_ROW_COMPACT_MIN_PX + SFTP_VIRTUAL_ROW_GAP_PX;
export const SFTP_VIRTUAL_OVERSCAN = 6;

export type VirtualRange = {
  start: number;
  end: number;
  paddingTop: number;
  paddingBottom: number;
  totalHeight: number;
};

export type VirtualListHandle<T> = {
  setItems(items: T[]): void;
  refresh(): void;
  destroy(): void;
};

export type MountVirtualListOpts<T> = {
  viewport: HTMLElement;
  items: T[];
  renderRow: (item: T, index: number) => string;
  onSliceRendered?: () => void;
};

export function computeVirtualRange(
  scrollTop: number,
  viewportHeight: number,
  itemCount: number,
  rowStride = SFTP_VIRTUAL_ROW_STRIDE_PX,
  overscan = SFTP_VIRTUAL_OVERSCAN,
): VirtualRange {
  const totalHeight = Math.max(0, itemCount * rowStride);
  if (itemCount === 0 || viewportHeight <= 0) {
    return { start: 0, end: 0, paddingTop: 0, paddingBottom: 0, totalHeight };
  }
  const first = Math.max(0, Math.floor(Math.max(0, scrollTop) / rowStride) - overscan);
  const visible = Math.ceil(viewportHeight / rowStride) + overscan * 2;
  const start = first;
  const end = Math.min(itemCount, start + visible);
  const paddingTop = start * rowStride;
  const paddingBottom = Math.max(0, totalHeight - end * rowStride);
  return { start, end, paddingTop, paddingBottom, totalHeight };
}

export function mountVirtualList<T>(opts: MountVirtualListOpts<T>): VirtualListHandle<T> {
  const { viewport, renderRow } = opts;
  let items = opts.items;
  let raf = 0;
  viewport.classList.add("sftp-virtual-viewport");
  viewport.innerHTML = `<div class="sftp-virtual-spacer"><div class="sftp-virtual-rows"></div></div>`;
  const spacer = viewport.querySelector(".sftp-virtual-spacer") as HTMLElement;
  const rowsEl = viewport.querySelector(".sftp-virtual-rows") as HTMLElement;

  const paint = () => {
    raf = 0;
    const range = computeVirtualRange(viewport.scrollTop, viewport.clientHeight, items.length);
    spacer.style.height = `${range.totalHeight}px`;
    rowsEl.style.paddingTop = `${range.paddingTop}px`;
    rowsEl.style.paddingBottom = `${range.paddingBottom}px`;
    const html: string[] = [];
    for (let i = range.start; i < range.end; i++) {
      html.push(renderRow(items[i]!, i));
    }
    rowsEl.innerHTML = html.join("");
    opts.onSliceRendered?.();
  };

  const schedule = () => {
    if (raf) return;
    raf = requestAnimationFrame(paint);
  };

  const onScroll = () => schedule();
  viewport.addEventListener("scroll", onScroll, { passive: true });
  const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(() => schedule()) : null;
  ro?.observe(viewport);
  paint();

  return {
    setItems(next) {
      items = next;
      viewport.scrollTop = 0;
      paint();
    },
    refresh() {
      paint();
    },
    destroy() {
      if (raf) cancelAnimationFrame(raf);
      viewport.removeEventListener("scroll", onScroll);
      ro?.disconnect();
      viewport.innerHTML = "";
    },
  };
}
