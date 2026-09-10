# Canva preview brief — Terminus

Use these assets to assemble a social / GitHub hero in [Canva](https://www.canva.com) (or any editor). There is no Canva API in the repo; import the PNGs manually.

## Canvas

| Field | Value |
|-------|--------|
| Size | **1920 × 1080** (16:9) |
| Safe margin | 80 px from edges |
| Primary export | PNG or JPG, ≤ 1.5 MB for README |
| Alt export | 1280 × 720 for GitHub social preview |

## Brand / colors (from app chrome)

| Token | Hex (approx.) | Use |
|-------|----------------|-----|
| Background | `#1e1e2e` | Full-bleed base |
| Panel | `#181825` / graphite | Sidebar / card |
| Text | `#cdd6f4` | Headlines |
| Accent / blue | `#89b4fa` | CTA underline, cursor, dots |
| Green (live) | `#a6e3a1` | “Connected” cue |
| Muted | `#6c7086` | Subcopy |

Fonts: prefer a geometric sans for UI labels (e.g. Inter / Geist) and a monospace for the terminal strip (JetBrains Mono / Cascadia).

## Suggested layout

1. **Full-bleed dark gradient** (`#1e1e2e` → `#11111b`).
2. **Left third**: wordmark **Terminus**, one headline, one short line, optional “MIT · Open source” pill (no emoji clutter).
3. **Right two-thirds**: screenshot of the app in a subtle window chrome / soft shadow (`docs/media/app-main.png`).
4. Optional thin accent bar or cursor block in brand blue — no floating badge stickers.

## Copy (English)

- **Brand**: Terminus  
- **Headline**: SSH & SFTP, without the subscription tax  
- **Sub**: Fast desktop terminal — hosts, files, sync — open source (MIT)  
- **CTA line**: github.com/delikesance/terminus  

French alternate:

- **Headline**: Un terminal SSH & SFTP open source  
- **Sub**: Hosts, fichiers, sync — sans abonnement  

## Source files

| File | Role |
|------|------|
| `docs/media/app-main.png` | Primary product screenshot |
| `docs/media/app-hosts.png` | Optional crop / second slide |
| `docs/media/hero-preview.png` | Ready-made 16:9 hero (use as-is or as reference) |
| `src-tauri/icons/128x128.png` | App icon |

## Steps in Canva

1. Create design → **Custom size** 1920×1080.  
2. Uploads → add the `docs/media/*.png` files.  
3. Place `app-main.png` on the right; scale ~70–85% height.  
4. Add text layers with the copy above.  
5. Export PNG → replace or keep `docs/media/hero-preview.png` in the repo if you refine it.
