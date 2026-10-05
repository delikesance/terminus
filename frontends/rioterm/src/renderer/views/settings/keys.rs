//! SSH keys tab painter.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonState};
use terminus_ui::components::input::{self as ui, FieldKind, FieldState};
use terminus_ui::components::list::{CardState, CARD_RADIUS};
use terminus_ui::components::overlay::{self as ov, DialogFocus, DialogKind};
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::font_size;
use terminus_ui::views::settings::keys::{
    DraftField, KeyDraft, KeysLayout, KeysState, KeysTarget, INTRO,
};

use super::with_measure;
use crate::renderer::chrome::paint_flat;
use crate::renderer::components::button::{label_spec, paint_button};
use crate::renderer::components::input::{paint_field, FieldContent};
use crate::renderer::components::list::{paint_card, Action, CardContent};
use crate::renderer::components::overlay::{paint_dialog, DialogSpec};
use crate::renderer::ui_text::{
    draw_ui_text, measure_mono_text, measure_ui_text, UiWeight,
};

pub(super) fn text_top(center: f32, size: f32) -> f32 {
    center - size * 0.62
}

/// Longest tail of `text` that fits `max_w`, prefixed with an ellipsis when cut.
pub(super) fn fit_tail(
    text: &str,
    max_w: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> String {
    if measure(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let s: String = std::iter::once('\u{2026}')
            .chain(chars[start..].iter().copied())
            .collect();
        if measure(&s) <= max_w {
            return s;
        }
        start += 1;
    }
    String::new()
}

fn button_state(state: &KeysState, t: KeysTarget) -> ButtonState {
    ButtonState::resolve(state.hover == Some(t), false, false, false)
}

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &KeysState,
) {
    let layout = with_measure(sugarloaf, |m| state.layout(content, m));
    paint_layout(sugarloaf, theme, state, &layout);
}

fn paint_layout(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    state: &KeysState,
    l: &KeysLayout,
) {
    let scale = sugarloaf.scale_factor();
    draw_ui_text(
        sugarloaf,
        l.intro.x,
        text_top(l.intro.y + l.intro.height / 2.0, font_size::BODY_SM),
        INTRO,
        font_size::BODY_SM,
        theme.text_muted,
        UiWeight::Regular,
    );
    let import = label_spec(
        sugarloaf,
        (l.import.x, l.import.y),
        ButtonKind::Secondary,
        ButtonSize::Medium,
        "Import",
        false,
    );
    paint_button(
        sugarloaf,
        theme,
        &import,
        button_state(state, KeysTarget::Import),
        "Import",
        None,
    );
    let gen = label_spec(
        sugarloaf,
        (l.generate.x, l.generate.y),
        ButtonKind::Primary,
        ButtonSize::Medium,
        "Generate key",
        false,
    );
    paint_button(
        sugarloaf,
        theme,
        &gen,
        button_state(state, KeysTarget::Generate),
        "Generate key",
        None,
    );

    if let (Some(d), Some(dl)) = (state.draft.as_ref(), l.draft.as_ref()) {
        paint_draft(sugarloaf, theme, state, d, dl, scale);
    }

    for (i, (key, row)) in state.keys.iter().zip(&l.rows).enumerate() {
        let hovered = matches!(
            state.hover,
            Some(KeysTarget::Copy(j)) | Some(KeysTarget::Delete(j)) if j == i
        );
        paint_card(
            sugarloaf,
            theme,
            row.card.rect,
            if hovered {
                CardState::Hover
            } else {
                CardState::Default
            },
            &CardContent {
                title: &key.name,
                detail: &key.fingerprint,
                meta: &key.created,
                dot: None,
                actions: &[Action::Secondary("Copy public key"), Action::QuietTrash],
            },
        );
    }

    if let Some(r) = l.empty {
        draw_ui_text(
            sugarloaf,
            r.x,
            text_top(r.y + r.height / 2.0, font_size::BODY_SM),
            "No keys yet. Generate one, or import a key you already use.",
            font_size::BODY_SM,
            theme.text_faint,
            UiWeight::Regular,
        );
    }
    if let (Some(r), Some(n)) = (l.notice, state.notice.as_ref()) {
        draw_ui_text(
            sugarloaf,
            r.x,
            text_top(r.y + r.height / 2.0, ui::HELPER_FONT),
            n,
            ui::HELPER_FONT,
            theme.text_muted,
            UiWeight::Regular,
        );
    }

    if let (Some(cl), Some(dialog)) = (l.confirm.as_ref(), state.delete_dialog()) {
        sugarloaf.begin_overlay();
        paint_flat(sugarloaf, &cl.dialog.scrim, ov::SCRIM, 0.08, 30);
        paint_dialog(
            sugarloaf,
            theme,
            &DialogSpec {
                kind: DialogKind::Destructive,
                title: &dialog.title,
                body: &dialog.body,
                confirm: &dialog.confirm,
                cancel: &dialog.cancel,
                option: None,
                option_checked: false,
            },
            &cl.dialog,
            &cl.lines,
            state.delete_focus().or(Some(DialogFocus::Cancel)),
        );
        sugarloaf.end_overlay();
    }
}

