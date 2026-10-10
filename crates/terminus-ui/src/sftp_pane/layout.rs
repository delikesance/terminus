use super::*;
use crate::confirm::ConfirmLayout;
use crate::geom::Rect;
use crate::settings::{field_input_in_card, FIELD_CARD_HEIGHT};

/// Computed geometry for one paint / hit-test pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SftpPaneLayout {
    pub bounds: Rect,
    pub toolbar: Rect,
    pub btn_close: Rect,
    pub name_field: Rect,
    pub name_confirm: Rect,
    pub name_cancel: Rect,
    pub left: Rect,
    pub right: Rect,
    pub left_header: Rect,
    pub right_header: Rect,
    pub left_parent_btn: Rect,
    pub right_parent_btn: Rect,
    pub left_list: Rect,
    pub right_list: Rect,
    pub footer: Rect,
}

impl SftpPaneLayout {
    /// Build layout inside the leaf grid's `layout_rect` (logical pixels).
    pub fn from_bounds(bounds: Rect) -> Self {
        Self::with_name_edit(bounds, false)
    }

    /// Layout for an active SFTP session (expands toolbar while naming).
    pub fn from_state(bounds: Rect, state: &SftpPaneState) -> Self {
        Self::with_name_edit(bounds, state.name_edit.is_some())
    }

    /// Build layout; when `name_editing`, toolbar grows for the shared field card.
    pub fn with_name_edit(bounds: Rect, name_editing: bool) -> Self {
        const TOOLBAR_PAD: f32 = 4.0;
        let toolbar_h = if name_editing {
            FIELD_CARD_HEIGHT + 2.0 * TOOLBAR_PAD
        } else {
            TOOLBAR_HEIGHT
        };
        let toolbar = Rect::new(bounds.x, bounds.y, bounds.width, toolbar_h);
        let footer = Rect::new(
            bounds.x,
            bounds.bottom() - FOOTER_HEIGHT,
            bounds.width,
            FOOTER_HEIGHT,
        );

        let btn_y = toolbar.y + (toolbar_h - BTN_SIZE) * 0.5;
        let close_w = 96.0;
        let btn_close = Rect::new(
            toolbar.right() - PANE_PAD - close_w,
            btn_y,
            close_w,
            BTN_SIZE,
        );

        // Name editor spans left of Close (shared field card).
        let name_actions_w = BTN_SIZE * 2.0 + BTN_GAP * 2.0;
        let name_left = toolbar.x + PANE_PAD;
        let name_right = btn_close.x - BTN_GAP - name_actions_w;
        let name_field = if name_editing {
            Rect::new(
                name_left,
                toolbar.y + TOOLBAR_PAD,
                (name_right - name_left).max(120.0),
                FIELD_CARD_HEIGHT,
            )
        } else {
            Rect::new(name_left, btn_y, 0.0, BTN_SIZE)
        };
        let name_btn_y = if name_editing {
            let input = field_input_in_card(name_field);
            input.y + (input.height - BTN_SIZE) * 0.5
        } else {
            btn_y
        };
        let name_cancel = Rect::new(
            btn_close.x - BTN_GAP - BTN_SIZE,
            name_btn_y,
            BTN_SIZE,
            BTN_SIZE,
        );
        let name_confirm = Rect::new(
            name_cancel.x - BTN_GAP - BTN_SIZE,
            name_btn_y,
            BTN_SIZE,
            BTN_SIZE,
        );

        let body_top = bounds.y + toolbar_h;
        let body_h = (footer.y - body_top).max(0.0);
        let pane_w = ((bounds.width - PANE_GAP) * 0.5).max(0.0);
        let left = Rect::new(bounds.x, body_top, pane_w, body_h);
        let right = Rect::new(bounds.x + pane_w + PANE_GAP, body_top, pane_w, body_h);
        let left_header = Rect::new(left.x, left.y, left.width, HEADER_HEIGHT);
        let right_header = Rect::new(right.x, right.y, right.width, HEADER_HEIGHT);
        let parent_pad = (HEADER_HEIGHT - BTN_SIZE) * 0.5;
        let left_parent_btn = Rect::new(
            left_header.x + PANE_PAD,
            left_header.y + parent_pad,
            BTN_SIZE,
            BTN_SIZE,
        );
        let right_parent_btn = Rect::new(
            right_header.x + PANE_PAD,
            right_header.y + parent_pad,
            BTN_SIZE,
            BTN_SIZE,
        );
        let list_h = (body_h - HEADER_HEIGHT).max(0.0);
        let left_list = Rect::new(left.x, left.y + HEADER_HEIGHT, left.width, list_h);
        let right_list = Rect::new(right.x, right.y + HEADER_HEIGHT, right.width, list_h);
        Self {
            bounds,
            toolbar,
            btn_close,
            name_field,
            name_confirm,
            name_cancel,
            left,
            right,
            left_header,
            right_header,
            left_parent_btn,
            right_parent_btn,
            left_list,
            right_list,
            footer,
        }
    }

