//! Sidebar: brand, command bar, machine list (Navigation server rows and
//! section headers), Add server and Settings.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::components::navigation::{
    section_header, server_row, RowMeta, RowState,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::os_icons::{HostStatus, OsGlyph};
use terminus_ui::shell::sidebar as geo;
use terminus_ui::shell::ShellHit;
use terminus_ui::sidebar::{self, Badge, HostDropTarget, Row};
use terminus_ui::theme::{text_color, ChromeTheme};

use super::{fill, text_top, u8_to_f32};
use crate::renderer::chrome::{draw_icon, draw_orbit_indicator};
use crate::renderer::components::navigation::{
    paint_drop_target, paint_section_header, paint_server_row, TileKind,
};
use crate::renderer::ui_text::{
    draw_mono_text, draw_ui_text, measure_mono_text, measure_ui_text, UiWeight,
};

pub(super) fn paint(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    device_scale: f32,
    connecting_phase: Option<f32>,
) {
    let h = chrome.shell.window.1;
    let hover = chrome.shell.hover;

    // Brand.
    let brand = geo::brand_rect();
    draw_ui_text(
        sugarloaf,
        brand.x + geo::BRAND_INSET,
        text_top(brand.y + brand.height * 0.5, geo::BRAND_SIZE),
        "Terminus",
        geo::BRAND_SIZE,
        theme.text,
        UiWeight::SemiBold,
    );

    // Command bar.
    let bar = geo::command_bar_rect();
    let bar_bg = if hover == Some(ShellHit::CommandBar) {
        theme.raised
    } else {
        theme.surface
    };
    fill(sugarloaf, &bar, bar_bg, geo::COMMAND_RADIUS);
    let icon = geo::lead_icon_rect(&bar, geo::COMMAND_PAD_X, geo::COMMAND_ICON);
    draw_icon(
        sugarloaf,
        Icon::Search,
        IconPlacement::new(icon.x, icon.y, icon.width),
        u8_to_f32(theme.text_muted),
        device_scale,
    );
    let cy = bar.y + bar.height * 0.5;
    // Hint first: the label is elided to the room it leaves.
    let hint = geo::palette_hint(
        chrome.shell.view().shows_terminal(),
        cfg!(target_os = "macos"),
    );
    let widths: Vec<f32> = hint
        .iter()
        .map(|t| match t {
            geo::HintToken::Text(s) => {
                measure_mono_text(sugarloaf, s, geo::COMMAND_HINT_SIZE, UiWeight::Regular)
            }
            geo::HintToken::Glyph(_) => geo::HINT_ICON,
        })
        .collect();
    let layout = geo::hint_layout(&bar, &widths);
    for (token, x) in hint.iter().zip(&layout.xs) {
        match *token {
            geo::HintToken::Text(s) => {
                draw_mono_text(
                    sugarloaf,
                    *x,
                    text_top(cy, geo::COMMAND_HINT_SIZE),
                    s,
                    geo::COMMAND_HINT_SIZE,
                    theme.text_muted,
                    UiWeight::Regular,
                );
            }
            geo::HintToken::Glyph(glyph) => draw_icon(
                sugarloaf,
                glyph,
                IconPlacement::new(*x, cy - geo::HINT_ICON * 0.5, geo::HINT_ICON),
                u8_to_f32(theme.text_muted),
                device_scale,
            ),
        }
    }
    let label = elide_ui(
        sugarloaf,
        "Search or run…",
        geo::COMMAND_SIZE,
        geo::command_label_max_width(&bar, layout.left),
    );
    draw_ui_text(
        sugarloaf,
        geo::command_label_x(&bar),
        text_top(cy, geo::COMMAND_SIZE),
        &label,
        geo::COMMAND_SIZE,
        theme.text_muted,
        UiWeight::Regular,
    );

    paint_list(sugarloaf, chrome, theme, device_scale, connecting_phase);

    // Add server (Text button).
    let add = geo::add_server_rect(h);
    if hover == Some(ShellHit::AddServer) {
        fill(sugarloaf, &add, theme.surface, geo::FOOT_RADIUS);
    }
    let icon = geo::lead_icon_rect(&add, geo::FOOT_PAD_X, geo::FOOT_ICON);
    draw_icon(
        sugarloaf,
        Icon::Plus,
        IconPlacement::new(icon.x, icon.y, icon.width),
        theme.accent,
        device_scale,
    );
    let cy = add.y + add.height * 0.5;
    draw_ui_text(
        sugarloaf,
        icon.right() + geo::FOOT_ICON_GAP,
        text_top(cy, geo::FOOT_SIZE),
        "Add server",
        geo::FOOT_SIZE,
        text_color(theme.accent),
        UiWeight::Medium,
    );

    // Settings (+ "Synced").
    let set = geo::settings_rect(h);
    let on_settings = chrome.shell.view().is_settings();
    if on_settings {
        fill(sugarloaf, &set, theme.selected, geo::FOOT_RADIUS);
    } else if hover == Some(ShellHit::Settings) {
        fill(sugarloaf, &set, theme.surface, geo::FOOT_RADIUS);
    }
    let fg = if on_settings {
        theme.text
    } else {
        theme.text_muted
    };
    let icon = geo::lead_icon_rect(&set, geo::FOOT_PAD_X, geo::FOOT_ICON);
    draw_icon(
        sugarloaf,
        Icon::Settings,
        IconPlacement::new(icon.x, icon.y, icon.width),
        u8_to_f32(fg),
        device_scale,
    );
    let cy = set.y + set.height * 0.5;
    draw_ui_text(
        sugarloaf,
        icon.right() + geo::FOOT_ICON_GAP,
        text_top(cy, geo::FOOT_SIZE),
        "Settings",
        geo::FOOT_SIZE,
        fg,
        UiWeight::Regular,
    );
    if chrome.shell.sync_ok {
        let w = measure_ui_text(sugarloaf, "Synced", geo::SYNCED_SIZE, UiWeight::Regular);
        draw_ui_text(
            sugarloaf,
            set.right() - geo::FOOT_PAD_X - w,
            text_top(cy, geo::SYNCED_SIZE),
            "Synced",
            geo::SYNCED_SIZE,
            text_color(theme.success),
            UiWeight::Regular,
        );
    }
}