fn paint_draft(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    state: &KeysState,
    d: &KeyDraft,
    dl: &terminus_ui::views::settings::keys::DraftLayout,
    scale: f32,
) {
    sugarloaf.rounded_rect(
        None,
        dl.card.x,
        dl.card.y,
        dl.card.width,
        dl.card.height,
        theme.frame,
        0.1,
        CARD_RADIUS,
        0,
    );
    draw_ui_text(
        sugarloaf,
        dl.title.x,
        text_top(dl.title.y + dl.title.height / 2.0, font_size::BODY),
        d.title(),
        font_size::BODY,
        theme.text,
        UiWeight::SemiBold,
    );
    for (field, fl) in &dl.fields {
        let (draft, label, placeholder, kind, masked) = match field {
            DraftField::Name => {
                (&d.name, "Name", "id_ed25519_laptop", FieldKind::Text, false)
            }
            DraftField::Pem => (
                &d.pem,
                "Private key",
                "-----BEGIN OPENSSH PRIVATE KEY-----",
                FieldKind::Textarea,
                false,
            ),
            DraftField::Passphrase => (
                &d.passphrase,
                "Passphrase (only if the key is encrypted)",
                "",
                FieldKind::Text,
                true,
            ),
        };
        let focused = d.focus == *field;
        let shown_full = if masked {
            ui::mask(draft.value.chars().count())
        } else {
            draft.value.replace(['\r', '\n'], " ")
        };
        let max_w = fl.text.width.max(10.0);
        let shown = if focused || !shown_full.is_empty() {
            fit_tail(&shown_full, max_w, |s| {
                if kind == FieldKind::Textarea {
                    measure_mono_text(sugarloaf, s, kind.value_font(), UiWeight::Regular)
                } else {
                    measure_ui_text(sugarloaf, s, kind.value_font(), UiWeight::Regular)
                }
            })
        } else {
            String::new()
        };
        // The caret sits at the end of what is shown (the tail the user types in).
        let caret = (focused && draft.caret >= draft.value.chars().count()).then(|| {
            if kind == FieldKind::Textarea {
                measure_mono_text(sugarloaf, &shown, kind.value_font(), UiWeight::Regular)
            } else {
                measure_ui_text(sugarloaf, &shown, kind.value_font(), UiWeight::Regular)
            }
        });
        let fstate = if focused {
            FieldState::Focus
        } else if state.hover == Some(KeysTarget::Field(*field)) {
            FieldState::Hover
        } else {
            FieldState::Default
        };
        paint_field(
            sugarloaf,
            theme,
            fl,
            &FieldContent {
                kind,
                state: fstate,
                label: Some(label),
                value: &shown,
                placeholder,
                helper: None,
                revealed: false,
                caret_prefix_width: caret,
            },
            theme.frame,
            scale,
        );
    }
    if let (Some(r), Some(e)) = (dl.error, d.error.as_ref()) {
        draw_ui_text(
            sugarloaf,
            r.x,
            text_top(r.y + r.height / 2.0, ui::HELPER_FONT),
            e,
            ui::HELPER_FONT,
            theme.danger_text,
            UiWeight::Regular,
        );
    }
    let cancel = label_spec(
        sugarloaf,
        (dl.cancel.x, dl.cancel.y),
        ButtonKind::Secondary,
        ButtonSize::Medium,
        "Cancel",
        false,
    );
    paint_button(
        sugarloaf,
        theme,
        &cancel,
        button_state(state, KeysTarget::Cancel),
        "Cancel",
        None,
    );
    let submit = label_spec(
        sugarloaf,
        (dl.submit.x, dl.submit.y),
        ButtonKind::Primary,
        ButtonSize::Medium,
        d.submit_label(),
        false,
    );
    paint_button(
        sugarloaf,
        theme,
        &submit,
        button_state(state, KeysTarget::Submit),
        d.submit_label(),
        None,
    );
}
