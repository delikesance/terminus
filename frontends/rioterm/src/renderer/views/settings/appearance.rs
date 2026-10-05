//! Appearance tab painter: font select (+ menu overlay), size stepper,
//! cursor and theme segmented controls and the live preview.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::input::{FieldKind, FieldState};
use terminus_ui::components::list::CARD_RADIUS;
use terminus_ui::components::selection::{
    SegmentedSize, SEGMENT_RADIUS, SEGMENT_TRACK_RADIUS,
};
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};
use terminus_ui::views::settings::appearance::{
    fit_segments, AppearanceLayout, AppearanceState, AppearanceTarget, CursorStyle,
    ThemeChoice, MENU_ROW_H, THEME_NOTE,
};

use super::keys::text_top;
use super::with_measure;
use crate::renderer::chrome::paint_surface_stroke;
use crate::renderer::components::input::{paint_field, FieldContent};
use crate::renderer::components::selection::paint_segmented;
use crate::renderer::ui_text::{
    draw_mono_text, draw_ui_text, measure_mono_text, UiWeight,
};

const PREVIEW_BLUE: [u8; 4] = [0x8D, 0xBB, 0xF5, 255];
const PREVIEW_GREEN: [u8; 4] = [0x9E, 0xD9, 0xB5, 255];
const PREVIEW_LILAC: [u8; 4] = [0xD6, 0xCB, 0xFF, 255];
const PREVIEW_FG: [u8; 4] = [0xE4, 0xDE, 0xEF, 255];

fn label(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, r: &Rect, text: &str) {
    draw_ui_text(
        sugarloaf,
        r.x,
        text_top(r.y + r.height / 2.0, font_size::LABEL),
        text,
        font_size::LABEL,
        theme.text,
        UiWeight::Medium,
    );
}

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    s: &AppearanceState,
) {
    let l = with_measure(sugarloaf, |m| s.layout(content, m));
    let scale = sugarloaf.scale_factor();

    label(sugarloaf, theme, &l.font_label, "Font");
    let fstate = if s.font_menu_open {
        FieldState::Focus
    } else if s.hover == Some(AppearanceTarget::FontSelect) {
        FieldState::Hover
    } else {
        FieldState::Filled
    };
    paint_field(
        sugarloaf,
        theme,
        &l.font_select,
        &FieldContent {
            kind: FieldKind::Select,
            state: fstate,
            label: None,
            value: s.font_label(),
            placeholder: "",
            helper: None,
            revealed: false,
            caret_prefix_width: None,
        },
        theme.canvas,
        scale,
    );

    label(sugarloaf, theme, &l.size_label, "Size");
    paint_stepper(sugarloaf, theme, s, &l);

    label(sugarloaf, theme, &l.cursor_label, "Cursor");
    let names: Vec<&str> = CursorStyle::ALL.iter().map(|c| c.label()).collect();
    let selected = CursorStyle::ALL
        .iter()
        .position(|c| *c == s.cursor)
        .unwrap_or(0);
    paint_segmented(
        sugarloaf,
        theme,
        (l.cursor.track.x, l.cursor.track.y),
        &names,
        selected,
        SegmentedSize::Medium,
    );

    label(sugarloaf, theme, &l.theme_label, "Theme");
    let names: Vec<&str> = ThemeChoice::ALL.iter().map(|c| c.label()).collect();
    let selected = ThemeChoice::ALL
        .iter()
        .position(|c| *c == s.theme)
        .unwrap_or(0);
    paint_segmented(
        sugarloaf,
        theme,
        (l.theme.track.x, l.theme.track.y),
        &names,
        selected,
        SegmentedSize::Medium,
    );
    // Light and System have no chrome theme behind them: veil those segments.
    for (i, seg) in l.theme.segments.iter().enumerate() {
        if !ThemeChoice::ALL[i].available() {
            let c = theme.surface;
            sugarloaf.rounded_rect(
                None,
                seg.x - 1.0,
                seg.y - 1.0,
                seg.width + 2.0,
                seg.height + 2.0,
                [c[0], c[1], c[2], 0.62],
                0.4,
                SEGMENT_RADIUS,
                7,
            );
        }
    }
    let _ = SEGMENT_TRACK_RADIUS;
    draw_ui_text(
        sugarloaf,
        l.theme_note.x,
        text_top(
            l.theme_note.y + l.theme_note.height / 2.0,
            font_size::CAPTION,
        ),
        THEME_NOTE,
        font_size::CAPTION,
        theme.text_faint,
        UiWeight::Regular,
    );

    label(sugarloaf, theme, &l.preview_label, "Preview");
    paint_preview(sugarloaf, theme, &l.preview, s);

    if let Some(panel) = l.menu {
        paint_menu(sugarloaf, theme, s, &l, panel);
    }
}

