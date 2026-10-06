//! Tunnels view painter (+ `TERMINUS_VIEW_PREVIEW=tunnels` harness).
//!
//! `paint` walks the geometry of `terminus_ui::views::tunnels`; input goes
//! through `crate::tunnel_worker::TunnelController::{press, key, hover}` with
//! the same `content` rect.

use std::cell::RefCell;

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonState};
use terminus_ui::components::input::{FieldKind, FieldState};
use terminus_ui::components::list::{CardState, CARD_RADIUS};
use terminus_ui::components::selection::SegmentedSize;
use terminus_ui::geom::Rect;
use terminus_ui::text_field::TextDraft;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};
use terminus_ui::views::tunnels::*;

use crate::renderer::chrome::{paint_flat, paint_surface_stroke};
use crate::renderer::components::button::{label_spec, paint_button_on};
use crate::renderer::components::input::{paint_field, FieldContent};
use crate::renderer::components::list::{paint_card, Action, CardContent, DotKind};
use crate::renderer::components::selection::paint_segmented;
use crate::renderer::components::Layer;
use crate::renderer::ui_text::{
    draw_mono_text, draw_ui_text, measure_mono_text, measure_ui_text, UiWeight,
};

const ORDER: u8 = 7;
const D_SCRIM: f32 = 0.0;
const D_PANEL: f32 = 0.02;
const D_CTRL: f32 = 0.2;

fn text_top(r: &Rect, size: f32) -> f32 {
    r.y + (r.height - size) / 2.0 - size * 0.12
}

/// Measure every text-dependent width and cache it for hit-testing.
fn measure(sugarloaf: &mut Sugarloaf, state: &TunnelsState) -> Metrics {
    let mut m = state.metrics.get();
    let w = |s: &mut Sugarloaf, kind, size, label: &str| {
        label_spec(s, (0.0, 0.0), kind, size, label, false).width()
    };
    m.new_button = w(
        sugarloaf,
        ButtonKind::Primary,
        ButtonSize::Medium,
        "New tunnel",
    );
    m.start = w(
        sugarloaf,
        ButtonKind::Secondary,
        ButtonSize::Medium,
        "Start",
    );
    m.stop = w(sugarloaf, ButtonKind::Secondary, ButtonSize::Medium, "Stop");
    m.meta = TunnelKind::ALL
        .iter()
        .map(|k| {
            measure_ui_text(sugarloaf, k.label(), font_size::CAPTION, UiWeight::Regular)
        })
        .fold(0.0, f32::max);
    for (i, k) in TunnelKind::ALL.iter().enumerate() {
        m.kind_text[i] = measure_ui_text(
            sugarloaf,
            k.label(),
            SegmentedSize::Medium.font_size(),
            UiWeight::Medium,
        );
    }
    m.cancel = w(
        sugarloaf,
        ButtonKind::Secondary,
        ButtonSize::Large,
        "Cancel",
    );
    let confirm = confirm_label(state);
    m.confirm = w(sugarloaf, ButtonKind::Primary, ButtonSize::Large, confirm);
    state.metrics.set(m);
    m
}

fn confirm_label(state: &TunnelsState) -> &'static str {
    match &state.form {
        Some(f) if f.id.is_some() => "Save",
        _ => "Start tunnel",
    }
}

/// Paint the view into `content`; `window` is the full window (scrim area).
pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    window: Rect,
    content: Rect,
    state: &TunnelsState,
) {
    let m = measure(sugarloaf, state);
    let l = list_layout(content, state.items.len(), &m);
    let dialog_open = state.form.is_some();

    if dialog_open {
        // Under a modal only shells are painted (text has no layering).
        for c in &l.cards {
            if c.bottom() <= content.bottom() {
                sugarloaf.rounded_rect(
                    None,
                    c.x,
                    c.y,
                    c.width,
                    c.height,
                    CardState::Default.background(theme),
                    0.1,
                    CARD_RADIUS,
                    0,
                );
            }
        }
    } else {
        paint_list(sugarloaf, theme, content, state, &l);
    }

    if let Some(form) = &state.form {
        paint_flat(
            sugarloaf,
            &window,
            terminus_ui::components::overlay::SCRIM,
            D_SCRIM,
            ORDER,
        );
        paint_dialog(sugarloaf, theme, content, state, form, &m);
    }
}

