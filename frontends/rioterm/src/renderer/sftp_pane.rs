//! Dual-pane SFTP browser painter (Sugarloaf) — Close + name edit toolbar.

use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::sftp_pane::{
    SftpFocus, SftpHit, SftpPaneLayout, SftpPaneState, SftpRow, SftpSideState, BTN_SIZE,
    PANE_PAD, ROW_HEIGHT,
};
use terminus_ui::{ChromeTheme, Rect};

use super::chrome::{
    draw_icon, paint_field_card_at, paint_field_caret_prefix_at, paint_flat,
};

const DEPTH: f32 = 0.05;
const ORDER: u8 = 8;
const ICON_IN_BTN: f32 = 16.0;

/// Paint the dual-pane SFTP UI into `bounds` (logical pixels).
///
/// Always paints quads + glyphs. Overlay dialogs (Edit Host, Settings, …)
/// must use `Sugarloaf::begin_overlay` so they composite *after* this
/// underlay text — never suppress SFTP glyphs for stacking.
pub fn paint(
    sugarloaf: &mut Sugarloaf,
    state: &SftpPaneState,
    bounds: Rect,
    theme: &ChromeTheme,
) {
    let layout = SftpPaneLayout::from_state(bounds, state);
    let scale = sugarloaf.scale_factor().max(1.0);

    paint_flat(sugarloaf, &layout.bounds, theme.panel_bg, DEPTH, ORDER);
    paint_toolbar(sugarloaf, state, &layout, theme, scale);
    paint_side(
        sugarloaf,
        state,
        theme,
        scale,
        &layout.left,
        &layout.left_header,
        &layout.left_parent_btn,
        &layout.left_list,
        &state.left,
        state.focus == SftpFocus::Left,
        true,
    );
    paint_side(
        sugarloaf,
        state,
        theme,
        scale,
        &layout.right,
        &layout.right_header,
        &layout.right_parent_btn,
        &layout.right_list,
        &state.right,
        state.focus == SftpFocus::Right,
        false,
    );

    paint_flat(sugarloaf, &layout.footer, theme.button_bg, DEPTH, ORDER);
    let footer_text = state.error.as_deref().unwrap_or(state.status.as_str());
    let footer_color = if state.error.is_some() {
        theme.danger
    } else {
        theme.text_muted
    };
    draw_text(
        sugarloaf,
        layout.footer.x + PANE_PAD,
        layout.footer.y + 6.0,
        footer_text,
        12.0,
        footer_color,
    );

    if let Some(prompt) = state.conflict.as_ref() {
        paint_conflict(sugarloaf, state, &layout, theme, prompt);
    }

    if let Some(drag) = state.drag.as_ref() {
        paint_drag_ghost(sugarloaf, theme, drag.pointer_x, drag.pointer_y, &drag.name);
    }
}

fn paint_conflict(
    sugarloaf: &mut Sugarloaf,
    state: &SftpPaneState,
    layout: &SftpPaneLayout,
    theme: &ChromeTheme,
    prompt: &terminus_ui::SftpConflictPrompt,
) {
    // Dim scrim over the pane.
    paint_flat(
        sugarloaf,
        &layout.bounds,
        [0.0, 0.0, 0.0, 0.45],
        DEPTH - 0.01,
        ORDER,
    );
    let card = layout.conflict_card();
    paint_flat(sugarloaf, &card, theme.panel_bg, DEPTH - 0.02, ORDER);
    paint_flat(
        sugarloaf,
        &Rect::new(card.x, card.y, card.width, 1.0),
        theme.panel_border,
        DEPTH - 0.02,
        ORDER,
    );

    draw_text(
        sugarloaf,
        card.x + 16.0,
        card.y + 16.0,
        prompt.title(),
        14.0,
        theme.text,
    );
    draw_text(
        sugarloaf,
        card.x + 16.0,
        card.y + 40.0,
        &prompt.message(),
        12.0,
        theme.text_muted,
    );

    let apply = layout.conflict_apply_all();
    let apply_hover = matches!(state.hover, Some(SftpHit::ConflictApplyAll));
    let check = if prompt.apply_to_all { "[x]" } else { "[ ]" };
    let apply_label = format!("{check} Apply to all");
    let apply_bg = if apply_hover {
        theme.item_hover
    } else {
        theme.panel_bg
    };
    paint_flat(sugarloaf, &apply, apply_bg, DEPTH - 0.02, ORDER);
    draw_text(
        sugarloaf,
        apply.x + 4.0,
        apply.y + 4.0,
        &apply_label,
        12.0,
        theme.text,
    );

    paint_text_btn(
        sugarloaf,
        theme,
        &layout.conflict_overwrite(),
        "Replace",
        matches!(state.hover, Some(SftpHit::ConflictOverwrite)),
        true,
    );
    paint_text_btn(
        sugarloaf,
        theme,
        &layout.conflict_keep(),
        "Keep",
        matches!(state.hover, Some(SftpHit::ConflictKeep)),
        false,
    );
    paint_text_btn(
        sugarloaf,
        theme,
        &layout.conflict_cancel(),
        "Cancel",
        matches!(state.hover, Some(SftpHit::ConflictCancel)),
        false,
    );
}

