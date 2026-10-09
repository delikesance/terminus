//! Overlay components: dialog shell, wizard stepper, context menu and
//! command palette, plus the `overlay` gallery section.
//!
//! Geometry comes from `terminus_ui::components::overlay`; this file only
//! walks those rects. Dialog buttons are Button components (large) and the
//! option row is the Selection checkbox, both painted on the dialog layer.
//! Known gap: the title's -2% letter spacing has no sugarloaf API, so the
//! title is drawn with default tracking.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonSpec, ButtonState};
use terminus_ui::components::overlay::{
    self as ov, dialog_layout_at, stepper_segments, wrap_text, DialogFocus, DialogKind,
    DialogLayout, Menu, MenuEntry, MenuVisual, Palette, PaletteGroup, PaletteItem,
    PaletteLayout, PaletteRow,
};
use terminus_ui::components::selection::ControlState;
use terminus_ui::geom::Rect;

use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::{text_color, ChromeTheme};
use terminus_ui::tokens::{font_size, radius};

use super::button::{label_spec, paint_button_on};
use super::selection::paint_checkbox_on;
use super::Layer;
use crate::renderer::chrome::{draw_icon, paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

const ORDER: u8 = 30;
const DEPTH: f32 = 0.1;

fn rgba8(c: [u8; 4]) -> [f32; 4] {
    c.map(|v| v as f32 / 255.0)
}

/// Y that vertically centres a text line of `size` in `rect`.
fn text_y(rect: &Rect, size: f32) -> f32 {
    rect.y + (rect.height - size * 1.25) / 2.0
}

/// Soft drop shadow under a rounded panel (layered translucent rects).
fn paint_shadow(sugarloaf: &mut Sugarloaf, r: &Rect, rad: f32, depth: f32) {
    for (grow, dy, a) in [(14.0, 14.0, 0.07), (8.0, 12.0, 0.10), (3.0, 10.0, 0.14)] {
        let s = Rect::new(
            r.x - grow,
            r.y + dy - grow,
            r.width + 2.0 * grow,
            r.height + 2.0 * grow,
        );
        sugarloaf.rounded_rect(
            None,
            s.x,
            s.y,
            s.width,
            s.height,
            [0.0, 0.0, 0.0, a],
            depth,
            rad + grow,
            ORDER,
        );
    }
}

// ----------------------------------------------------------------- dialog

/// Content of a dialog.
pub struct DialogSpec<'a> {
    pub kind: DialogKind,
    pub title: &'a str,
    pub body: &'a str,
    pub confirm: &'a str,
    pub cancel: &'a str,
    /// Label of the optional checkbox row (`DialogKind::WithOption`).
    pub option: Option<&'a str>,
    pub option_checked: bool,
}

/// Lay out `spec` at `(x, y)` (measures text for wrapping and button widths).
pub fn dialog_layout_for(
    sugarloaf: &mut Sugarloaf,
    spec: &DialogSpec,
    at: (f32, f32),
    window: (f32, f32),
) -> (DialogLayout, Vec<String>) {
    let inner = ov::DIALOG_WIDTH - 2.0 * ov::DIALOG_PAD;
    let lines = wrap_text(spec.body, inner, |s| {
        measure_ui_text(sugarloaf, s, font_size::BODY_SM, UiWeight::Regular)
    });
    let (cancel_w, confirm_w) = action_widths(sugarloaf, spec);
    let layout = dialog_layout_at(
        at.0,
        at.1,
        spec.kind,
        lines.len(),
        cancel_w,
        confirm_w,
        window,
    );
    (layout, lines)
}

/// Button kinds of a dialog's (cancel, confirm) actions.
fn action_kinds(kind: DialogKind) -> (ButtonKind, ButtonKind) {
    let confirm = if kind == DialogKind::Destructive {
        ButtonKind::Danger
    } else {
        ButtonKind::Primary
    };
    (ButtonKind::Secondary, confirm)
}

fn action_spec(
    sugarloaf: &mut Sugarloaf,
    rect: &Rect,
    kind: ButtonKind,
    label: &str,
) -> ButtonSpec {
    label_spec(
        sugarloaf,
        (rect.x, rect.y),
        kind,
        ButtonSize::Large,
        label,
        false,
    )
}

