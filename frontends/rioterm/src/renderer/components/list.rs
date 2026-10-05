//! List component painter and gallery: list cards, file rows, history rows.
//!
//! Geometry comes from `terminus_ui::components::list`; this file only
//! paints it. Card and history actions are real Button components.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonSpec, ButtonState};
use terminus_ui::components::list::*;
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, space};

use super::button::{label_spec, paint_button};
use crate::renderer::chrome::{draw_icon, paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{draw_mono_text, draw_ui_text, measure_ui_text, UiWeight};

// Depth layers (later = on top) within one gallery section.
const D_PANEL: f32 = 0.0;
const D_ROW: f32 = 0.1;
const D_CTRL: f32 = 0.2;

/// Vertical text origin so a line of `size` is centred on `center`.
fn text_top(center: f32, size: f32) -> f32 {
    center - size * 0.62
}

/// `fg` over opaque `bg` at `alpha`.
fn blend(bg: [f32; 4], fg: [f32; 4], alpha: f32) -> [f32; 4] {
    [
        bg[0] + (fg[0] - bg[0]) * alpha,
        bg[1] + (fg[1] - bg[1]) * alpha,
        bg[2] + (fg[2] - bg[2]) * alpha,
        1.0,
    ]
}

fn rrect(s: &mut Sugarloaf, r: &Rect, color: [f32; 4], rad: f32, depth: f32) {
    s.rounded_rect(None, r.x, r.y, r.width, r.height, color, depth, rad, 0);
}

fn ring(
    s: &mut Sugarloaf,
    r: &Rect,
    fill: [f32; 4],
    stroke: [f32; 4],
    rad: f32,
    w: f32,
    depth: f32,
) {
    paint_surface_stroke(s, r, fill, Some(stroke), rad, w, depth, 0, false);
}

// ---------------------------------------------------------------------
// Status dot
// ---------------------------------------------------------------------

/// Leading dot of a card.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DotKind {
    Running,
    Idle,
}

pub fn paint_dot(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    r: &Rect,
    kind: DotKind,
    bg: [f32; 4],
) {
    match kind {
        DotKind::Running => rrect(s, r, theme.success, CARD_DOT / 2.0, D_CTRL),
        // Hollow ring in the idle colour.
        DotKind::Idle => ring(s, r, bg, theme.idle_ring, CARD_DOT / 2.0, 1.5, D_CTRL),
    }
}

// ---------------------------------------------------------------------
// Action buttons (Button component, medium)
// ---------------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Action<'a> {
    Secondary(&'a str),
    Ghost(&'a str),
    /// Icon-only quiet button with a trash glyph.
    QuietTrash,
}

impl Action<'_> {
    fn kind(&self) -> ButtonKind {
        match self {
            Action::Secondary(_) => ButtonKind::Secondary,
            Action::Ghost(_) => ButtonKind::Text,
            Action::QuietTrash => ButtonKind::Quiet,
        }
    }

    fn label(&self) -> &str {
        match self {
            Action::Secondary(l) | Action::Ghost(l) => l,
            Action::QuietTrash => "Delete",
        }
    }

    fn icon(&self) -> Option<Icon> {
        matches!(self, Action::QuietTrash).then_some(Icon::Trash2)
    }

    fn spec(&self, s: &mut Sugarloaf, origin: (f32, f32)) -> ButtonSpec {
        match self {
            Action::QuietTrash => {
                ButtonSpec::icon_only(origin, self.kind(), ButtonSize::Medium)
            }
            _ => label_spec(
                s,
                origin,
                self.kind(),
                ButtonSize::Medium,
                self.label(),
                false,
            ),
        }
    }

    fn width(&self, s: &mut Sugarloaf) -> f32 {
        self.spec(s, (0.0, 0.0)).width()
    }
}

fn paint_action(s: &mut Sugarloaf, theme: &ChromeTheme, r: &Rect, a: &Action) {
    let spec = a.spec(s, (r.x, r.y));
    paint_button(s, theme, &spec, ButtonState::Default, a.label(), a.icon());
}

// ---------------------------------------------------------------------
// List card
// ---------------------------------------------------------------------

