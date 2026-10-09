//! Connection progress: the Feedback step line (Local, Network, Handshake,
//! Shell) with a status line, optional logs and a Cancel button.
//!
//! [`paint_connection_content`] paints it over the shell's content rect
//! (where the connecting session's terminal will appear), through
//! [`paint_connection_in`], which centres the card in any area.

use crate::renderer::components::button::paint_button_in_rect;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize};
use terminus_ui::components::feedback::{
    step_paint, steps_at, StepLine, StepState, STEP_HALO_ALPHA, STEP_LABELS,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};
use terminus_ui::{Chrome, ConnectKind, ConnectionSequence, STEP_COUNT};

use super::{button_state, dialog_layer, paint_shadow, text_y, DEPTH, ORDER};
use crate::renderer::chrome::{draw_icon, paint_surface_stroke};
use crate::renderer::ui_text::{draw_mono_text, draw_ui_text, measure_ui_text, UiWeight};

/// Paint the progress over the shell's content rect (the Terminal view's
/// area): an opaque card-coloured cover hides the session's terminal, and
/// the progress card sits centred on it. The sidebar and header stay live.
pub fn paint_connection_content(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    card: [f32; 4],
    phase: f32,
) {
    let Some(conn) = chrome.connection.as_ref() else {
        return;
    };
    let area = chrome.connection_area();
    let r = terminus_ui::shell::layout::MAIN_RADIUS;
    sugarloaf.quad(
        None,
        area.x,
        area.y,
        area.width,
        area.height,
        card,
        [0.0, 0.0, r, r],
        DEPTH - 0.02,
        ORDER,
    );
    paint_connection_in(sugarloaf, conn, theme, area, phase, true);
}

fn disc(sugarloaf: &mut Sugarloaf, r: &Rect, color: [f32; 4], depth: f32) {
    sugarloaf.rounded_rect(
        None,
        r.x,
        r.y,
        r.width,
        r.height,
        color,
        depth,
        r.width * 0.5,
        ORDER,
    );
}

