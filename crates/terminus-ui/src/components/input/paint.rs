use super::TextDraft;

/// Paint model for a labeled text field card (Settings, SFTP name, …).
///
/// Carries everything a painter needs to place the caret and draw the
/// selection wash: the text being shown, the display-space prefix that
/// precedes the caret, and the selected display range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPaint {
    pub text: String,
    pub placeholder: bool,
    pub show_caret: bool,
    /// Display text from the line start up to the caret. The caret is
    /// painted immediately after it, so it moves with the editing model
    /// instead of being pinned to the end of the value.
    pub caret_prefix: String,
    /// Selected display range while focused — `None` when collapsed or
    /// when the field does not own the caret.
    pub selection: Option<(usize, usize)>,
}

impl FieldPaint {
    /// Build paint state from a [`TextDraft`].
    pub fn from_draft(draft: &TextDraft, placeholder: &str, focused: bool) -> Self {
        if draft.value.is_empty() && !focused {
            Self {
                text: placeholder.to_string(),
                placeholder: true,
                show_caret: false,
                caret_prefix: String::new(),
                selection: None,
            }
        } else if draft.value.is_empty() {
            Self {
                text: String::new(),
                placeholder: false,
                show_caret: focused,
                caret_prefix: String::new(),
                selection: None,
            }
        } else {
            Self {
                text: draft.display_line(),
                placeholder: false,
                show_caret: focused,
                caret_prefix: draft.prefix_display(),
                selection: if focused {
                    draft.selection_range()
                } else {
                    None
                },
            }
        }
    }

    /// [Self::from_draft] with every visible character replaced by a
    /// bullet — used by secret fields. Lengths are preserved so the caret
    /// prefix and the selection range stay aligned with the real draft.
    pub fn from_draft_masked(
        draft: &TextDraft,
        placeholder: &str,
        focused: bool,
        mask: bool,
    ) -> Self {
        let mut paint = Self::from_draft(draft, placeholder, focused);
        if mask && !paint.placeholder && !draft.value.is_empty() {
            let len = draft.value.chars().count();
            paint.text = "•".repeat(len);
            paint.caret_prefix = "•".repeat(draft.caret.min(len));
        }
        paint
    }

    /// Idle / focused paint for a plain `String` field (Settings URI, …).
    pub fn from_value(value: &str, placeholder: &str, focused: bool) -> Self {
        if value.is_empty() && !focused {
            Self {
                text: placeholder.to_string(),
                placeholder: true,
                show_caret: false,
                caret_prefix: String::new(),
                selection: None,
            }
        } else {
            Self {
                text: value.to_string(),
                placeholder: false,
                show_caret: focused,
                caret_prefix: value.to_string(),
                selection: None,
            }
        }
    }
}
