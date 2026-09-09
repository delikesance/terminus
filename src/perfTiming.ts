/**
 * Small timing helpers for SFTP filter debounce (#48).
 */

export const SFTP_FILTER_DEBOUNCE_MS = 100;

export function createTrailingDebounce(ms: number, fn: () => void) {
  let timer = 0;
  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = window.setTimeout(() => {
      timer = 0;
      fn();
    }, ms) as unknown as number;
  };
  const cancel = () => {
    if (timer) {
      clearTimeout(timer);
      timer = 0;
    }
  };
  const pending = () => timer !== 0;
  return { schedule, cancel, pending };
}

/** Frame paint rate cap (#53). */
export const FRAME_MIN_MS = 1000 / 60;
