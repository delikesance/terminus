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
  chevronLeft: svg(`<path d="M14 6l-6 6 6 6"/>`),
  chevronRight: svg(`<path d="M10 6l6 6-6 6"/>`),
  reconnect: svg(`<path d="M20 12a8 8 0 1 1-2.2-5.5"/><path d="M20 4v6h-6"/>`),
  more: svg(`<circle cx="6.5" cy="12" r="1.2"/><circle cx="12" cy="12" r="1.2"/><circle cx="17.5" cy="12" r="1.2"/>`),
};