fn tile_kind(host: &sidebar::HostItem) -> TileKind {
    match host.badge {
        Badge::Local => TileKind::Local,
        _ => TileKind::Os(OsGlyph::from_hint(host.os_id.as_deref(), &host.name)),
    }
}

/// Right-hand meta of a machine row: open sessions, else "Running" for a
/// running WSL distro.
fn row_meta(host: &sidebar::HostItem) -> RowMeta {
    if host.session_count > 0 {
        RowMeta::Sessions(host.session_count as u32)
    } else if host.badge == Badge::Wsl && host.status == HostStatus::Running {
        RowMeta::Running
    } else {
        RowMeta::None
    }
}

fn paint_list(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    device_scale: f32,
    connecting_phase: Option<f32>,
) {
    let panel = &chrome.panel;
    let origin_y = chrome.origin_y();
    let height = chrome.shell.window.1;
    let body = panel.body_rect(origin_y, height);
    let machine_view = chrome.shell.view().is_machine_view();
    let selected_id = chrome.shell.machine.as_ref().map(|m| m.id.as_str());
    let drag = panel.host_drag.as_ref().filter(|d| d.started());

    if panel.filter_visible() {
        crate::renderer::chrome::paint_search_field(
            sugarloaf,
            chrome,
            theme,
            origin_y,
            device_scale,
            0.06,
        );
    }

    for index in panel.visible_row_indices() {
        let card = panel.card_rect(origin_y, index);
        // UI text cannot be clipped: rows half outside the viewport are
        // skipped rather than drawn over the command bar or footer.
        if card.y < body.y - 0.5 || card.bottom() > body.bottom() + 0.5 {
            continue;
        }
        match &panel.rows[index] {
            Row::Section(label) => {
                let action =
                    (panel.hosts_section_index() == Some(index)).then_some("New group");
                paint_section_header(sugarloaf, theme, &label_box(&card), label, action);
                if panel.hosts_section_index() == Some(index) {
                    crate::renderer::chrome::paint_new_group_form(
                        sugarloaf, chrome, theme, origin_y, true, &body,
                    );
                }
            }
            Row::Group {
                id,
                name,
                host_count,
                collapsed,
                ..
            } => {
                let collapsed = *collapsed || panel.collapsed_groups.contains(id);
                let renaming = panel.is_renaming(id);
                if !renaming {
                    paint_section_header(sugarloaf, theme, &label_box(&card), name, None);
                }
                let b = label_box(&card);
                let chevron = 12.0;
                let ix = b.right() - chevron;
                let iy = b.y + (b.height - chevron) * 0.5;
                if collapsed && *host_count > 0 {
                    let n = host_count.to_string();
                    let w = measure_ui_text(
                        sugarloaf,
                        &n,
                        section_header::SIZE,
                        UiWeight::Regular,
                    );
                    draw_ui_text(
                        sugarloaf,
                        ix - 6.0 - w,
                        text_top(b.y + b.height * 0.5, section_header::SIZE),
                        &n,
                        section_header::SIZE,
                        theme.text_faint,
                        UiWeight::Regular,
                    );
                }
                let hovered = panel.hover == Some(index);
                draw_icon(
                    sugarloaf,
                    if collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    },
                    IconPlacement::new(ix, iy, chevron),
                    u8_to_f32(if hovered {
                        theme.text
                    } else {
                        theme.text_faint
                    }),
                    device_scale,
                );
                if renaming {
                    paint_rename(sugarloaf, chrome, theme, &b, b.x);
                }
                // "Move to <group>" while a host hovers it.
                if let Some(HostDropTarget::Group(target)) =
                    drag.and_then(|d| d.drop_target.as_ref())
                {
                    if target == id {
                        if let Some(slot) = panel
                            .drop_slot_rect(origin_y, &HostDropTarget::Group(id.clone()))
                        {
                            if slot.bottom() <= body.bottom() {
                                paint_drop_target(
                                    sugarloaf,
                                    theme,
                                    &slot,
                                    &format!("Move to {name}"),
                                );
                            }
                        }
                    }
                }
            }
            Row::Host(host) => {
                let dragging_this = drag.is_some_and(|d| d.host_id == host.id);
                let state = if dragging_this {
                    RowState::Default
                } else if machine_view && selected_id == Some(host.id.as_str()) {
                    RowState::Selected
                } else if panel.hover == Some(index) {
                    RowState::Hover
                } else {
                    RowState::Default
                };
                let connecting = panel.is_connecting(&host.id);
                let meta = if connecting {
                    RowMeta::None
                } else {
                    row_meta(host)
                };
                let renaming = panel.is_renaming(&host.id);
                paint_server_row(
                    sugarloaf,
                    theme,
                    &card,
                    if renaming { "" } else { &host.name },
                    tile_kind(host),
                    &meta,
                    state,
                    device_scale,
                );
                if renaming {
                    let x = server_row::name_x(&card);
                    let field = Rect::new(
                        x - 6.0,
                        card.y + 8.0,
                        card.right() - x - 2.0,
                        card.height - 16.0,
                    );
                    paint_rename(sugarloaf, chrome, theme, &field, x);
                }
                if connecting {
                    if let Some(phase) = connecting_phase {
                        let cx = server_row::meta_right(&card) - 8.0;
                        let cy = card.y + card.height * 0.5;
                        draw_orbit_indicator(sugarloaf, theme, cx, cy, 5.5, 2.0, phase);
                    }
                }
            }
            // Sessions are pills now; legacy session rows are not drawn.
            Row::Session(_) => {}
        }
    }

    crate::renderer::chrome::render_empty_hint(
        sugarloaf, chrome, theme, origin_y, height, None,
    );
    crate::renderer::chrome::render_notice(
        sugarloaf, chrome, theme, origin_y, height, true,
    );
}

