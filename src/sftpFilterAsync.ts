/**
 * Async SFTP filter — Worker off main thread when N is large (#56).
 */

import { filterFileEntries, type FileListEntry, type SftpListFilter } from "./sftpUx";

export const SFTP_FILTER_WORKER_MIN = 200;

let worker: Worker | null = null;
let seq = 0;

function getWorker(): Worker | null {
  if (typeof Worker === "undefined") return null;
  if (worker) return worker;
  try {
    worker = new Worker(new URL("./sftpFilter.worker.ts", import.meta.url), { type: "module" });
    return worker;
  } catch {
    worker = null;
    return null;
  }
}

export async function filterFileEntriesAsync<T extends FileListEntry>(
  entries: T[],
  filter: SftpListFilter,
): Promise<T[]> {
  if (entries.length < SFTP_FILTER_WORKER_MIN) {
    return filterFileEntries(entries, filter);
  }
  const w = getWorker();
  if (!w) return filterFileEntries(entries, filter);
  const id = ++seq;
  return new Promise((resolve) => {
    const onMsg = (ev: MessageEvent) => {
      if (ev.data?.id !== id) return;
      w.removeEventListener("message", onMsg);
      resolve(ev.data.result as T[]);
    };
    w.addEventListener("message", onMsg);
    w.postMessage({ id, entries, filter });
    window.setTimeout(() => {
      w.removeEventListener("message", onMsg);
      resolve(filterFileEntries(entries, filter));
    }, 800);
  });
}
