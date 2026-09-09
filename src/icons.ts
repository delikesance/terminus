const svg = (path: string) =>
  `<svg viewBox="0 0 24 24" aria-hidden="true">${path}</svg>`;

export const icons = {
  sidebar: svg(
    `<rect x="4" y="5" width="16" height="14" rx="2"/><path d="M10 5v14"/>`,
  ),
  plus: svg(`<path d="M12 5v14M5 12h14"/>`),
  search: svg(`<circle cx="11" cy="11" r="6"/><path d="m16 16 4 4"/>`),
  settings: svg(
    `<path d="M4 8h16M4 16h16"/><circle cx="9" cy="8" r="2.1"/><circle cx="15" cy="16" r="2.1"/>`,
  ),
  close: svg(`<path d="M6 6l12 12M18 6 6 18"/>`),
  laptop: svg(
    `<rect x="4" y="6" width="16" height="10" rx="1.6"/><path d="M3 18h18"/>`,
  ),
  server: svg(
    `<rect x="4" y="4" width="16" height="7" rx="1.6"/><rect x="4" y="13" width="16" height="7" rx="1.6"/><path d="M8 7.5h.01M8 16.5h.01"/>`,
  ),
  key: svg(
    `<circle cx="8" cy="14" r="3.2"/><path d="M11 14h9l-2 2 2 2"/>`,
  ),
  password: svg(
    `<rect x="5" y="10" width="14" height="10" rx="2"/><path d="M8 10V8a4 4 0 0 1 8 0v2"/>`,
  ),
  snippet: svg(
    `<path d="M8 7h8M8 12h8M8 17h5"/><rect x="4" y="4" width="16" height="16" rx="2"/>`,
  ),
  clock: svg(`<circle cx="12" cy="12" r="8"/><path d="M12 8v5l3 2"/>`),
  folder: svg(
    `<path d="M3.5 8.5V7a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-7.5z"/>`,
  ),
  file: svg(
    `<path d="M7 3.5h7l5 5V20a1.5 1.5 0 0 1-1.5 1.5h-10.5A1.5 1.5 0 0 1 5.5 20V5A1.5 1.5 0 0 1 7 3.5z"/><path d="M14 3.5V9h5.5"/>`,
  ),
  cloud: svg(
    `<path d="M7 17h10a4 4 0 0 0 .4-8 6 6 0 0 0-11.4 2A3.5 3.5 0 0 0 7 17z"/>`,
  ),
  terminal: svg(
    `<rect x="3.5" y="5" width="17" height="14" rx="2"/><path d="m8 10 3 2-3 2M13 14h4"/>`,
  ),
  tunnel: svg(
    `<path d="M4 12h6M14 12h6"/><circle cx="12" cy="12" r="2.2"/><path d="M7 8v8M17 8v8"/>`,
  ),
  chevronLeft: svg(`<path d="M14 6l-6 6 6 6"/>`),
  chevronRight: svg(`<path d="M10 6l6 6-6 6"/>`),
  reconnect: svg(`<path d="M20 12a8 8 0 1 1-2.2-5.5"/><path d="M20 4v6h-6"/>`),
  check: svg(`<path d="m6 12 4 4 8-8"/>`),
  more: svg(`<circle cx="6.5" cy="12" r="1.2"/><circle cx="12" cy="12" r="1.2"/><circle cx="17.5" cy="12" r="1.2"/>`),
  eye: svg(`<path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6z"/><circle cx="12" cy="12" r="2.5"/>`),
  eyeOff: svg(`<path d="M3 3l18 18"/><path d="M10.6 10.6A2.5 2.5 0 0 0 12 14.5a2.5 2.5 0 0 0 1.4-.4M6.7 6.7C4.6 8.1 3 10 3 10s3.5 6 10 6c1.2 0 2.3-.3 3.3-.8M14 9.3c.6-.8 1-1.7 1-2.3 0-2.2-1.8-4-4-4-.6 0-1.5.4-2.3 1"/>`),
  columns: svg(
    `<rect x="4" y="5" width="7" height="14" rx="1.5"/><rect x="13" y="5" width="7" height="14" rx="1.5"/>`,
  ),
  chevronDown: svg(`<path d="m6 9 6 6 6-6"/>`),
  home: svg(
    `<path d="M4.5 11 12 4.5 19.5 11"/><path d="M6.5 10.5V19h11v-8.5"/>`,
  ),
  arrowUp: svg(`<path d="M12 19V6"/><path d="m7 11 5-5 5 5"/>`),
  upload: svg(`<path d="M12 16V5"/><path d="m8 8 4-4 4 4"/><path d="M5 19h14"/>`),
  download: svg(`<path d="M12 5v11"/><path d="m8 12 4 4 4-4"/><path d="M5 19h14"/>`),
  fileCode: svg(
    `<path d="M7 3.5h7l5 5V20a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 20V5A1.5 1.5 0 0 1 7 3.5z"/><path d="M14 3.5V9h5.5M9.5 13.5 8 15l1.5 1.5M14.5 13.5 16 15l-1.5 1.5"/>`,
  ),
  fileImage: svg(
    `<rect x="4" y="5" width="16" height="14" rx="2"/><circle cx="9" cy="10" r="1.5"/><path d="m6.5 17 3.5-3.5 2.5 2.5 2.5-3.5 3 4.5"/>`,
  ),
  fileVideo: svg(
    `<rect x="3.5" y="6" width="12" height="12" rx="2"/><path d="m15.5 10 5-2.5v9L15.5 14z"/>`,
  ),
  fileAudio: svg(
    `<path d="M9 18V6l10-2v12"/><circle cx="7" cy="18" r="2.2"/><circle cx="17" cy="16" r="2.2"/>`,
  ),
  fileArchive: svg(
    `<path d="M7 3.5h10V20.5H7z"/><path d="M10 4.5h2M10 7h2M10 9.5h2"/><rect x="9.5" y="12.5" width="5" height="3.5" rx="0.6"/>`,
  ),
  filePdf: svg(
    `<path d="M7 3.5h7l5 5V20a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 20V5A1.5 1.5 0 0 1 7 3.5z"/><path d="M14 3.5V9h5.5M8 14h2.6a1.4 1.4 0 1 1 0 2.8H8V12.5"/>`,
  ),
  fileTable: svg(
    `<rect x="4" y="5" width="16" height="14" rx="2"/><path d="M4 10h16M4 15h16M10 5v14"/>`,
  ),
};

