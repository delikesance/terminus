//! Destructive confirm dialog drawn over the Settings content rect.

use super::{button_spec, Measure};
use crate::components::button::{ButtonKind, ButtonSize};
use crate::components::overlay::{
    self as ov, dialog_layout_at, wrap_text, DialogFocus, DialogHit, DialogKind,
    DialogLayout,
};
use crate::geom::Rect;

/// A destructive confirm dialog shown over the content rect.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub confirm: String,
    pub cancel: String,
    pub focus: DialogFocus,
}

impl Confirm {
    pub fn destructive(
        title: impl Into<String>,
        body: impl Into<String>,
        confirm: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            confirm: confirm.into(),
            cancel: "Cancel".into(),
            focus: DialogKind::Destructive.default_focus(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmLayout {
    pub dialog: DialogLayout,
    pub lines: Vec<String>,
}

/// Dialog centred in `content`; the scrim covers only `content`.
pub fn confirm_layout(content: Rect, m: Measure, c: &Confirm) -> ConfirmLayout {
    let inner = ov::DIALOG_WIDTH - 2.0 * ov::DIALOG_PAD;
    let lines = wrap_text(&c.body, inner, |s| m(s, 14.0, false));
    let cancel_w = confirm_button_width(m, ButtonKind::Secondary, &c.cancel);
    let confirm_w = confirm_button_width(m, ButtonKind::Danger, &c.confirm);
    // Height does not depend on x/y: build once to read it, then centre.
    let probe = dialog_layout_at(
        0.0,
        0.0,
        DialogKind::Destructive,
        lines.len(),
        cancel_w,
        confirm_w,
        (content.width, content.height),
    );
    let x = (content.x + (content.width - ov::DIALOG_WIDTH) / 2.0).round();
    let y = (content.y + (content.height - probe.dialog.height) / 2.0).round();
    let mut dialog = dialog_layout_at(
        x,
        y,
        DialogKind::Destructive,
        lines.len(),
        cancel_w,
        confirm_w,
        (content.width, content.height),
    );
    dialog.scrim = content;
    ConfirmLayout { dialog, lines }
}

fn confirm_button_width(m: Measure, kind: ButtonKind, label: &str) -> f32 {
    button_spec(m, (0.0, 0.0), kind, ButtonSize::Large, label).width()
}

/// Outcome of a press while a confirm dialog is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmHit {
    Confirm,
    Cancel,
    /// Inside the dialog, on nothing: swallow.
    Inside,
}

pub fn confirm_hit(layout: &ConfirmLayout, x: f32, y: f32) -> ConfirmHit {
    match layout.dialog.hit_test(x, y) {
        DialogHit::Confirm => ConfirmHit::Confirm,
        DialogHit::Cancel | DialogHit::Scrim => ConfirmHit::Cancel,
        _ => ConfirmHit::Inside,
    }
}