pub struct CardContent<'a> {
    pub title: &'a str,
    pub detail: &'a str,
    pub meta: &'a str,
    pub dot: Option<DotKind>,
    pub actions: &'a [Action<'a>],
}

/// Paint one card at `rect` (use [`CARD_HEIGHT`] for the height) and return
/// its layout, so callers can hit-test the same rects.
pub fn paint_card(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: Rect,
    state: CardState,
    content: &CardContent,
) -> CardLayout {
    let bg = state.background(theme);
    rrect(s, &rect, bg, CARD_RADIUS, D_ROW);
    let meta_w = measure_ui_text(s, content.meta, font_size::CAPTION, UiWeight::Regular);
    let widths: Vec<f32> = content.actions.iter().map(|a| a.width(s)).collect();
    let spec = CardSpec {
        has_dot: content.dot.is_some(),
        meta_width: meta_w,
        action_widths: &widths,
    };
    let l = card_layout(rect, &spec);
    if let (Some(d), Some(kind)) = (l.dot, content.dot) {
        paint_dot(s, theme, &d, kind, bg);
    }
    draw_ui_text(
        s,
        l.text.x,
        l.text.y,
        content.title,
        font_size::BODY,
        theme.text,
        UiWeight::Medium,
    );
    draw_mono_text(
        s,
        l.text.x,
        l.text.y + 19.0 + 4.0,
        content.detail,
        11.0,
        theme.text_muted,
        UiWeight::Regular,
    );
    draw_ui_text(
        s,
        l.meta.x,
        text_top(rect.y + rect.height / 2.0, font_size::CAPTION),
        content.meta,
        font_size::CAPTION,
        theme.text_muted,
        UiWeight::Regular,
    );
    for (slot, action) in l.actions.iter().zip(content.actions) {
        if let Some(r) = slot {
            paint_action(s, theme, r, action);
        }
    }
    l
}

// ---------------------------------------------------------------------
// File row
// ---------------------------------------------------------------------

/// `under` is the opaque colour behind the row (used to flatten the
/// translucent drop fill and to fill outlined glyphs).
pub fn paint_file_row(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: Rect,
    under: [f32; 4],
    state: FileRowState,
    kind: FileKind,
    name: &str,
    size: &str,
    date: &str,
) -> FileRowLayout {
    let l = file_row_layout(rect);
    let fill = match state.background(theme) {
        Some(c) if c[3] < 1.0 => blend(under, c, c[3]),
        Some(c) => c,
        None => under,
    };
    match state.inset_stroke() {
        Some(w) => ring(s, &rect, fill, theme.accent, FILE_ROW_RADIUS, w, D_ROW),
        None => {
            if state.background(theme).is_some() {
                rrect(s, &rect, fill, FILE_ROW_RADIUS, D_ROW);
            }
        }
    }
    match kind {
        FileKind::Folder => draw_icon(
            s,
            Icon::Folder,
            IconPlacement::new(l.icon.x, l.icon.y, FILE_ICON_SIZE),
            theme.info,
            s.scale_factor(),
        ),
        FileKind::File => draw_icon(
            s,
            Icon::File,
            IconPlacement::new(l.icon.x, l.icon.y, FILE_ICON_SIZE),
            terminus_ui::theme::unit_color(theme.text_faint),
            s.scale_factor(),
        ),
    }
    let cy = rect.y + rect.height / 2.0;
    let ty = text_top(cy, font_size::LABEL);
    if state.shows_rename_field() {
        ring(
            s,
            &l.rename_field,
            theme.field,
            theme.accent,
            FILE_RENAME_RADIUS,
            1.0,
            D_CTRL,
        );
        draw_ui_text(
            s,
            l.rename_field.x + 8.0,
            ty,
            name,
            font_size::LABEL,
            theme.text,
            UiWeight::Regular,
        );
    } else {
        draw_ui_text(
            s,
            l.name.x,
            ty,
            name,
            font_size::LABEL,
            theme.text,
            UiWeight::Regular,
        );
    }
    for (col, text) in [(&l.size, size), (&l.date, date)] {
        let w = measure_ui_text(s, text, font_size::LABEL, UiWeight::Regular);
        draw_ui_text(
            s,
            col.right() - w,
            ty,
            text,
            font_size::LABEL,
            theme.text_muted,
            UiWeight::Regular,
        );
    }
    l
}

