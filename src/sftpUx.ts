/**
 * SFTP file-browser UX helpers (#43).
 */

export const SFTP_ROW_COMPACT_MIN_PX = 28;

export type FileListEntry = { name: string };

export type SftpListFilter = {
  query: string;
  showHidden: boolean;
};

/** Dot-prefixed names except `.` and `..`. */
export function isHiddenEntry(name: string): boolean {
  return name.startsWith(".") && name !== "." && name !== "..";
}

export function filterFileEntries<T extends FileListEntry>(
  entries: T[],
  filter: SftpListFilter,
): T[] {
  const q = filter.query.trim().toLowerCase();
  return entries.filter((e) => {
    if (!filter.showHidden && isHiddenEntry(e.name)) return false;
    if (q && !e.name.toLowerCase().includes(q)) return false;
    return true;
  });
}

export function selectionCount(local: number, remote: number): number {
  return local + remote;
}

export function shouldShowBatchBar(opts: {
  split: boolean;
  canTransfer: boolean;
  localSelected: number;
  remoteSelected: number;
}): boolean {
  if (!opts.split || !opts.canTransfer) return false;
  return opts.localSelected + opts.remoteSelected > 0;
}

export function batchUploadEnabled(localSelected: number): boolean {
  return localSelected > 0;
}

export function batchDownloadEnabled(remoteSelected: number): boolean {
  return remoteSelected > 0;
}

export function sftpSearchPlaceholder(paneLabel: string): string {
  return `Filter ${paneLabel}…`;
}
