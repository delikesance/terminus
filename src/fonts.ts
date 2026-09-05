const APPLE_MONO = ["SF Mono", "Menlo", "Monaco"] as const;
export const WEB_MONO = "IBM Plex Mono";

function localFontExists(name: string): boolean {
  const probe = "mmmmmmmmlli";
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return false;
  ctx.font = "16px monospace";
  const fallback = ctx.measureText(probe).width;
  ctx.font = `16px "${name}", monospace`;
  return ctx.measureText(probe).width !== fallback;
}

/** Single loaded face — xterm cell metrics break if the first family is missing. */
export function resolveMonoFont(): string {
  for (const name of APPLE_MONO) {
    if (localFontExists(name)) return `"${name}"`;
  }
  return `"${WEB_MONO}"`;
}
