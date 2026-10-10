use crate::geom::Rect;

pub const DIALOG_WIDTH: f32 = 440.0;
pub const DIALOG_PAD: f32 = 30.0;
/// Vertical gap between title block, option row and actions.
pub const DIALOG_GAP: f32 = 18.0;
pub const TITLE_LINE: f32 = 28.0;
pub const TITLE_BODY_GAP: f32 = 8.0;
/// 14px body at line-height 1.5 (rounded to a whole pixel run of 21).
pub const BODY_LINE: f32 = 21.0;
pub const OPTION_HEIGHT: f32 = 20.0;
pub const ACTION_HEIGHT: f32 = 44.0;
pub const ACTION_GAP: f32 = 10.0;
/// Horizontal padding inside an action button (text is measured by the painter).
pub const ACTION_PAD_X: f32 = 20.0;
/// Scrim colour, rgba(6,5,10,0.64).
pub const SCRIM: [f32; 4] = [6.0 / 255.0, 5.0 / 255.0, 10.0 / 255.0, 0.64];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Confirm,
    Destructive,
    WithOption,
}

impl DialogKind {
    /// Destructive dialogs open with Cancel focused so a stray Enter is safe.
    pub fn default_focus(self) -> DialogFocus {
        match self {
            DialogKind::Destructive => DialogFocus::Cancel,
            _ => DialogFocus::Confirm,
        }
    }

    pub fn has_option(self) -> bool {
        self == DialogKind::WithOption
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogFocus {
    Cancel,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKey {
    Escape,
    Enter,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogOutcome {
    Cancel,
    Confirm,
    Focus(DialogFocus),
}

/// Esc always cancels; Enter activates the focused button; Tab swaps focus.
pub fn dialog_key(key: DialogKey, focus: DialogFocus) -> DialogOutcome {
    match key {
        DialogKey::Escape => DialogOutcome::Cancel,
        DialogKey::Enter => match focus {
            DialogFocus::Cancel => DialogOutcome::Cancel,
            DialogFocus::Confirm => DialogOutcome::Confirm,
        },
        DialogKey::Tab => DialogOutcome::Focus(match focus {
            DialogFocus::Cancel => DialogFocus::Confirm,
            DialogFocus::Confirm => DialogFocus::Cancel,
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogHit {
    Confirm,
    Cancel,
    Option,
    Inside,
    Scrim,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogLayout {
    pub scrim: Rect,
    pub dialog: Rect,
    pub title: Rect,
    pub body: Rect,
    pub option: Option<Rect>,
    pub cancel: Rect,
    pub confirm: Rect,
}

impl DialogLayout {
    pub fn hit_test(&self, x: f32, y: f32) -> DialogHit {
        if self.confirm.contains(x, y) {
            DialogHit::Confirm
        } else if self.cancel.contains(x, y) {
            DialogHit::Cancel
        } else if self.option.is_some_and(|r| r.contains(x, y)) {
            DialogHit::Option
        } else if self.dialog.contains(x, y) {
            DialogHit::Inside
        } else {
            DialogHit::Scrim
        }
    }
}

/// Width of an action button for a label of `text_width` px.
pub fn action_width(text_width: f32) -> f32 {
    text_width + 2.0 * ACTION_PAD_X
}

/// Dialog centred in `window`. `cancel_w`/`confirm_w` are button widths
/// (see [`action_width`]); `body_lines` is the wrapped line count.
pub fn dialog_layout(
    window: (f32, f32),
    kind: DialogKind,
    body_lines: usize,
    cancel_w: f32,
    confirm_w: f32,
) -> DialogLayout {
    let body_h = body_lines.max(1) as f32 * BODY_LINE;
    let option_h = if kind.has_option() {
        DIALOG_GAP + OPTION_HEIGHT
    } else {
        0.0
    };
    let height = DIALOG_PAD * 2.0
        + TITLE_LINE
        + TITLE_BODY_GAP
        + body_h
        + option_h
        + DIALOG_GAP
        + ACTION_HEIGHT;
    let x = ((window.0 - DIALOG_WIDTH) / 2.0).round();
    let y = ((window.1 - height) / 2.0).round();
    dialog_layout_at(x, y, kind, body_lines, cancel_w, confirm_w, window)
}

/// Same as [`dialog_layout`] with an explicit top-left (gallery use).
pub fn dialog_layout_at(
    x: f32,
    y: f32,
    kind: DialogKind,
    body_lines: usize,
    cancel_w: f32,
    confirm_w: f32,
    window: (f32, f32),
) -> DialogLayout {
    let body_h = body_lines.max(1) as f32 * BODY_LINE;
    let option_h = if kind.has_option() {
        DIALOG_GAP + OPTION_HEIGHT
    } else {
        0.0
    };
    let height = DIALOG_PAD * 2.0
        + TITLE_LINE
        + TITLE_BODY_GAP
        + body_h
        + option_h
        + DIALOG_GAP
        + ACTION_HEIGHT;
    let dialog = Rect::new(x, y, DIALOG_WIDTH, height);
    let inner_x = x + DIALOG_PAD;
    let inner_w = DIALOG_WIDTH - 2.0 * DIALOG_PAD;
    let title = Rect::new(inner_x, y + DIALOG_PAD, inner_w, TITLE_LINE);
    let body = Rect::new(inner_x, title.bottom() + TITLE_BODY_GAP, inner_w, body_h);
    let option = kind
        .has_option()
        .then(|| Rect::new(inner_x, body.bottom() + DIALOG_GAP, inner_w, OPTION_HEIGHT));
    let (cancel, confirm) = action_row(
        &dialog,
        DIALOG_PAD,
        ACTION_HEIGHT,
        ACTION_GAP,
        cancel_w,
        confirm_w,
    );
    DialogLayout {
        scrim: Rect::new(0.0, 0.0, window.0, window.1),
        dialog,
        title,
        body,
        option,
        cancel,
        confirm,
    }
}

/// `(cancel, confirm)` buttons right-aligned on the bottom edge of `dialog`.
pub fn action_row(
    dialog: &Rect,
    pad: f32,
    height: f32,
    gap: f32,
    cancel_w: f32,
    confirm_w: f32,
) -> (Rect, Rect) {
    let y = dialog.bottom() - pad - height;
    let confirm = Rect::new(dialog.right() - pad - confirm_w, y, confirm_w, height);
    let cancel = Rect::new(confirm.x - gap - cancel_w, y, cancel_w, height);
    (cancel, confirm)
}

/// Greedy word wrap using a caller supplied width measure. A word wider
/// than `max_width` gets its own line.
pub fn wrap_text(
    text: &str,
    max_width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
            continue;
        }
        let candidate = format!("{cur} {word}");
        if measure(&candidate) <= max_width {
            cur = candidate;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    lines.push(cur);
    lines
}