/// Widths of the (cancel, confirm) buttons, measured as Button components.
fn action_widths(sugarloaf: &mut Sugarloaf, spec: &DialogSpec) -> (f32, f32) {
    let (ck, fk) = action_kinds(spec.kind);
    let origin = Rect::new(0.0, 0.0, 0.0, 0.0);
    (
        action_spec(sugarloaf, &origin, ck, spec.cancel).width(),
        action_spec(sugarloaf, &origin, fk, spec.confirm).width(),
    )
}

fn dialog_layer(theme: &ChromeTheme, depth: f32) -> Layer {
    Layer {
        order: ORDER,
        depth,
        backdrop: theme.dialog,
    }
}

/// Paint one dialog (no scrim). `focus` draws a focus ring on that button.
pub fn paint_dialog(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: &DialogSpec,
    layout: &DialogLayout,
    lines: &[String],
    focus: Option<DialogFocus>,
) {
    paint_shadow(sugarloaf, &layout.dialog, radius::DIALOG, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &layout.dialog,
        theme.dialog,
        Some(theme.dialog_line),
        radius::DIALOG,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    draw_ui_text(
        sugarloaf,
        layout.title.x,
        layout.title.y,
        spec.title,
        font_size::TITLE,
        theme.text,
        UiWeight::SemiBold,
    );
    for (i, line) in lines.iter().enumerate() {
        draw_ui_text(
            sugarloaf,
            layout.body.x,
            layout.body.y + i as f32 * ov::BODY_LINE,
            line,
            font_size::BODY_SM,
            theme.text_muted,
            UiWeight::Regular,
        );
    }
    if let (Some(row), Some(label)) = (layout.option.as_ref(), spec.option) {
        paint_checkbox_on(
            sugarloaf,
            theme,
            (row.x, row.y),
            label,
            spec.option_checked,
            ControlState::Default,
            dialog_layer(theme, DEPTH + 0.05),
        );
    }
    let (cancel_kind, confirm_kind) = action_kinds(spec.kind);
    let layer = dialog_layer(theme, DEPTH + 0.05);
    for (rect, kind, label, which) in [
        (
            &layout.cancel,
            cancel_kind,
            spec.cancel,
            DialogFocus::Cancel,
        ),
        (
            &layout.confirm,
            confirm_kind,
            spec.confirm,
            DialogFocus::Confirm,
        ),
    ] {
        let button = action_spec(sugarloaf, rect, kind, label);
        let state = if focus == Some(which) {
            ButtonState::Focus
        } else {
            ButtonState::Default
        };
        paint_button_on(sugarloaf, theme, &button, state, label, None, layer);
    }
}

/// Full-window scrim, then the dialog centred over it.
#[allow(dead_code)]
pub fn paint_modal_dialog(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: &DialogSpec,
    window: (f32, f32),
    focus: DialogFocus,
) -> DialogLayout {
    let lines = wrap_text(spec.body, ov::DIALOG_WIDTH - 2.0 * ov::DIALOG_PAD, |s| {
        measure_ui_text(sugarloaf, s, font_size::BODY_SM, UiWeight::Regular)
    });
    let (cancel_w, confirm_w) = action_widths(sugarloaf, spec);
    let layout = ov::dialog_layout(window, spec.kind, lines.len(), cancel_w, confirm_w);
    paint_flat(sugarloaf, &layout.scrim, ov::SCRIM, DEPTH - 0.02, ORDER);
    paint_dialog(sugarloaf, theme, spec, &layout, &lines, Some(focus));
    layout
}

// ---------------------------------------------------------------- stepper

/// Paint the wizard stepper in `area` (see [`ov::STEPPER_HEIGHT`]).
pub fn paint_stepper(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    area: Rect,
    step: usize,
) {
    for seg in stepper_segments(area, step) {
        let col = if seg.filled {
            theme.accent
        } else {
            theme.dialog_line
        };
        sugarloaf.rounded_rect(
            None,
            seg.bar.x,
            seg.bar.y,
            seg.bar.width,
            seg.bar.height,
            col,
            DEPTH,
            2.0,
            ORDER,
        );
        let tc = if seg.current {
            theme.text
        } else {
            theme.text_faint
        };
        draw_ui_text(
            sugarloaf,
            seg.label_rect.x,
            seg.label_rect.y,
            seg.label,
            font_size::CAPTION,
            tc,
            UiWeight::Regular,
        );
    }
}

// ------------------------------------------------------------------- menu

pub fn paint_menu(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, menu: &Menu) {
    let r = menu.rect();
    paint_shadow(sugarloaf, &r, ov::MENU_RADIUS, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &r,
        theme.surface,
        Some(theme.line),
        ov::MENU_RADIUS,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    for (i, entry) in menu.entries.iter().enumerate() {
        if let Some(sep) = menu.separator_rect(i) {
            paint_flat(
                sugarloaf,
                &Rect::new(sep.x + 2.0, sep.y + 5.0, sep.width - 4.0, 1.0),
                theme.line,
                DEPTH + 0.05,
                ORDER,
            );
            continue;
        }
        let Some(row) = menu.item_rect(i) else {
            continue;
        };
        let visual = menu.visual(i);
        if matches!(visual, MenuVisual::Hover | MenuVisual::DangerHover) {
            sugarloaf.rounded_rect(
                None,
                row.x,
                row.y,
                row.width,
                row.height,
                theme.selected,
                DEPTH + 0.05,
                ov::MENU_ITEM_RADIUS,
                ORDER,
            );
        }
        let col = match visual {
            MenuVisual::Disabled => theme.text_faint,
            MenuVisual::Danger | MenuVisual::DangerHover => theme.danger_text,
            _ => theme.text,
        };
        draw_ui_text(
            sugarloaf,
            row.x + ov::MENU_ITEM_PAD_X,
            text_y(&row, 14.0),
            &entry.label,
            14.0,
            col,
            UiWeight::Regular,
        );
    }
}

// ---------------------------------------------------------------- palette

pub fn paint_palette(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    palette: &Palette,
    layout: &PaletteLayout,
) {
    paint_shadow(sugarloaf, &layout.panel, ov::PALETTE_RADIUS, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &layout.panel,
        theme.dialog,
        Some(theme.dialog_line),
        ov::PALETTE_RADIUS,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    // Query row.
    let q = &layout.query;
    let icon = ov::PALETTE_QUERY_ICON;
    draw_icon(
        sugarloaf,
        Icon::Search,
        IconPlacement::new(
            q.x + ov::PALETTE_QUERY_PAD_X,
            q.y + (q.height - icon) / 2.0,
            icon,
        ),
        rgba8(theme.text_muted),
        sugarloaf.scale_factor(),
    );
    let (qtext, qcol) = if palette.query.is_empty() {
        (
            palette
                .placeholder
                .as_deref()
                .unwrap_or("Search servers and commands"),
            theme.text_faint,
        )
    } else {
        (palette.query.as_str(), theme.text)
    };
    draw_ui_text(
        sugarloaf,
        q.x + ov::PALETTE_QUERY_PAD_X + icon + 12.0,
        text_y(q, 19.0),
        qtext,
        19.0,
        qcol,
        UiWeight::Regular,
    );
    paint_flat(
        sugarloaf,
        &Rect::new(q.x + 1.0, q.bottom() - 1.0, q.width - 2.0, 1.0),
        theme.raised,
        DEPTH + 0.05,
        ORDER,
    );
    // Rows.
    for row in &layout.rows {
        match *row {
            PaletteRow::Header { rect, group } => {
                let first = matches!(layout.rows.first(), Some(PaletteRow::Header { group: g, .. }) if *g == group);
                let top = if first { 8.0 } else { 12.0 };
                draw_ui_text(
                    sugarloaf,
                    rect.x + ov::PALETTE_ITEM_PAD_X,
                    rect.y + top,
                    &palette.groups[group].title,
                    font_size::CAPTION,
                    theme.text_faint,
                    UiWeight::Regular,
                );
            }
            PaletteRow::Item { rect, index } => {
                if index == palette.selected {
                    sugarloaf.rounded_rect(
                        None,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                        theme.selected,
                        DEPTH + 0.05,
                        ov::PALETTE_ITEM_RADIUS,
                        ORDER,
                    );
                }
                let item = palette
                    .groups
                    .iter()
                    .flat_map(|g| g.items.iter())
                    .nth(index);
                if let Some(item) = item {
                    draw_ui_text(
                        sugarloaf,
                        rect.x + ov::PALETTE_ITEM_PAD_X,
                        text_y(&rect, 15.0),
                        &item.label,
                        15.0,
                        theme.text,
                        UiWeight::Regular,
                    );
                    let hw = measure_ui_text(
                        sugarloaf,
                        &item.hint,
                        font_size::CAPTION,
                        UiWeight::Regular,
                    );
                    draw_ui_text(
                        sugarloaf,
                        rect.right() - ov::PALETTE_ITEM_PAD_X - hw,
                        text_y(&rect, font_size::CAPTION),
                        &item.hint,
                        font_size::CAPTION,
                        theme.text_muted,
                        UiWeight::Regular,
                    );
                }
            }
        }
    }
    if let Some(e) = layout.empty {
        let msg = format!(
            "No server or command matches \u{201c}{}\u{201d}. ",
            palette.query
        );
        let x = e.x + ov::PALETTE_ITEM_PAD_X;
        let y = text_y(&e, 14.0);
        let w = draw_ui_text(
            sugarloaf,
            x,
            y,
            &msg,
            14.0,
            theme.text_muted,
            UiWeight::Regular,
        );
        let link = palette.add_server_label();
        let lw = draw_ui_text(
            sugarloaf,
            x + w,
            y,
            &link,
            14.0,
            text_color(theme.accent),
            UiWeight::Regular,
        );
        paint_flat(
            sugarloaf,
            &Rect::new(x + w, y + 14.0 * 1.25, lw, 1.0),
            theme.accent,
            DEPTH + 0.05,
            ORDER,
        );
    }
    // Footer.
    let f = &layout.footer;
    paint_flat(
        sugarloaf,
        &Rect::new(f.x + 1.0, f.y, f.width - 2.0, 1.0),
        theme.raised,
        DEPTH + 0.05,
        ORDER,
    );
    let mut x = f.x + ov::PALETTE_QUERY_PAD_X;
    for hint in ov::PALETTE_HINTS {
        let w = draw_ui_text(
            sugarloaf,
            x,
            text_y(f, font_size::CAPTION),
            hint,
            font_size::CAPTION,
            theme.text_muted,
            UiWeight::Regular,
        );
        x += w + ov::PALETTE_FOOTER_GAP;
    }
}

// ---------------------------------------------------------------- gallery

fn caption(
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
        font_size::CAPTION,
        theme.text_faint,
        UiWeight::Regular,
    );
    font_size::CAPTION + 10.0
}

fn heading(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    x: f32,
    y: f32,
    text: &str,
) -> f32 {
    draw_ui_text(sugarloaf, x, y, text, 18.0, theme.text, UiWeight::SemiBold);
    18.0 + 14.0
}

fn note(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    x: f32,
    y: f32,
    width: f32,
    text: &str,
) -> f32 {
    let lines = wrap_text(text, width, |s| {
        measure_ui_text(sugarloaf, s, 13.0, UiWeight::Regular)
    });
    for (i, l) in lines.iter().enumerate() {
        draw_ui_text(
            sugarloaf,
            x,
            y + i as f32 * 20.0,
            l,
            13.0,
            theme.text_muted,
            UiWeight::Regular,
        );
    }
    lines.len() as f32 * 20.0 + 8.0
}

/// Paint the `overlay` gallery at `origin` within `width`; returns the height used.
pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let (ox, oy) = origin;
    let mut y = oy + super::paint_section_title(sugarloaf, theme, origin, "Overlay");
    y += 8.0;
    let window = (width + 2.0 * ox, 10_000.0);

    // --- Dialog ---
    y += heading(sugarloaf, theme, ox, y, "Dialog");
    let specs = [
        ("Confirm", DialogSpec {
            kind: DialogKind::Confirm, title: "Quit Terminus?",
            body: "3 sessions are still open. Quitting closes them.",
            confirm: "Quit", cancel: "Keep working", option: None, option_checked: false,
        }),
        ("Destructive", DialogSpec {
            kind: DialogKind::Destructive, title: "Delete jerem prod?",
            body: "It disappears from every computer you sync with. Your keys and snippets stay.",
            confirm: "Delete", cancel: "Cancel", option: None, option_checked: false,
        }),
        ("With option", DialogSpec {
            kind: DialogKind::WithOption, title: "build.tar.gz already exists",
            body: "There is already a file with this name in /home/ubuntu/app.",
            confirm: "Replace", cancel: "Keep existing",
            option: Some("Do this for every conflict"), option_checked: false,
        }),
    ];
    let (mut x, mut row_h, row_top) = (ox, 0.0f32, y);
    let mut row_y = row_top;
    for (label, spec) in &specs {
        if x + ov::DIALOG_WIDTH > ox + width && x > ox {
            x = ox;
            row_y += row_h + 28.0;
            row_h = 0.0;
        }
        let cap_h = caption(sugarloaf, theme, x, row_y, label);
        let (layout, lines) =
            dialog_layout_for(sugarloaf, spec, (x, row_y + cap_h), window);
        let focus = (*label == "Destructive").then_some(DialogFocus::Cancel);
        paint_dialog(sugarloaf, theme, spec, &layout, &lines, focus);
        row_h = row_h.max(cap_h + layout.dialog.height);
        x += ov::DIALOG_WIDTH + 28.0;
    }
    y = row_y + row_h + 28.0;
    // Scrim swatch.
    let cap_h = caption(
        sugarloaf,
        theme,
        ox,
        y,
        "Scrim · rgba(6,5,10,0.64) over the whole window, dialog centred",
    );
    let sw = Rect::new(ox, y + cap_h, 140.0, 48.0);
    paint_flat(
        sugarloaf,
        &Rect::new(sw.x, sw.y, sw.width / 2.0, sw.height),
        theme.surface,
        DEPTH,
        ORDER,
    );
    paint_flat(sugarloaf, &sw, ov::SCRIM, DEPTH + 0.01, ORDER);
    y = sw.bottom() + 10.0;
    y += note(
        sugarloaf, theme, ox, y, width.min(720.0),
        "Title says what will happen as a question or a fact. The confirming button repeats the verb: Quit, Delete, Replace, never OK. Esc cancels; Enter activates the focused button (Cancel for destructive).",
    );
    y += 24.0;

    // --- Wizard stepper ---
    y += heading(sugarloaf, theme, ox, y, "Wizard stepper");
    for step in 1..=3 {
        let cap_h = caption(sugarloaf, theme, ox, y, &format!("Step {step}"));
        paint_stepper(
            sugarloaf,
            theme,
            Rect::new(ox, y + cap_h, 380.0, ov::STEPPER_HEIGHT),
            step,
        );
        y += cap_h + ov::STEPPER_HEIGHT + 18.0;
    }
    y += 6.0;

    // --- Context menu ---
    y += heading(sugarloaf, theme, ox, y, "Context menu");
    let cap_h = caption(
        sugarloaf,
        theme,
        ox,
        y,
        "Server menu · states: hover, default, disabled, danger",
    );
    let mut menu = Menu::open(
        ox,
        y + cap_h,
        vec![
            MenuEntry::item("New session"),
            MenuEntry::item("Open files"),
            MenuEntry::item("Copy SSH command"),
            MenuEntry::item("Open in the other pane").disabled(),
            MenuEntry::separator(),
            MenuEntry::item("Edit server"),
            MenuEntry::item("Delete").danger(),
        ],
    )
    .expect("non-empty");
    menu.hover = Some(0);
    paint_menu(sugarloaf, theme, &menu);
    // Danger hover beside it.
    let mut danger = Menu::open(
        ox + ov::MENU_WIDTH + 160.0, // clear of the long caption above the first menu
        y + cap_h,
        vec![
            MenuEntry::item("Edit server"),
            MenuEntry::separator(),
            MenuEntry::item("Delete").danger(),
        ],
    )
    .expect("non-empty");
    danger.hover = Some(2);
    caption(sugarloaf, theme, danger.x, y, "Danger hovered");
    paint_menu(sugarloaf, theme, &danger);
    y = menu.rect().bottom() + 10.0;
    y += note(
        sugarloaf, theme, ox, y, width.min(720.0),
        "Opens on right click or the row's \u{2026} button. Separators split everyday actions from destructive ones. Up/Down skip disabled rows and separators.",
    );
    y += 24.0;

    // --- Command palette ---
    y += heading(sugarloaf, theme, ox, y, "Command palette");
    let results = Palette::new(
        "spl",
        vec![PaletteGroup::new(
            "Commands",
            vec![
                PaletteItem::new("Split Right", "Terminal"),
                PaletteItem::new("Split Down", "Terminal"),
            ],
        )],
    );
    let cap_h = caption(sugarloaf, theme, ox, y, "Results · first item selected");
    let l = results.layout_at(ox, y + cap_h);
    paint_palette(sugarloaf, theme, &results, &l);
    y = l.panel.bottom() + 32.0;
    let empty = Palette::new("splt", vec![]);
    let cap_h = caption(sugarloaf, theme, ox, y, "Empty result");
    let l = empty.layout_at(ox, y + cap_h);
    paint_palette(sugarloaf, theme, &empty, &l);
    y = l.panel.bottom() + 12.0;

    y - oy
}
