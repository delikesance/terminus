//! Navigation components: server row, section/group headers, drop target,
//! view tabs and session pills, plus their gallery (design board `CNavigation`).
//!
//! Geometry and state come from `terminus_ui::components::navigation`.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::identity::TileGlyph;
use terminus_ui::components::navigation::{
    drop_target, section_header, server_row, session_pill, view_tabs, MetaTone,
    PillState, RowMeta, RowState, TabSize, TabState,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::os_icons::OsGlyph;
use terminus_ui::theme::{text_color, ChromeTheme};
use terminus_ui::tokens::{font_size, space};

use crate::renderer::chrome::{draw_icon, paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

const DEPTH: f32 = 0.1;
const ORDER: u8 = 0;

fn u8_to_f32(c: [u8; 4]) -> [f32; 4] {
    c.map(|v| v as f32 / 255.0)
}

fn fill(sugarloaf: &mut Sugarloaf, r: &Rect, color: [f32; 4], radius: f32, depth: f32) {
    paint_surface_stroke(sugarloaf, r, color, None, radius, 0.0, depth, ORDER, false);
}

/// Top y that vertically centres a line of `size` text on `cy`.
fn text_top(cy: f32, size: f32) -> f32 {
    cy - size * 0.6
}

/// What a server-row tile shows (mapped onto the Identity tile).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TileKind {
    /// This computer.
    Local,
    /// A known OS / distro brand mark.
    Os(OsGlyph),
    /// Generic remote server.
    #[allow(dead_code)]
    Server,
}

/// Paint a server-row tile with the Identity component's tile (brand tint,
/// accent when the row is active).
pub fn paint_tile(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    kind: TileKind,
    active: bool,
    _device_scale: f32,
) {
    let glyph = match kind {
        TileKind::Local => TileGlyph::Local,
        TileKind::Os(g) => TileGlyph::from_os_glyph(g),
        TileKind::Server => TileGlyph::Unknown,
    };
    super::identity::paint_tile(sugarloaf, theme, rect, glyph, active);
}

/// Soft drop shadow (`0 10px 24px rgba(0,0,0,.45)` approximated by layers).
fn paint_shadow(sugarloaf: &mut Sugarloaf, r: &Rect, radius: f32) {
    for (i, a) in [0.10_f32, 0.09, 0.08, 0.07].iter().enumerate() {
        let grow = (i as f32) * 3.0;
        let s = Rect::new(
            r.x - grow,
            r.y + 6.0 - grow * 0.4,
            r.width + 2.0 * grow,
            r.height + 2.0 * grow,
        );
        fill(
            sugarloaf,
            &s,
            [0.0, 0.0, 0.0, *a],
            radius + grow,
            DEPTH - 0.05,
        );
    }
}

/// Paint one server row. The "Dragging" tilt is not supported by the
/// sugarloaf primitives and is omitted; raised bg + shadow are drawn.
#[allow(clippy::too_many_arguments)]
pub fn paint_server_row(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    row: &Rect,
    name: &str,
    kind: TileKind,
    meta: &RowMeta,
    state: RowState,
    device_scale: f32,
) {
    let st = server_row::style(theme, state);
    if st.ring {
        let (outer, inner) = server_row::focus_ring(row);
        let r = server_row::RADIUS;
        fill(sugarloaf, &outer, theme.accent, r + 4.0, DEPTH - 0.04);
        fill(sugarloaf, &inner, theme.canvas, r + 2.0, DEPTH - 0.03);
    }
    if st.shadow {
        paint_shadow(sugarloaf, row, server_row::RADIUS);
    }
    if let Some(bg) = st.bg {
        fill(sugarloaf, row, bg, server_row::RADIUS, DEPTH);
    }
    paint_tile(
        sugarloaf,
        theme,
        &server_row::tile_rect(row),
        kind,
        st.tile_active,
        device_scale,
    );
    let weight = if st.medium {
        UiWeight::Medium
    } else {
        UiWeight::Regular
    };
    let cy = row.y + row.height * 0.5;
    draw_ui_text(
        sugarloaf,
        server_row::name_x(row),
        text_top(cy, server_row::NAME_SIZE),
        name,
        server_row::NAME_SIZE,
        theme.text,
        weight,
    );
    if let Some(label) = meta.label() {
        let tone: MetaTone = meta.tone();
        let w =
            measure_ui_text(sugarloaf, &label, server_row::META_SIZE, UiWeight::Regular);
        draw_ui_text(
            sugarloaf,
            server_row::meta_right(row) - w,
            text_top(cy, server_row::META_SIZE),
            &label,
            server_row::META_SIZE,
            server_row::meta_color(theme, tone, state),
            UiWeight::Regular,
        );
    }
}

/// Section header: 12px faint label.
pub fn paint_section_header(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    header: &Rect,
    label: &str,
    action: Option<&str>,
) {
    let cy = header.y + header.height * 0.5;
    draw_ui_text(
        sugarloaf,
        header.x,
        text_top(cy, section_header::SIZE),
        label,
        section_header::SIZE,
        theme.text_faint,
        UiWeight::Regular,
    );
    if let Some(action) = action {
        let w =
            measure_ui_text(sugarloaf, action, section_header::SIZE, UiWeight::Regular);
        let a = section_header::action_rect(header, w);
        draw_ui_text(
            sugarloaf,
            a.x,
            text_top(cy, section_header::SIZE),
            action,
            section_header::SIZE,
            text_color(theme.accent),
            UiWeight::Regular,
        );
    }
}

/// Dashed rounded outline (local version of chrome.rs's private helper):
/// dashes are SDF capsules along the straight edges; corners are dotted arcs.
fn paint_dashed_outline(
    sugarloaf: &mut Sugarloaf,
    r: &Rect,
    radius: f32,
    color: [f32; 4],
    depth: f32,
) {
    let stroke = 1.5_f32;
    let (dash, gap) = (6.0_f32, 4.0_f32);
    let half = stroke * 0.5;
    let rr = (radius - half).max(0.0);
    let (x0, y0, x1, y1) = (r.x + half, r.y + half, r.right() - half, r.bottom() - half);
    let cap = |sg: &mut Sugarloaf, x: f32, y: f32, w: f32, h: f32| {
        fill(sg, &Rect::new(x, y, w, h), color, half, depth);
    };
    for (horizontal, fixed) in [(true, y0), (true, y1), (false, x0), (false, x1)] {
        let (a, b) = if horizontal {
            (x0 + rr, x1 - rr)
        } else {
            (y0 + rr, y1 - rr)
        };
        let n = (((b - a) + gap) / (dash + gap)).round().max(1.0);
        let d = ((b - a) - (n - 1.0) * gap) / n;
        for i in 0..n as usize {
            let s = a + i as f32 * (d + gap);
            if horizontal {
                cap(sugarloaf, s, fixed - half, d, stroke);
            } else {
                cap(sugarloaf, fixed - half, s, stroke, d);
            }
        }
    }
    if rr > 0.5 {
        for (cx, cy, start) in [
            (x0 + rr, y0 + rr, 180.0_f32),
            (x1 - rr, y0 + rr, 270.0),
            (x1 - rr, y1 - rr, 0.0),
            (x0 + rr, y1 - rr, 90.0),
        ] {
            for i in 0..=6 {
                let a = (start + 15.0 * i as f32).to_radians();
                let (px, py) = (cx + rr * a.cos(), cy + rr * a.sin());
                cap(sugarloaf, px - half, py - half, stroke, stroke);
            }
        }
    }
}

/// "Move to <group>" drop target.
pub fn paint_drop_target(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    label: &str,
) {
    fill(
        sugarloaf,
        rect,
        drop_target::fill(theme),
        drop_target::RADIUS,
        DEPTH,
    );
    paint_dashed_outline(
        sugarloaf,
        rect,
        drop_target::RADIUS,
        theme.accent,
        DEPTH + 0.01,
    );
    let w = measure_ui_text(sugarloaf, label, drop_target::LABEL_SIZE, UiWeight::Regular);
    draw_ui_text(
        sugarloaf,
        rect.x + (rect.width - w) * 0.5,
        text_top(rect.y + rect.height * 0.5, drop_target::LABEL_SIZE),
        label,
        drop_target::LABEL_SIZE,
        text_color(theme.accent),
        UiWeight::Regular,
    );
}

/// Measure a tab's label/badge so the caller can lay tabs out.
pub fn measure_tab(sugarloaf: &mut Sugarloaf, label: &str, badge: &str) -> TabSize {
    let medium = measure_ui_text(sugarloaf, label, view_tabs::SIZE, UiWeight::Medium);
    let regular = measure_ui_text(sugarloaf, label, view_tabs::SIZE, UiWeight::Regular);
    TabSize {
        // Reserve the wider (Medium) width so activating a tab never shifts its neighbours.
        label_w: medium.max(regular),
        badge_w: if badge.is_empty() {
            0.0
        } else {
            measure_ui_text(sugarloaf, badge, view_tabs::SIZE, UiWeight::Regular)
        },
    }
}

/// Paint a view tab inside `tab` (from [`view_tabs::layout`]).
pub fn paint_view_tab(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    tab: &Rect,
    label: &str,
    badge: &str,
    state: TabState,
) {
    let st = view_tabs::style(theme, state);
    let weight = if st.medium {
        UiWeight::Medium
    } else {
        UiWeight::Regular
    };
    // 20px text line sits at the top of the tab box.
    let ty = tab.y + 10.0 - view_tabs::SIZE * 0.6;
    let w = draw_ui_text(
        sugarloaf,
        tab.x,
        ty,
        label,
        view_tabs::SIZE,
        st.text,
        weight,
    );
    if !badge.is_empty() {
        draw_ui_text(
            sugarloaf,
            tab.x + w + view_tabs::BADGE_GAP,
            ty,
            badge,
            view_tabs::SIZE,
            text_color(theme.success),
            UiWeight::Regular,
        );
    }
    if let Some(color) = st.underline {
        paint_flat(
            sugarloaf,
            &view_tabs::underline_rect(tab),
            color,
            DEPTH,
            ORDER,
        );
    }
}

/// Paint a session pill inside `pill` (width from [`session_pill::width`]).
pub fn paint_session_pill(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    pill: &Rect,
    label: &str,
    state: PillState,
    device_scale: f32,
) {
    let st = session_pill::style(theme, state);
    if let Some(bg) = st.bg {
        fill(sugarloaf, pill, bg, session_pill::RADIUS, DEPTH);
    }
    if st.dot {
        fill(
            sugarloaf,
            &session_pill::dot_rect(pill),
            theme.accent,
            3.0,
            DEPTH + 0.01,
        );
    }
    draw_ui_text(
        sugarloaf,
        session_pill::text_x(pill, st.dot),
        text_top(pill.y + pill.height * 0.5, session_pill::SIZE),
        label,
        session_pill::SIZE,
        st.text,
        UiWeight::Regular,
    );
    if st.close {
        let i = session_pill::close_icon_rect(pill);
        draw_icon(
            sugarloaf,
            Icon::X,
            IconPlacement::new(i.x, i.y, i.width),
            u8_to_f32(theme.text_faint),
            device_scale,
        );
    }
}

fn caption(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, x: f32, y: f32, text: &str) {
    draw_ui_text(
        sugarloaf,
        x,
        y,
        text,
        font_size::CAPTION,
        theme.text_faint,
        UiWeight::Regular,
    );
}

fn subtitle(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    x: f32,
    y: f32,
    text: &str,
) -> f32 {
    draw_ui_text(
        sugarloaf,
        x,
        y,
        text,
        font_size::BODY,
        theme.text,
        UiWeight::SemiBold,
    );
    font_size::BODY + space::MD
}

/// Paint the `navigation` gallery at `origin` within `width`; returns the height used.
pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let scale = sugarloaf.scale_factor();
    let (x0, mut y) = origin;
    y += super::paint_section_title(sugarloaf, theme, origin, "Navigation");
    y += space::SM;

    // ---- Server row ----
    y += subtitle(sugarloaf, theme, x0, y, "Server row");
    let label_w = 120.0;
    let cols = 5.0;
    let col_gap = 20.0;
    let col_w = ((width - label_w - col_gap * cols) / cols).clamp(180.0, 260.0);
    let col_x = |i: usize| x0 + label_w + i as f32 * (col_w + col_gap);
    for (i, h) in ["Default", "Hover", "Selected", "Focus", "Dragging"]
        .iter()
        .enumerate()
    {
        caption(sugarloaf, theme, col_x(i), y, h);
    }
    y += font_size::CAPTION + space::MD;
    let states = [
        RowState::Default,
        RowState::Hover,
        RowState::Selected,
        RowState::Focus,
        RowState::Dragging,
    ];
    let ubuntu = TileKind::Os(OsGlyph::from_hint(Some("ubuntu"), "Ubuntu"));
    let nixos = TileKind::Os(OsGlyph::from_hint(Some("nixos"), "NixOS"));
    let kinds: [(&str, &str, TileKind, RowMeta); 4] = [
        ("SSH host", "jerem prod", ubuntu, RowMeta::None),
        ("WSL running", "NixOS", nixos, RowMeta::Running),
        (
            "Open sessions",
            "Local",
            TileKind::Local,
            RowMeta::Sessions(2),
        ),
        ("WSL stopped", "Ubuntu 24.04 LTS", ubuntu, RowMeta::None),
    ];
    for (cap, name, kind, meta) in &kinds {
        caption(sugarloaf, theme, x0, y + 14.0, cap);
        for (i, st) in states.iter().enumerate() {
            let row = Rect::new(col_x(i), y, col_w, server_row::HEIGHT);
            paint_server_row(sugarloaf, theme, &row, name, *kind, meta, *st, scale);
        }
        y += server_row::HEIGHT + space::XL;
    }
    caption(
        sugarloaf,
        theme,
        x0,
        y - space::SM,
        "Selected = the machine whose workspace is open. Dragging tilt is omitted (no rotation in sugarloaf).",
    );
    y += space::XL;

    // ---- Section header and drop target ----
    y += subtitle(sugarloaf, theme, x0, y, "Section header and drop target");
    let w3 = 248.0;
    let gx = |i: usize| x0 + label_w + i as f32 * (w3 + 40.0);
    for (i, h) in ["Section", "Group with action", "Drop into group"]
        .iter()
        .enumerate()
    {
        caption(sugarloaf, theme, gx(i), y, h);
    }
    y += font_size::CAPTION + space::MD;
    paint_section_header(
        sugarloaf,
        theme,
        &Rect::new(gx(0), y + 12.0, w3, section_header::HEIGHT),
        "This computer",
        None,
    );
    paint_section_header(
        sugarloaf,
        theme,
        &Rect::new(gx(1), y + 12.0, w3, section_header::HEIGHT),
        "jeremy",
        Some("New group"),
    );
    paint_drop_target(
        sugarloaf,
        theme,
        &drop_target::rect(gx(2), y, w3),
        "Move to jeremy",
    );
    y += drop_target::HEIGHT + space::XL;

    // ---- View tabs ----
    y += subtitle(sugarloaf, theme, x0, y, "View tabs");
    let tab_specs: [(&str, &str, &str, TabState); 5] = [
        ("Default", "Files", "", TabState::Default),
        ("Hover", "Files", "", TabState::Hover),
        ("Active", "Terminal", "", TabState::Active),
        ("With badge", "Tunnels", "1", TabState::Default),
        ("Active + badge", "Tunnels", "1", TabState::Active),
    ];
    for (i, (cap, ..)) in tab_specs.iter().enumerate() {
        caption(sugarloaf, theme, col_x(i), y, cap);
    }
    y += font_size::CAPTION + space::MD;
    caption(sugarloaf, theme, x0, y + 8.0, "Underline tab");
    for (i, (_, label, badge, st)) in tab_specs.iter().enumerate() {
        let size = measure_tab(sugarloaf, label, badge);
        let rect = view_tabs::layout(col_x(i), y, &[size])[0];
        paint_view_tab(sugarloaf, theme, &rect, label, badge, *st);
    }
    y += view_tabs::HEIGHT + space::XL;

    // ---- Session pills ----
    y += subtitle(sugarloaf, theme, x0, y, "Session pills");
    let pill_specs: [(&str, &str, PillState); 4] = [
        ("Default", "logs", PillState::Default),
        ("Hover", "logs", PillState::Hover),
        ("Active", "~/app", PillState::Active),
        ("New output", "build", PillState::NewOutput),
    ];
    for (i, (cap, ..)) in pill_specs.iter().enumerate() {
        caption(sugarloaf, theme, col_x(i), y, cap);
    }
    y += font_size::CAPTION + space::MD;
    caption(sugarloaf, theme, x0, y + 8.0, "Pill");
    for (i, (_, label, st)) in pill_specs.iter().enumerate() {
        let style = session_pill::style(theme, *st);
        let lw = measure_ui_text(sugarloaf, label, session_pill::SIZE, UiWeight::Regular);
        let w = session_pill::width(lw, style.close, style.dot);
        let rect = Rect::new(col_x(i), y, w, session_pill::HEIGHT);
        paint_session_pill(sugarloaf, theme, &rect, label, *st, scale);
    }
    y += session_pill::HEIGHT + space::MD;
    caption(
        sugarloaf,
        theme,
        x0,
        y,
        "One per terminal session. The accent dot marks output in a session you are not looking at.",
    );
    y += font_size::CAPTION;
    y - origin.1
}