fn paint_text_btn(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    label: &str,
    hovered: bool,
    primary: bool,
) {
    let bg = if hovered {
        if primary {
            theme.accent_soft
        } else {
            theme.item_hover
        }
    } else if primary {
        theme.accent_soft
    } else {
        theme.field_bg
    };
    paint_flat(sugarloaf, rect, bg, DEPTH - 0.02, ORDER);
    draw_text(
        sugarloaf,
        rect.x + 12.0,
        rect.y + 6.0,
        label,
        12.0,
        theme.text,
    );
}

fn paint_drag_ghost(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    x: f32,
    y: f32,
    name: &str,
) {
    let mut color = theme.text;
    color[3] = 160;
    draw_text(sugarloaf, x + 12.0, y + 4.0, name, 12.0, color);
}

fn paint_toolbar(
    sugarloaf: &mut Sugarloaf,
    state: &SftpPaneState,
    layout: &SftpPaneLayout,
    theme: &ChromeTheme,
    scale: f32,
) {
    paint_flat(sugarloaf, &layout.toolbar, theme.button_bg, DEPTH, ORDER);
    paint_flat(
        sugarloaf,
        &Rect::new(
            layout.toolbar.x,
            layout.toolbar.bottom() - 1.0,
            layout.toolbar.width,
            1.0,
        ),
        theme.panel_border,
        DEPTH,
        ORDER,
    );

    if let Some(edit) = state.name_edit.as_ref() {
        let paint = edit.field_paint();
        paint_field_card_at(
            sugarloaf,
            theme,
            layout.name_field,
            edit.field_label(),
            &paint.text,
            true,
            paint.placeholder,
            0.0,
            true,
            DEPTH,
            ORDER,
        );
        if paint.show_caret {
            paint_field_caret_prefix_at(
                sugarloaf,
                theme,
                layout.name_field,
                &edit.draft.prefix_display(),
                0.0,
                DEPTH,
                ORDER,
            );
        }
        paint_icon_btn(
            sugarloaf,
            theme,
            scale,
            &layout.name_confirm,
            Icon::Check,
            matches!(state.hover, Some(SftpHit::NameConfirm)),
            false,
        );
        paint_icon_btn(
            sugarloaf,
            theme,
            scale,
            &layout.name_cancel,
            Icon::X,
            matches!(state.hover, Some(SftpHit::NameCancel)),
            true,
        );
    }

    let close_hover = matches!(state.hover, Some(SftpHit::Close));
    let close_bg = if close_hover {
        theme.item_hover
    } else {
        theme.accent_soft
    };
    paint_flat(sugarloaf, &layout.btn_close, close_bg, DEPTH, ORDER);
    let ix = layout.btn_close.x + 8.0;
    let iy = layout.btn_close.y + (BTN_SIZE - ICON_IN_BTN) * 0.5;
    draw_icon(
        sugarloaf,
        Icon::SquareTerminal,
        IconPlacement::new(ix, iy, ICON_IN_BTN),
        rgba_u8(theme.text),
        scale,
    );
    draw_text(
        sugarloaf,
        ix + ICON_IN_BTN + 6.0,
        layout.btn_close.y + 7.0,
        "Terminal",
        12.0,
        theme.text,
    );
}

fn paint_icon_btn(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    scale: f32,
    rect: &Rect,
    icon: Icon,
    hovered: bool,
    danger: bool,
) {
    let bg = if hovered {
        if danger {
            [
                theme.danger[0] as f32 / 255.0,
                theme.danger[1] as f32 / 255.0,
                theme.danger[2] as f32 / 255.0,
                0.25,
            ]
        } else {
            theme.item_hover
        }
    } else {
        theme.field_bg
    };
    paint_flat(sugarloaf, rect, bg, DEPTH, ORDER);
    let ix = rect.x + (BTN_SIZE - ICON_IN_BTN) * 0.5;
    let iy = rect.y + (BTN_SIZE - ICON_IN_BTN) * 0.5;
    let color = if danger && hovered {
        rgba_u8(theme.danger)
    } else {
        rgba_u8(theme.text)
    };
    draw_icon(
        sugarloaf,
        icon,
        IconPlacement::new(ix, iy, ICON_IN_BTN),
        color,
        scale,
    );
}