    /// Row rect for `index` in the left list (unclipped; may sit above/below).
    pub fn left_row_rect(&self, index: usize, scroll: f32) -> Rect {
        row_rect(self.left_list, index, scroll)
    }

    pub fn right_row_rect(&self, index: usize, scroll: f32) -> Rect {
        row_rect(self.right_list, index, scroll)
    }

    pub fn hit_test(&self, state: &SftpPaneState, x: f32, y: f32) -> SftpHit {
        if !self.bounds.contains(x, y) {
            return SftpHit::Miss;
        }
        if let Some(prompt) = state.conflict.as_ref() {
            use crate::components::overlay::DialogHit;
            // Scrim and dialog body swallow clicks while a conflict is open.
            return match self.conflict_layout(prompt).hit_test(x, y) {
                DialogHit::Confirm => SftpHit::ConflictOverwrite,
                DialogHit::Cancel => SftpHit::ConflictKeep,
                DialogHit::Option => SftpHit::ConflictApplyAll,
                DialogHit::Inside | DialogHit::Scrim => SftpHit::Consume,
            };
        }
        if state.name_edit.is_some() {
            if self.name_confirm.contains(x, y) {
                return SftpHit::NameConfirm;
            }
            if self.name_cancel.contains(x, y) {
                return SftpHit::NameCancel;
            }
            if self.name_field.contains(x, y) {
                return SftpHit::NameField;
            }
            if self.btn_close.contains(x, y) {
                return SftpHit::Close;
            }
            if self.toolbar.contains(x, y) {
                return SftpHit::Consume;
            }
        }
        if self.btn_close.contains(x, y) {
            return SftpHit::Close;
        }
        if self.toolbar.contains(x, y) {
            return SftpHit::Consume;
        }
        if self.footer.contains(x, y) {
            return SftpHit::Footer;
        }
        if self.left_parent_btn.contains(x, y) {
            return SftpHit::LeftParent;
        }
        if self.right_parent_btn.contains(x, y) {
            return SftpHit::RightParent;
        }
        if self.left_header.contains(x, y) {
            return SftpHit::LeftCrumb;
        }
        if self.right_header.contains(x, y) {
            return SftpHit::RightCrumb;
        }
        if self.left_list.contains(x, y) {
            if let Some(i) = row_index_at(self.left_list, y, state.left.scroll) {
                if i < state.left.entries.len() {
                    return SftpHit::LeftRow(i);
                }
            }
            return SftpHit::Consume;
        }
        if self.right_list.contains(x, y) {
            if let Some(i) = row_index_at(self.right_list, y, state.right.scroll) {
                if i < state.right.entries.len() {
                    return SftpHit::RightRow(i);
                }
            }
            return SftpHit::Consume;
        }
        SftpHit::Consume
    }

    /// The conflict dialog, centred in the pane and scrimming only it.
    pub fn conflict_layout(&self, prompt: &SftpConflictPrompt) -> ConfirmLayout {
        prompt.spec().layout_in(self.bounds)
    }

    /// Which list (if any) contains `(x, y)` — used for drag-drop targets.
    pub fn focus_at_list(&self, x: f32, y: f32) -> Option<SftpFocus> {
        if self.left_list.contains(x, y) || self.left.contains(x, y) {
            Some(SftpFocus::Left)
        } else if self.right_list.contains(x, y) || self.right.contains(x, y) {
            Some(SftpFocus::Right)
        } else {
            None
        }
    }
}

fn row_rect(list: Rect, index: usize, scroll: f32) -> Rect {
    Rect::new(
        list.x,
        list.y + index as f32 * ROW_HEIGHT - scroll,
        list.width,
        ROW_HEIGHT,
    )
}

fn row_index_at(list: Rect, y: f32, scroll: f32) -> Option<usize> {
    if y < list.y || y >= list.bottom() {
        return None;
    }
    let rel = y - list.y + scroll;
    if rel < 0.0 {
        return None;
    }
    Some((rel / ROW_HEIGHT) as usize)
}