/// The 15px label line of a header card, inset like the mock (0 10px).
fn label_box(card: &Rect) -> Rect {
    Rect::new(card.x + 10.0, card.y, card.width - 20.0, card.height - 6.0)
}

/// Inline rename: a field-coloured box with the draft text and caret.
fn paint_rename(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    field: &Rect,
    text_x: f32,
) {
    let Some(draft) = chrome.panel.rename.as_ref() else {
        return;
    };
    sugarloaf.rounded_rect(
        None,
        field.x,
        field.y,
        field.width,
        field.height,
        theme.field,
        0.05,
        6.0,
        3,
    );
    let ty = field.y + (field.height - sidebar::ROW_TITLE_FONT_SIZE) * 0.5;
    crate::renderer::chrome::paint_rename_text(sugarloaf, draft, text_x, ty, theme);
}

/// `text` in Sora Regular `size`, cut with an ellipsis to fit `max_w`.
fn elide_ui(sugarloaf: &mut Sugarloaf, text: &str, size: f32, max_w: f32) -> String {
    if measure_ui_text(sugarloaf, text, size, UiWeight::Regular) <= max_w + 0.5 {
        return text.to_string();
    }
    let chars: Vec<char> = text.trim_end_matches('…').chars().collect();
    for n in (0..chars.len()).rev() {
        let s: String =
            chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
        if measure_ui_text(sugarloaf, &s, size, UiWeight::Regular) <= max_w {
            return s;
        }
    }
    String::new()
}