fn paint_list(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &TunnelsState,
    l: &ListLayout,
) {
    let desc = format!(
        "Forward ports between this computer and {}.",
        state.host_label
    );
    draw_ui_text(
        sugarloaf,
        l.description.x,
        text_top(&l.description, font_size::BODY_SM),
        &desc,
        font_size::BODY_SM,
        theme.text_muted,
        UiWeight::Regular,
    );
    let btn = label_spec(
        sugarloaf,
        (l.new_button.x, l.new_button.y),
        ButtonKind::Primary,
        ButtonSize::Medium,
        "New tunnel",
        false,
    );
    let bstate = if state.hover == Hover::New {
        ButtonState::Hover
    } else {
        ButtonState::Default
    };
    let layer = Layer {
        order: 2,
        depth: 0.06,
        backdrop: theme.canvas,
    };
    paint_button_on(sugarloaf, theme, &btn, bstate, "New tunnel", None, layer);

    if let Some(e) = l.empty {
        let title = "No tunnels yet";
        let w = measure_ui_text(sugarloaf, title, font_size::BODY, UiWeight::Medium);
        draw_ui_text(
            sugarloaf,
            e.x + (e.width - w) / 2.0,
            e.y + 36.0,
            title,
            font_size::BODY,
            theme.text,
            UiWeight::Medium,
        );
        let sub = format!(
            "Create one to reach a database or web app on {}.",
            state.host_label
        );
        let sw = measure_ui_text(sugarloaf, &sub, font_size::BODY_SM, UiWeight::Regular);
        draw_ui_text(
            sugarloaf,
            e.x + (e.width - sw) / 2.0,
            e.y + 64.0,
            &sub,
            font_size::BODY_SM,
            theme.text_muted,
            UiWeight::Regular,
        );
    }

    for (i, (rect, item)) in l.cards.iter().zip(&state.items).enumerate() {
        if rect.bottom() > content.bottom() {
            break;
        }
        let hovered = matches!(
            state.hover,
            Hover::Card(j) | Hover::Toggle(j) | Hover::Delete(j) if j == i
        );
        let detail = match (&item.status, &item.error) {
            (TunnelStatus::Failed, Some(err)) => format!("Failed \u{2014} {err}"),
            _ => item.route(),
        };
        let toggle = if item.status.is_active() {
            "Stop"
        } else {
            "Start"
        };
        let actions = [Action::Secondary(toggle), Action::QuietTrash];
        paint_card(
            sugarloaf,
            theme,
            *rect,
            if hovered {
                CardState::Hover
            } else {
                CardState::Default
            },
            &CardContent {
                title: &item.name,
                detail: &detail,
                meta: item.kind.label(),
                dot: Some(if item.status == TunnelStatus::Running {
                    DotKind::Running
                } else {
                    DotKind::Idle
                }),
                actions: &actions,
            },
        );
    }
}

fn field_state(form: &TunnelForm, f: FormField) -> FieldState {
    if form.error_for(f).is_some() {
        FieldState::Error
    } else if form.focus == f {
        FieldState::Focus
    } else if form.value(f).is_empty() {
        FieldState::Default
    } else {
        FieldState::Filled
    }
}

#[allow(clippy::too_many_arguments)]
fn one_field(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    form: &TunnelForm,
    f: FormField,
    layout: &terminus_ui::components::input::FieldLayout,
    kind: FieldKind,
    label: &str,
    placeholder: &str,
) {
    let value = form.value(f);
    // Caret sits after the text before it, not at the end of the value.
    let prefix = form.draft(f).prefix_display();
    let caret = (form.focus == f).then(|| {
        if kind == FieldKind::Mono {
            measure_mono_text(sugarloaf, &prefix, kind.value_font(), UiWeight::Regular)
        } else {
            measure_ui_text(sugarloaf, &prefix, kind.value_font(), UiWeight::Regular)
        }
    });
    let scale = sugarloaf.scale_factor();
    paint_field(
        sugarloaf,
        theme,
        layout,
        &FieldContent {
            kind,
            state: field_state(form, f),
            label: Some(label),
            value,
            placeholder,
            helper: None,
            revealed: false,
            caret_prefix_width: caret,
        },
        theme.dialog,
        scale,
    );
}

fn paint_dialog(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &TunnelsState,
    form: &TunnelForm,
    m: &Metrics,
) {
    let d = dialog_layout(content, form, m);
    paint_surface_stroke(
        sugarloaf,
        &d.dialog,
        theme.dialog,
        Some(theme.dialog_line),
        radius::DIALOG,
        1.0,
        D_PANEL,
        ORDER,
        false,
    );
    draw_ui_text(
        sugarloaf,
        d.title.x,
        d.title.y,
        form.title(),
        24.0,
        theme.text,
        UiWeight::SemiBold,
    );
    let names = TunnelKind::ALL.map(|k| k.label());
    paint_segmented(
        sugarloaf,
        theme,
        (d.segments.track.x, d.segments.track.y),
        &names,
        state.form_kind_index(),
        SegmentedSize::Medium,
    );
    one_field(
        sugarloaf,
        theme,
        form,
        FormField::Name,
        &d.name,
        FieldKind::Text,
        "Name",
        "Database",
    );
    one_field(
        sugarloaf,
        theme,
        form,
        FormField::LocalPort,
        &d.local,
        FieldKind::Mono,
        form.local_label(),
        "5432",
    );
    if let (Some(a), Some(dest), Some(port)) = (d.arrow, d.dest, d.port) {
        let aw =
            measure_mono_text(sugarloaf, "\u{2192}", font_size::BODY, UiWeight::Regular);
        draw_mono_text(
            sugarloaf,
            a.x + (a.width - aw) / 2.0,
            text_top(&a, font_size::BODY),
            "\u{2192}",
            font_size::BODY,
            theme.text_muted,
            UiWeight::Regular,
        );
        one_field(
            sugarloaf,
            theme,
            form,
            FormField::DestHost,
            &dest,
            FieldKind::Mono,
            &form.dest_label(),
            "localhost",
        );
        one_field(
            sugarloaf,
            theme,
            form,
            FormField::DestPort,
            &port,
            FieldKind::Mono,
            "Port",
            "5432",
        );
    }
    if let (Some(r), Some(err)) = (d.error_line, form.first_error()) {
        draw_ui_text(
            sugarloaf,
            r.x,
            text_top(&r, font_size::CAPTION),
            err,
            font_size::CAPTION,
            theme.danger_text,
            UiWeight::Regular,
        );
    }
    let layer = Layer {
        order: ORDER,
        depth: D_CTRL,
        backdrop: theme.dialog,
    };
    let cancel = label_spec(
        sugarloaf,
        (d.cancel.x, d.cancel.y),
        ButtonKind::Secondary,
        ButtonSize::Large,
        "Cancel",
        false,
    );
    let cs = if state.hover == Hover::Cancel {
        ButtonState::Hover
    } else {
        ButtonState::Default
    };
    paint_button_on(sugarloaf, theme, &cancel, cs, "Cancel", None, layer);
    let label = confirm_label(state);
    let confirm = label_spec(
        sugarloaf,
        (d.confirm.x, d.confirm.y),
        ButtonKind::Primary,
        ButtonSize::Large,
        label,
        false,
    );
    let ps = if state.hover == Hover::Confirm {
        ButtonState::Hover
    } else {
        ButtonState::Default
    };
    paint_button_on(sugarloaf, theme, &confirm, ps, label, None, layer);
}

