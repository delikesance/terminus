import { getCurrentWindow } from "@tauri-apps/api/window";

export type Theme = {
  id: string;
  name: string;
  background: string;
  foreground: string;
  cursor: string;
  selection_background: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  bright_black: string;
  bright_red: string;
  bright_green: string;
  bright_yellow: string;
  bright_blue: string;
  bright_magenta: string;
  bright_cyan: string;
  bright_white: string;
};

function hexToRgb(hex: string): [number, number, number] {
  const raw = hex.replace("#", "").trim();
  const full = raw.length === 3 ? raw.split("").map((c) => c + c).join("") : raw.padEnd(6, "0");
  return [
    Number.parseInt(full.slice(0, 2), 16) || 0,
    Number.parseInt(full.slice(2, 4), 16) || 0,
    Number.parseInt(full.slice(4, 6), 16) || 0,
  ];
}

function rgbToHex(r: number, g: number, b: number): string {
  const to = (n: number) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  return `#${to(r)}${to(g)}${to(b)}`;
}

function mix(a: string, b: string, t: number): string {
  const [ar, ag, ab] = hexToRgb(a);
  const [br, bg, bb] = hexToRgb(b);
  return rgbToHex(ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t);
}

function luma(hex: string): number {
  const [r, g, b] = hexToRgb(hex);
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

function fgRgba(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function chevronUri(stroke: string): string {
  return `url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='${stroke}' stroke-width='2'><path d='m6 9 6 6 6-6'/></svg>")`;
}

export function applyChrome(theme: Theme) {
  const dark = luma(theme.background) < 0.55;
  const root = document.documentElement;
  root.dataset.scheme = dark ? "dark" : "light";
  root.style.colorScheme = dark ? "dark" : "light";
  const set = (name: string, value: string) => root.style.setProperty(name, value);
  const ink = (alpha: number) => fgRgba(theme.foreground, alpha);
  set("--bg", theme.background);
  set("--text", theme.foreground);
  set("--accent", theme.blue || theme.cursor);
  set("--blue", theme.blue);
  set("--blue-press", theme.bright_blue);
  set("--red", theme.red);
  set("--green", theme.green);
  set("--yellow", theme.yellow);
  set("--magenta", theme.magenta);
  set("--cyan", theme.cyan);
  set("--on-accent", luma(theme.blue || theme.cursor) < 0.62 ? "#ffffff" : theme.foreground);
  set("--elevated", mix(theme.background, theme.foreground, dark ? 0.09 : 0.1));
  set("--grouped", mix(theme.background, theme.foreground, dark ? 0.05 : 0.06));
  if (dark) {
    set("--fill", "rgba(120, 120, 128, 0.24)");
    set("--fill-strong", "rgba(120, 120, 128, 0.36)");
    set("--line", "rgba(255, 255, 255, 0.08)");
    set("--line-strong", "rgba(255, 255, 255, 0.14)");
    set("--secondary", "rgba(235, 235, 245, 0.6)");
    set("--tertiary", "rgba(235, 235, 245, 0.38)");
    set("--placeholder", "rgba(235, 235, 245, 0.36)");
    set("--icon-muted", "rgba(235, 235, 245, 0.45)");
    set("--shadow", "0 18px 50px rgba(0, 0, 0, 0.45)");
    set("--scroll-thumb", "rgba(255, 255, 255, 0.18)");
    set("--chrome", "rgba(28, 28, 30, 0.82)");
    set("--sidebar-bg", "rgba(30, 30, 35, 0.75)");
    set("--hover", "rgba(255, 255, 255, 0.05)");
    set("--hover-strong", "rgba(255, 255, 255, 0.08)");
    set("--track", "rgba(255, 255, 255, 0.06)");
    set("--track-focus", "rgba(255, 255, 255, 0.09)");
    set("--muted-fill", "rgba(255, 255, 255, 0.04)");
    set("--seg-on", "rgba(255, 255, 255, 0.12)");
    set("--swatch-ring", "rgba(255, 255, 255, 0.18)");
    set("--inset", "rgba(0, 0, 0, 0.35)");
    set("--overlay", "color-mix(in srgb, var(--bg) 38%, black)");
    set("--hairline", "rgba(255, 255, 255, 0.06)");
    set("--nav-active", "rgba(120, 120, 128, 0.42)");
    set("--select-chevron", chevronUri("%23ebebf599"));
  } else {
    set("--fill", ink(0.09));
    set("--fill-strong", ink(0.15));
    set("--line", ink(0.16));
    set("--line-strong", ink(0.24));
    set("--secondary", ink(0.78));
    set("--tertiary", ink(0.6));
    set("--placeholder", ink(0.48));
    set("--icon-muted", ink(0.58));
    set("--shadow", `0 18px 48px ${ink(0.16)}`);
    set("--scroll-thumb", ink(0.3));
    set("--chrome", mix(theme.background, theme.foreground, 0.06));
    set("--sidebar-bg", mix(theme.background, theme.foreground, 0.08));
    set("--hover", ink(0.08));
    set("--hover-strong", ink(0.12));
    set("--track", ink(0.08));
    set("--track-focus", ink(0.12));
    set("--muted-fill", ink(0.07));
    set("--seg-on", ink(0.14));
    set("--swatch-ring", ink(0.28));
    set("--inset", ink(0.08));
    set("--overlay", ink(0.32));
    set("--hairline", ink(0.1));
    set("--nav-active", ink(0.16));
    const [fr, fg, fb] = hexToRgb(theme.foreground);
    set("--select-chevron", chevronUri(`rgba(${fr}%2C${fg}%2C${fb}%2C0.78)`));
  }
  try {
    void getCurrentWindow().setBackgroundColor(theme.background);
  } catch {
    /* vite preview */
  }
}