// ---------------------------------------------------------------------
// History row
// ---------------------------------------------------------------------

pub fn paint_history_row(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: Rect,
    command: &str,
    cwd: &str,
    when: &str,
    action: Option<&Action>,
) -> HistoryRowLayout {
    let cwd_w =
        crate::renderer::ui_text::measure_mono_text(s, cwd, 11.0, UiWeight::Regular);
    let act_w = action.map_or(0.0, |a| a.width(s));
    let l = history_row_layout(rect, cwd_w, act_w);
    let cy = rect.y + rect.height / 2.0;
    draw_mono_text(
        s,
        l.command.x,
        text_top(cy, 12.0),
        command,
        12.0,
        theme.text,
        UiWeight::Regular,
    );
    draw_mono_text(
        s,
        l.cwd.x,
        text_top(cy, 11.0),
        cwd,
        11.0,
        theme.text_faint,
        UiWeight::Regular,
    );
    let w = measure_ui_text(s, when, font_size::CAPTION, UiWeight::Regular);
    draw_ui_text(
        s,
        l.time.right() - w,
        text_top(cy, font_size::CAPTION),
        when,
        font_size::CAPTION,
        theme.text_muted,
        UiWeight::Regular,
    );
    if let (Some(r), Some(a)) = (l.action, action) {
        paint_action(s, theme, &r, a);
    }
    paint_flat(s, &l.divider, theme.divider, D_ROW, 0);
    l
}

// ---------------------------------------------------------------------
// Gallery
// ---------------------------------------------------------------------

const PANEL_PAD: f32 = 28.0;
const PANEL_RADIUS: f32 = 16.0;

fn caption(s: &mut Sugarloaf, theme: &ChromeTheme, x: f32, y: f32, text: &str) -> f32 {
    draw_ui_text(
        s,
        x,
        y,
        text,
        font_size::CAPTION,
        theme.text_faint,
        UiWeight::Regular,
    );
    font_size::CAPTION + 3.0 + 10.0
}

/// Panel header (title + note); returns its height incl. the 20 px gap.
fn panel_header(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    x: f32,
    y: f32,
    title: &str,
    note: &str,
) -> f32 {
    draw_ui_text(s, x, y, title, 18.0, theme.text, UiWeight::SemiBold);
    let mut h = 23.0;
    if !note.is_empty() {
        h += 6.0;
        draw_ui_text(
            s,
            x,
            y + h,
            note,
            font_size::LABEL,
            theme.text_muted,
            UiWeight::Regular,
        );
        h += 20.0;
    }
    h + 20.0
}

pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let s = sugarloaf;
    let mut y =
        origin.1 + super::paint_section_title(s, theme, origin, "List") + space::MD;
    let inner_w = (width - 2.0 * PANEL_PAD).max(0.0);
    let x0 = origin.0 + PANEL_PAD;

    // ---- List card ----
    let cards: [(&str, CardState, CardContent); 5] = [
        (
            "Tunnel · running",
            CardState::Default,
            CardContent {
                title: "[Tunnel name]",
                detail: "localhost:5432 → localhost:5432",
                meta: "Local",
                dot: Some(DotKind::Running),
                actions: &[Action::Secondary("Stop")],
            },
        ),
        (
            "Tunnel · stopped, hover",
            CardState::Hover,
            CardContent {
                title: "[Tunnel name]",
                detail: "localhost:8080 → localhost:80",
                meta: "Local",
                dot: Some(DotKind::Idle),
                actions: &[Action::Secondary("Start")],
            },
        ),
        (
            "Snippet",
            CardState::Default,
            CardContent {
                title: "[Snippet title]",
                detail: "sudo systemctl restart nginx",
                meta: "nginx",
                dot: None,
                actions: &[Action::Secondary("Paste"), Action::Ghost("Run")],
            },
        ),
        (
            "SSH key",
            CardState::Default,
            CardContent {
                title: "id_ed25519",
                detail: "SHA256:[fingerprint]",
                meta: "[date]",
                dot: None,
                actions: &[Action::Secondary("Copy public key"), Action::QuietTrash],
            },
        ),
        (
            "Status row",
            CardState::Default,
            CardContent {
                title: "Synced",
                detail: "Last change pulled from the shared database",
                meta: "",
                dot: Some(DotKind::Running),
                actions: &[Action::Secondary("Sync now")],
            },
        ),
    ];
    let note = "The same card for every list: leading status dot when the thing can run, title, a mono detail line, meta, then actions.";
    let panel1_h = PANEL_PAD * 2.0
        + panel_header_height(true)
        + 5.0 * (cap_h() + CARD_HEIGHT)
        + 4.0 * 14.0;
    rrect(
        s,
        &Rect::new(origin.0, y, width, panel1_h),
        theme.canvas,
        PANEL_RADIUS,
        D_PANEL,
    );
    let mut cy = y + PANEL_PAD;
    cy += panel_header(s, theme, x0, cy, "List card", note);
    for (i, (label, state, content)) in cards.iter().enumerate() {
        if i > 0 {
            cy += 14.0;
        }
        cy += caption(s, theme, x0, cy, label);
        paint_card(
            s,
            theme,
            Rect::new(x0, cy, inner_w, CARD_HEIGHT),
            *state,
            content,
        );
        cy += CARD_HEIGHT;
    }
    y += panel1_h + space::LG;

    // ---- File row ----
    let col_gap = 28.0;
    let col_w = ((inner_w - col_gap) / 2.0).max(0.0);
    let files = [
        (
            "Folder",
            FileRowState::Default,
            FileKind::Folder,
            "releases",
            "—",
            "Today",
        ),
        (
            "File · hover",
            FileRowState::Hover,
            FileKind::File,
            "docker-compose.yml",
            "2 KB",
            "Mon",
        ),
        (
            "Selected",
            FileRowState::Selected,
            FileKind::File,
            "build.tar.gz",
            "18 MB",
            "Today",
        ),
        (
            "Drop target",
            FileRowState::DropTarget,
            FileKind::Folder,
            "releases",
            "—",
            "Today",
        ),
        (
            "Renaming",
            FileRowState::Renaming,
            FileKind::File,
            "notes-2.md",
            "4 KB",
            "Mon",
        ),
    ];
    let cell_h = cap_h() + FILE_ROW_HEIGHT;
    let grid_h = 3.0 * cell_h + 2.0 * 18.0;
    let panel2_h = PANEL_PAD * 2.0 + panel_header_height(false) + grid_h;
    rrect(
        s,
        &Rect::new(origin.0, y, width, panel2_h),
        theme.canvas,
        PANEL_RADIUS,
        D_PANEL,
    );
    let gy = y + PANEL_PAD + panel_header(s, theme, x0, y + PANEL_PAD, "File row", "");
    for (i, (label, state, kind, name, size, date)) in files.iter().enumerate() {
        let (col, row) = (i % 2, i / 2);
        let cx = x0 + col as f32 * (col_w + col_gap);
        let cy = gy + row as f32 * (cell_h + 18.0);
        let h = caption(s, theme, cx, cy, label);
        paint_file_row(
            s,
            theme,
            Rect::new(cx, cy + h, col_w, FILE_ROW_HEIGHT),
            theme.canvas,
            *state,
            *kind,
            name,
            size,
            date,
        );
    }
    y += panel2_h + space::LG;

    // ---- History row ----
    let panel3_h = PANEL_PAD * 2.0 + panel_header_height(false) + HISTORY_ROW_HEIGHT;
    rrect(
        s,
        &Rect::new(origin.0, y, width, panel3_h),
        theme.canvas,
        PANEL_RADIUS,
        D_PANEL,
    );
    let hy = y + PANEL_PAD + panel_header(s, theme, x0, y + PANEL_PAD, "History row", "");
    paint_history_row(
        s,
        theme,
        Rect::new(x0, hy, inner_w, HISTORY_ROW_HEIGHT),
        "docker compose up -d",
        "~/app",
        "5 min ago",
        Some(&Action::Secondary("Run again")),
    );
    y += panel3_h;

    y - origin.1
}

fn cap_h() -> f32 {
    font_size::CAPTION + 3.0 + 10.0
}

fn panel_header_height(with_note: bool) -> f32 {
    23.0 + if with_note { 26.0 } else { 0.0 } + 20.0
}