// ---------------------------------------------------------------- preview

/// Content rect used by the preview: window minus a 260px left strip and a
/// 96px top strip.
pub fn preview_content(window: Rect) -> Rect {
    Rect::new(
        260.0,
        96.0,
        (window.width - 260.0).max(0.0),
        (window.height - 96.0).max(0.0),
    )
}

fn preview_item(
    id: &str,
    name: &str,
    kind: TunnelKind,
    port: u16,
    dest: (&str, u16),
    status: TunnelStatus,
    error: Option<&str>,
) -> TunnelItem {
    TunnelItem {
        id: id.into(),
        name: name.into(),
        kind,
        bind_host: "127.0.0.1".into(),
        bind_port: port,
        dest_host: dest.0.into(),
        dest_port: dest.1,
        status,
        error: error.map(String::from),
    }
}

fn preview_state(variant: &str) -> TunnelsState {
    let mut s = TunnelsState::new("jerem prod");
    if variant != "empty" {
        s.items = vec![
            preview_item(
                "1",
                "Database",
                TunnelKind::Local,
                5432,
                ("localhost", 5432),
                TunnelStatus::Running,
                None,
            ),
            preview_item(
                "2",
                "Web app",
                TunnelKind::Local,
                8080,
                ("localhost", 80),
                TunnelStatus::Stopped,
                None,
            ),
            preview_item(
                "3",
                "Webhooks",
                TunnelKind::Remote,
                9000,
                ("localhost", 3000),
                TunnelStatus::Failed,
                Some("The server refused the remote port forward"),
            ),
            preview_item(
                "4",
                "SOCKS proxy",
                TunnelKind::Dynamic,
                1080,
                ("", 0),
                TunnelStatus::Stopped,
                None,
            ),
        ];
    }
    match variant {
        "new" => s.open_new(),
        "error" => {
            s.open_new();
            if let Some(f) = s.form.as_mut() {
                f.local_port = TextDraft::new("0");
                f.dest_port = TextDraft::new("70000");
                let _ = f.validate(&|_| true);
                f.focus = FormField::LocalPort;
            }
        }
        "edit" => s.open_edit(2),
        _ => {}
    }
    s
}

thread_local! {
    static PREVIEW: RefCell<Option<TunnelsState>> = const { RefCell::new(None) };
}

/// `TERMINUS_VIEW_PREVIEW=tunnels[:new|:error|:edit|:empty]`.
fn preview_variant() -> Option<&'static str> {
    static V: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        let v = std::env::var("TERMINUS_VIEW_PREVIEW").ok()?;
        let v = v.trim().to_string();
        (v == "tunnels" || v.starts_with("tunnels:")).then_some(v)
    })
    .as_deref()
    .map(|v| v.strip_prefix("tunnels:").unwrap_or(""))
}

/// Paint the preview full-window if `TERMINUS_VIEW_PREVIEW` selects this
/// view; returns whether it did.
pub fn paint_preview_if_selected(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme) -> bool {
    let Some(variant) = preview_variant() else {
        return false;
    };
    let scale = sugarloaf.scale_factor();
    let size = sugarloaf.window_size();
    let window = Rect::new(0.0, 0.0, size.width / scale, size.height / scale);
    crate::renderer::ui_text::sync_ui_fonts(sugarloaf);
    paint_flat(sugarloaf, &window, theme.canvas, 0.0, 0);
    PREVIEW.with(|p| {
        let mut p = p.borrow_mut();
        let state = p.get_or_insert_with(|| preview_state(variant));
        paint(sugarloaf, theme, window, preview_content(window), state);
    });
    true
}