/** Official distro glyphs from font-logos (https://github.com/Lukas-W/font-logos). */
const DISTRO_FL: Record<string, string> = {
  ubuntu: "fl-ubuntu",
  kubuntu: "fl-kubuntu",
  zorin: "fl-zorin",
  neon: "fl-kde-neon",
  pop: "fl-pop-os",
  debian: "fl-debian",
  raspberry: "fl-raspberry-pi",
  fedora: "fl-fedora",
  nobara: "fl-nobara",
  centos: "fl-centos",
  rhel: "fl-redhat",
  rocky: "fl-rocky-linux",
  alma: "fl-almalinux",
  arch: "fl-archlinux",
  endeavour: "fl-endeavour",
  garuda: "fl-garuda",
  artix: "fl-artix",
  manjaro: "fl-manjaro",
  nixos: "fl-nixos",
  alpine: "fl-alpine",
  opensuse: "fl-opensuse",
  leap: "fl-leap",
  tumbleweed: "fl-tumbleweed",
  kali: "fl-kali-linux",
  mint: "fl-linuxmint",
  elementary: "fl-elementary",
  gentoo: "fl-gentoo",
  void: "fl-void",
  macos: "fl-apple",
  freebsd: "fl-freebsd",
  openbsd: "fl-openbsd",
  linux: "fl-tux",
  amazon: "fl-tux",
  oracle: "fl-tux",
  netbsd: "fl-tux",
  solus: "fl-solus",
  deepin: "fl-deepin",
  devuan: "fl-devuan",
  coreos: "fl-coreos",
  mageia: "fl-mageia",
  slackware: "fl-slackware",
  parrot: "fl-parrot",
  postmarketos: "fl-postmarketos",
  qubesos: "fl-qubesos",
  tails: "fl-tails",
  vanilla: "fl-vanilla",
  guix: "fl-gnu-guix",
  mxlinux: "fl-mxlinux",
  aosc: "fl-aosc",
};

