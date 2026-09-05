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

export function applyChrome(theme: Theme) {
  const dark = luma(theme.background) < 0.55;
  const root = document.documentElement;
  root.dataset.scheme = dark ? "dark" : "light";
  root.style.colorScheme = dark ? "dark" : "light";
  const set = (name: string, value: string) => root.style.setProperty(name, value);
  set("--bg", theme.background);
  set("--text", theme.foreground);
  set("--accent", theme.blue || theme.cursor);
  set("--blue", theme.blue);
  set("--blue-press", theme.bright_blue);
  set("--red", theme.red);
  set("--green", theme.green);
  set("--yellow", theme.yellow);
  set("--elevated", mix(theme.background, theme.foreground, dark ? 0.09 : 0.07));
  set("--grouped", mix(theme.background, theme.foreground, dark ? 0.05 : 0.04));
  set("--fill", dark ? "rgba(120, 120, 128, 0.24)" : "rgba(0, 0, 0, 0.06)");
  set("--fill-strong", dark ? "rgba(120, 120, 128, 0.36)" : "rgba(0, 0, 0, 0.1)");
  set("--line", dark ? "rgba(255, 255, 255, 0.08)" : "rgba(0, 0, 0, 0.08)");
  set("--line-strong", dark ? "rgba(255, 255, 255, 0.14)" : "rgba(0, 0, 0, 0.14)");
  set("--secondary", dark ? "rgba(235, 235, 245, 0.6)" : "rgba(0, 0, 0, 0.55)");
  set("--tertiary", dark ? "rgba(235, 235, 245, 0.38)" : "rgba(0, 0, 0, 0.38)");
  set("--shadow", dark ? "0 18px 50px rgba(0, 0, 0, 0.45)" : "0 18px 40px rgba(0, 0, 0, 0.12)");
  set("--scroll-thumb", dark ? "rgba(255, 255, 255, 0.18)" : "rgba(0, 0, 0, 0.22)");
  try {
    void getCurrentWindow().setBackgroundColor(theme.background);
  } catch {
    /* vite preview */
  }
}