fn paint_side(
    sugarloaf: &mut Sugarloaf,
    state: &SftpPaneState,
    theme: &ChromeTheme,
    scale: f32,
    pane: &Rect,
    header: &Rect,
    parent_btn: &Rect,
    list: &Rect,
    side: &SftpSideState,
    focused: bool,
    is_left: bool,
) {
    let bg = if focused {
        theme.panel_bg
    } else {
        theme.shell_bg
    };
    paint_flat(sugarloaf, pane, bg, DEPTH, ORDER);
    paint_flat(sugarloaf, header, theme.button_bg, DEPTH, ORDER);

    let parent_hover = matches!(
        (&state.hover, is_left),
        (Some(SftpHit::LeftParent), true) | (Some(SftpHit::RightParent), false)
    );
    paint_icon_btn(
        sugarloaf,
        theme,
        scale,
        parent_btn,
        Icon::ChevronRight,
        parent_hover,
        false,
    );

    let crumb_x = parent_btn.right() + 8.0;
    draw_icon(
        sugarloaf,
        Icon::Folder,
        IconPlacement::new(crumb_x, header.y + 9.0, 14.0),
        rgba_u8(theme.text_muted),
        scale,
    );
    let label = format!("{}  {}", side.title(), side.cwd);
    draw_text(
        sugarloaf,
        crumb_x + 18.0,
        header.y + 9.0,
        &label,
        12.0,
        theme.text,
    );

    paint_flat(
        sugarloaf,
        &Rect::new(header.x, header.bottom() - 1.0, header.width, 1.0),
        theme.panel_border,
        DEPTH,
        ORDER,
    );

    paint_entries(
        sugarloaf,
        theme,
        scale,
        list,
        &side.entries,
        side.selected,
        side.scroll,
        &state.hover,
        is_left,
    );
}

fn paint_entries(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    scale: f32,
    list: &Rect,
    entries: &[SftpRow],
    selected: Option<usize>,
    scroll: f32,
    hover: &Option<SftpHit>,
    is_left: bool,
) {
    let visible_bottom = list.bottom();
    for (i, entry) in entries.iter().enumerate() {
        let y = list.y + i as f32 * ROW_HEIGHT - scroll;
        if y + ROW_HEIGHT < list.y || y > visible_bottom {
            continue;
        }
        let row = Rect::new(list.x, y, list.width, ROW_HEIGHT);
        let row_hover = match (hover, is_left) {
            (Some(SftpHit::LeftRow(h)), true) if *h == i => true,
            (Some(SftpHit::RightRow(h)), false) if *h == i => true,
            _ => false,
        };
        if selected == Some(i) {
            paint_flat(sugarloaf, &row, theme.item_selected, DEPTH, ORDER);
        } else if row_hover {
            paint_flat(sugarloaf, &row, theme.item_hover, DEPTH, ORDER);
        }

        let icon = if entry.is_dir {
            Icon::Folder
        } else {
            Icon::Copy
        };
        let icon_x = row.x + PANE_PAD;
        let icon_y = row.y + (ROW_HEIGHT - ICON_IN_BTN) * 0.5;
        draw_icon(
            sugarloaf,
            icon,
            IconPlacement::new(icon_x, icon_y, ICON_IN_BTN),
            rgba_u8(if entry.is_dir {
                theme.text
            } else {
                theme.text_muted
            }),
            scale,
        );

        let size = if entry.is_dir {
            String::new()
        } else {
            format!("  {}", format_size(entry.size))
        };
        let label = format!("{}{size}", entry.name);
        draw_text(
            sugarloaf,
            icon_x + ICON_IN_BTN + 8.0,
            row.y + 7.0,
            &label,
            12.0,
            theme.text,
        );
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}B")
    }
}

fn rgba_u8(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

fn draw_text(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: [u8; 4],
) {
    let opts = DrawOpts {
        font_size: size,
        color,
        ..DrawOpts::default()
    };
    sugarloaf.text_mut().draw_late(x, y, text, &opts);
}