const OS_ALIASES: Record<string, string> = {
  archlinux: "arch",
  "pop-os": "pop",
  pop_os: "pop",
  linuxmint: "mint",
  almalinux: "alma",
  "rocky-linux": "rocky",
  redhat: "rhel",
  raspbian: "raspberry",
  raspberrypi: "raspberry",
  raspios: "raspberry",
  "kde-neon": "neon",
  kdeneon: "neon",
  endeavouros: "endeavour",
  darwin: "macos",
  "opensuse-leap": "leap",
  "opensuse-tumbleweed": "tumbleweed",
  qubes: "qubesos",
  guixsd: "guix",
  mx: "mxlinux",
  zorinos: "zorin",
};

/** Windows is not in font-logos; keep a filled mark. */
const WINDOWS_ICON =
  `<svg class="os-logo" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6.2 11.2 5.2v6.2H4V6.2zm8.4-.9L20 4.2v7.2h-7.6V5.3zM4 12.8h7.2V18.9L4 17.8v-5zm8.4 0H20V19.8l-7.6-1.2v-5.8z"/></svg>`;

const flIcon = (cls: string) => `<span class="os-fl ${cls}" aria-hidden="true"></span>`;

export function hostOsIcon(osId?: string | null): { icon: string; os: string } {
  const raw = (osId ?? "").trim().toLowerCase();
  if (!raw || raw === "unknown") return { icon: icons.server, os: "" };
  const os = OS_ALIASES[raw] ?? raw;
  if (os === "windows") return { icon: WINDOWS_ICON, os: "windows" };
  const cls = DISTRO_FL[os];
  if (cls) return { icon: flIcon(cls), os };
  return { icon: flIcon("fl-tux"), os: "linux" };
}

const CODE_EXT = new Set([
  "js", "jsx", "ts", "tsx", "mjs", "cjs", "py", "rs", "go", "java", "kt", "c", "cc", "cpp", "h", "hpp",
  "cs", "rb", "php", "swift", "lua", "sh", "bash", "zsh", "fish", "ps1", "html", "htm", "css", "scss",
  "less", "vue", "svelte", "json", "jsonc", "yaml", "yml", "toml", "xml", "sql", "graphql", "dockerfile",
]);
const IMAGE_EXT = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "heic", "avif"]);
const VIDEO_EXT = new Set(["mp4", "mov", "mkv", "webm", "avi", "m4v"]);
const AUDIO_EXT = new Set(["mp3", "wav", "flac", "aac", "ogg", "m4a", "wma"]);
const ARCHIVE_EXT = new Set(["zip", "tar", "gz", "tgz", "bz2", "7z", "rar", "xz"]);
const TABLE_EXT = new Set(["csv", "tsv", "xls", "xlsx", "ods"]);
const KEY_EXT = new Set(["pem", "key", "pub", "crt", "cer", "p12", "pfx"]);
const TEXT_EXT = new Set(["txt", "md", "markdown", "rst", "log", "ini", "cfg", "conf", "env"]);

export function sftpKindIcon(name: string, isDir: boolean): { icon: string; kind: string } {
  if (isDir) return { icon: icons.folder, kind: "dir" };
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1).toLowerCase() : "";
  if (ext === "pdf") return { icon: icons.filePdf, kind: "pdf" };
  if (CODE_EXT.has(ext)) return { icon: icons.fileCode, kind: "code" };
  if (IMAGE_EXT.has(ext)) return { icon: icons.fileImage, kind: "image" };
  if (VIDEO_EXT.has(ext)) return { icon: icons.fileVideo, kind: "video" };
  if (AUDIO_EXT.has(ext)) return { icon: icons.fileAudio, kind: "audio" };
  if (ARCHIVE_EXT.has(ext)) return { icon: icons.fileArchive, kind: "archive" };
  if (TABLE_EXT.has(ext)) return { icon: icons.fileTable, kind: "table" };
  if (KEY_EXT.has(ext)) return { icon: icons.key, kind: "key" };
  if (TEXT_EXT.has(ext) || !ext) return { icon: icons.file, kind: "text" };
  return { icon: icons.file, kind: "file" };
}
