/// <reference lib="webworker" />
import { filterFileEntries } from "./sftpUx";

self.onmessage = (ev: MessageEvent) => {
  const { id, entries, filter } = ev.data ?? {};
  try {
    const result = filterFileEntries(entries ?? [], filter ?? { query: "", showHidden: false });
    self.postMessage({ id, result });
  } catch (err) {
    self.postMessage({ id, result: [], error: String(err) });
  }
};