fn paint_stepper(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    s: &AppearanceState,
    l: &AppearanceLayout,
) {
    sugarloaf.rounded_rect(
        None,
        l.size_track.x,
        l.size_track.y,
        l.size_track.width,
        l.size_track.height,
        theme.surface,
        0.1,
        SEGMENT_TRACK_RADIUS,
        7,
    );
    for (r, glyph, target) in [
        (l.size_minus, "\u{2212}", AppearanceTarget::SizeMinus),
        (l.size_plus, "+", AppearanceTarget::SizePlus),
    ] {
        if s.hover == Some(target) {
            sugarloaf.rounded_rect(
                None,
                r.x,
                r.y,
                r.width,
                r.height,
                theme.selected,
                0.11,
                SEGMENT_RADIUS,
                7,
            );
        }
        let w = crate::renderer::ui_text::measure_ui_text(
            sugarloaf,
            glyph,
            16.0,
            UiWeight::Medium,
        );
        draw_ui_text(
            sugarloaf,
            r.x + (r.width - w) / 2.0,
            text_top(r.y + r.height / 2.0, 16.0),
            glyph,
            16.0,
            theme.text,
            UiWeight::Medium,
        );
    }
    let value = terminus_ui::views::settings::format_size(s.size);
    let w = crate::renderer::ui_text::measure_ui_text(
        sugarloaf,
        &value,
        14.0,
        UiWeight::Regular,
    );
    draw_ui_text(
        sugarloaf,
        l.size_value.x + (l.size_value.width - w) / 2.0,
        text_top(l.size_value.y + l.size_value.height / 2.0, 14.0),
        &value,
        14.0,
        theme.text,
        UiWeight::Regular,
    );
}

fn paint_preview(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    r: &Rect,
    s: &AppearanceState,
) {
    sugarloaf.rounded_rect(
        None,
        r.x,
        r.y,
        r.width,
        r.height,
        theme.frame,
        0.1,
        CARD_RADIUS,
        0,
    );
    let size = (s.size * 13.0 / 14.0).clamp(10.0, 20.0);
    let line = (size * 1.7).round();
    let x = r.x + 22.0;
    let mut y = r.y + 22.0;
    let cw = measure_mono_text(sugarloaf, "M", size, UiWeight::Regular);
    // Columns that fit inside the card (22 px padding each side); a
    // narrow window cuts the lines instead of running past the card.
    let cols = if cw > 0.0 {
        ((r.width - 44.0) / cw).floor().max(0.0) as usize
    } else {
        usize::MAX
    };
    let prompt = "delikesance@DELIKESANCE:~$ ";
    let draw_line = |sugarloaf: &mut Sugarloaf, y: f32, segs: &[(&str, [u8; 4])]| {
        let texts: Vec<&str> = segs.iter().map(|(t, _)| *t).collect();
        let mut cx = x;
        for (text, (_, color)) in fit_segments(&texts, cols).iter().zip(segs) {
            cx += draw_mono_text(sugarloaf, cx, y, text, size, *color, UiWeight::Regular);
        }
        cx - x
    };
    draw_line(sugarloaf, y, &[(prompt, PREVIEW_FG), ("ls", PREVIEW_FG)]);
    y += line;
    draw_line(
        sugarloaf,
        y,
        &[
            ("development", PREVIEW_BLUE),
            ("  ", PREVIEW_FG),
            ("notes.md", PREVIEW_GREEN),
            ("  ", PREVIEW_FG),
            ("build.tar.gz", PREVIEW_LILAC),
        ],
    );
    y += line;
    let pw = draw_line(sugarloaf, y, &[(prompt, PREVIEW_FG)]);
    let cell_h = (size * 1.35).round();
    let top = y - size * 0.12;
    let cursor_x = x + pw;
    let rect = match s.cursor {
        CursorStyle::Block => Rect::new(cursor_x, top, cw, cell_h),
        CursorStyle::Underline => Rect::new(cursor_x, top + cell_h - 2.0, cw, 2.0),
        CursorStyle::Beam => Rect::new(cursor_x, top, 2.0, cell_h),
    };
    sugarloaf.rounded_rect(
        None,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        theme.accent,
        0.2,
        1.0,
        0,
    );
}

fn paint_menu(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    s: &AppearanceState,
    l: &AppearanceLayout,
    panel: Rect,
) {
    sugarloaf.begin_overlay();
    paint_surface_stroke(
        sugarloaf,
        &panel,
        theme.dialog,
        Some(theme.dialog_line),
        radius::CONTROL,
        1.0,
        0.1,
        30,
        false,
    );
    for (i, row) in &l.menu_rows {
        let current = s.fonts[*i] == s.font;
        let hovered = s.hover == Some(AppearanceTarget::FontRow(*i));
        if hovered || current {
            let bg = if hovered {
                theme.selected
            } else {
                theme.surface
            };
            sugarloaf.rounded_rect(
                None,
                row.x,
                row.y,
                row.width,
                row.height,
                bg,
                0.12,
                radius::SMALL,
                30,
            );
        }
        let color = if current {
            theme.text
        } else {
            theme.text_muted
        };
        draw_ui_text(
            sugarloaf,
            row.x + 12.0,
            text_top(row.y + MENU_ROW_H / 2.0, font_size::BODY_SM),
            &s.fonts[*i],
            font_size::BODY_SM,
            color,
            if current {
                UiWeight::Medium
            } else {
                UiWeight::Regular
            },
        );
        if current {
            sugarloaf.rounded_rect(
                None,
                row.right() - 18.0,
                row.y + MENU_ROW_H / 2.0 - 3.0,
                6.0,
                6.0,
                theme.accent,
                0.13,
                3.0,
                30,
            );
        }
    }
    sugarloaf.end_overlay();
}