/// Paint the progress card centred in `area` (no scrim).
pub fn paint_connection_in(
    sugarloaf: &mut Sugarloaf,
    conn: &ConnectionSequence,
    theme: &ChromeTheme,
    area: Rect,
    phase: f32,
    glyphs: bool,
) {
    // Columns hug the real label advances.
    if glyphs {
        let mut widths = [0.0f32; STEP_COUNT];
        for (i, w) in widths.iter_mut().enumerate() {
            *w = measure_ui_text(
                sugarloaf,
                conn.step_label(i),
                font_size::CAPTION,
                UiWeight::Regular,
            );
        }
        conn.set_label_widths(widths);
    }
    let local = conn.dialog_rect(area.width, area.height);
    let dialog = Rect::new(
        area.x + local.x,
        area.y + local.y,
        local.width,
        local.height,
    );

    paint_shadow(sugarloaf, &dialog, radius::DIALOG, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &dialog,
        theme.dialog,
        Some(theme.dialog_line),
        radius::DIALOG,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    if !glyphs {
        return;
    }
    let scale = sugarloaf.scale_factor();

    // Header: host tile, name, endpoint, Show logs.
    let tile = conn.header_icon_rect(dialog);
    sugarloaf.rounded_rect(
        None,
        tile.x,
        tile.y,
        tile.width,
        tile.height,
        theme.choice_selected_bg,
        DEPTH + 0.04,
        radius::CARD,
        ORDER,
    );
    let glyph = match conn.kind {
        ConnectKind::Ssh => Icon::Server,
        ConnectKind::Wsl => Icon::SquareTerminal,
    };
    draw_icon(
        sugarloaf,
        glyph,
        IconPlacement::new(
            tile.x + (tile.width - 22.0) / 2.0,
            tile.y + (tile.height - 22.0) / 2.0,
            22.0,
        ),
        theme.accent,
        scale,
    );
    let tx = tile.right() + 14.0;
    draw_ui_text(
        sugarloaf,
        tx,
        tile.y + 2.0,
        &conn.title,
        font_size::BODY,
        theme.text,
        UiWeight::SemiBold,
    );
    draw_mono_text(
        sugarloaf,
        tx,
        tile.y + 24.0,
        &conn.endpoint,
        font_size::MONO,
        theme.text_muted,
        UiWeight::Regular,
    );
    let logs_btn = conn.logs_button_rect(dialog);
    paint_button_in_rect(
        sugarloaf,
        theme,
        &logs_btn,
        ButtonKind::Text,
        ButtonSize::Small,
        button_state(false, false),
        if conn.logs_open {
            "Hide logs"
        } else {
            "Show logs"
        },
        dialog_layer(theme),
    );

    // Step line.
    let (_, node_cx) =
        terminus_ui::connection::track_layout_from_labels(&conn.label_widths());
    let abs_cx = node_cx.map(|c| dialog.x + c);
    let cy = conn.node_center(dialog, 0).1;
    let line = StepLine::from_centers(&abs_cx, cy);
    let states: [StepState; STEP_COUNT] = if conn.succeeded {
        [StepState::Done; STEP_COUNT]
    } else {
        steps_at(conn.step, false)
    };
    for (i, seg) in line.segments.iter().enumerate() {
        let c = if StepLine::segment_lit(&states, i) {
            theme.success
        } else {
            theme.raised
        };
        sugarloaf.rounded_rect(
            None,
            seg.x,
            seg.y,
            seg.width,
            seg.height,
            c,
            DEPTH + 0.04,
            seg.height * 0.5,
            ORDER,
        );
    }
    for (i, state) in states.iter().enumerate() {
        let paint = step_paint(*state, theme);
        let node = line.nodes[i];
        if let Some(mut halo) = paint.halo {
            let pulse = (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
            halo[3] = STEP_HALO_ALPHA * (0.6 + 0.8 * pulse);
            disc(sugarloaf, &line.halo(i), halo, DEPTH + 0.05);
        }
        disc(sugarloaf, &node, paint.fill, DEPTH + 0.06);
        let (cx, ncy) = (node.x + node.width * 0.5, node.y + node.height * 0.5);
        if *state == StepState::Done {
            draw_icon(
                sugarloaf,
                Icon::Check,
                IconPlacement::new(cx - 8.0, ncy - 8.0, 16.0),
                paint.glyph,
                scale,
            );
        } else {
            let label = (i + 1).to_string();
            let w = measure_ui_text(sugarloaf, &label, 14.0, UiWeight::SemiBold);
            draw_ui_text(
                sugarloaf,
                cx - w * 0.5,
                ncy - 14.0 * 0.625,
                &label,
                14.0,
                terminus_ui::theme::text_color(paint.glyph),
                UiWeight::SemiBold,
            );
        }
        let name = STEP_LABELS[i];
        let w = measure_ui_text(sugarloaf, name, font_size::CAPTION, UiWeight::Regular);
        draw_ui_text(
            sugarloaf,
            cx - w * 0.5,
            node.bottom() + 10.0,
            name,
            font_size::CAPTION,
            if *state == StepState::Pending {
                theme.text_faint
            } else {
                theme.text_muted
            },
            UiWeight::Regular,
        );
    }

    // Status line.
    let status = conn.status_rect(dialog);
    let color = if conn.succeeded {
        terminus_ui::theme::text_color(theme.success)
    } else {
        theme.text_muted
    };
    let sw =
        measure_ui_text(sugarloaf, &conn.status, font_size::LABEL, UiWeight::Regular);
    draw_ui_text(
        sugarloaf,
        status.x + (status.width - sw) * 0.5,
        status.y + 8.0,
        &conn.status,
        font_size::LABEL,
        color,
        UiWeight::Regular,
    );

    // Logs drawer.
    if let Some(logs) = conn.logs_rect(dialog) {
        paint_surface_stroke(
            sugarloaf,
            &logs,
            theme.field,
            Some(theme.line),
            radius::SMALL,
            1.0,
            DEPTH + 0.04,
            ORDER,
            false,
        );
        let line_h = 15.0;
        let max_lines = ((logs.height - 16.0) / line_h).floor().max(1.0) as usize;
        let start = conn.logs.len().saturating_sub(max_lines);
        for (n, line) in conn.logs.iter().skip(start).take(max_lines).enumerate() {
            draw_mono_text(
                sugarloaf,
                logs.x + 10.0,
                logs.y + 8.0 + n as f32 * line_h,
                line,
                11.0,
                theme.text_muted,
                UiWeight::Regular,
            );
        }
    }

    // Cancel (Close once connected).
    let close = conn.close_button_rect(dialog);
    paint_button_in_rect(
        sugarloaf,
        theme,
        &close,
        ButtonKind::Secondary,
        ButtonSize::Medium,
        button_state(false, false),
        if conn.succeeded { "Close" } else { "Cancel" },
        dialog_layer(theme),
    );
    let _ = text_y;
}
