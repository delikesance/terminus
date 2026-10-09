use super::*;
use crate::components::input::{self as inp, FieldLayout};
use crate::components::overlay as ov;
use crate::components::selection as sel;
use crate::geom::Rect;

/// Screen geometry of the dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AddHostLayout {
    /// Top-left of the whole dialog.
    pub x: f32,
    pub y: f32,
}

impl AddHostLayout {
    /// Center the dialog in a `width` x `height` window, in logical pixels.
    /// `anchor_height` is [`AddHostForm::anchor_height`], so every step
    /// shares the same top edge.
    pub fn centered(width: f32, height: f32, anchor_height: f32) -> Self {
        Self {
            x: ((width - WIDTH) / 2.0).max(0.0),
            y: ((height - anchor_height) / 2.0).max(0.0),
        }
    }

    pub fn rect(&self, dialog_height: f32) -> Rect {
        Rect::new(self.x, self.y, WIDTH, dialog_height)
    }

    fn inner_x(&self) -> f32 {
        self.x + PAD_X
    }

    fn inner_width(&self) -> f32 {
        WIDTH - 2.0 * PAD_X
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(
            self.inner_x(),
            self.y + HEADER_TOP,
            self.inner_width() - CLOSE_SIZE,
            CLOSE_SIZE,
        )
    }

    pub fn close_button_rect(&self) -> Rect {
        Rect::new(
            self.x + WIDTH - CLOSE_RIGHT - CLOSE_SIZE,
            self.y + HEADER_TOP,
            CLOSE_SIZE,
            CLOSE_SIZE,
        )
    }

    /// The Overlays stepper under the title.
    pub fn stepper_rect(&self) -> Rect {
        Rect::new(
            self.inner_x(),
            self.y + HEADER_TOP + CLOSE_SIZE + STEPPER_TOP,
            self.inner_width(),
            ov::STEPPER_HEIGHT,
        )
    }

    /// Bar and label of one stepper segment (the click target).
    pub fn step_rect(&self, step: AddHostStep) -> Rect {
        let seg =
            ov::stepper_segments(self.stepper_rect(), step.index() + 1)[step.index()];
        Rect::new(seg.bar.x, seg.bar.y, seg.bar.width, ov::STEPPER_HEIGHT)
    }

    fn body_top(&self) -> f32 {
        self.stepper_rect().bottom() + BODY_PAD
    }

    /// Top of each body row.
    fn row_tops(&self, form: &AddHostForm) -> Vec<(Row, f32)> {
        let mut y = self.body_top();
        form.rows()
            .into_iter()
            .map(|row| {
                let top = y;
                y += row.height() + ROW_GAP;
                (row, top)
            })
            .collect()
    }

    /// Label, box, trailing slot and helper of one field.
    pub fn field_layout(&self, form: &AddHostForm, field: Field) -> Option<FieldLayout> {
        let x = self.inner_x();
        let width = self.inner_width();
        for (row, top) in self.row_tops(form) {
            match row {
                Row::Single(f) if f == field => {
                    return Some(inp::field_layout(
                        (x, top),
                        width,
                        f.kind(),
                        true,
                        f.has_helper(),
                    ));
                }
                Row::Pair(a, b) if a == field || b == field => {
                    let col = (width - 2.0 * PAIR_GAP) / 3.0;
                    let wide = 2.0 * col + PAIR_GAP;
                    let (fx, fw) = if a == field {
                        (x, wide)
                    } else {
                        (x + wide + PAIR_GAP, col)
                    };
                    return Some(inp::field_layout(
                        (fx, top),
                        fw,
                        field.kind(),
                        true,
                        false,
                    ));
                }
                _ => {}
            }
        }
        None
    }

    /// The whole field (label, box, helper).
    pub fn field_rect(&self, form: &AddHostForm, field: Field) -> Option<Rect> {
        self.field_layout(form, field).map(|l| l.total)
    }

    /// The bordered input box.
    pub fn input_rect(&self, form: &AddHostForm, field: Field) -> Option<Rect> {
        self.field_layout(form, field).map(|l| l.box_rect)
    }

    /// Eye toggle on the right of the password box.
    pub fn password_toggle_rect(&self, form: &AddHostForm) -> Option<Rect> {
        let input = self.input_rect(form, Field::Password)?;
        Some(Rect::new(input.right() - 44.0, input.y, 44.0, input.height))
    }

    /// "Sign in with" caption above the cards.
    pub fn choices_label_rect(&self, form: &AddHostForm) -> Option<Rect> {
        self.row_tops(form)
            .into_iter()
            .find(|(r, _)| *r == Row::Choices)
            .map(|(_, top)| {
                Rect::new(self.inner_x(), top, self.inner_width(), inp::LABEL_HEIGHT)
            })
    }

    /// The three sign-in cards (key, password, Kerberos).
    pub fn choice_rects(&self, form: &AddHostForm) -> Option<Vec<Rect>> {
        let label = self.choices_label_rect(form)?;
        Some(sel::choice_row(
            label.x,
            label.bottom() + inp::LABEL_GAP,
            label.width,
            CARD_GAP,
            AUTH_METHODS.len(),
        ))
    }

    /// The hint line under the key select / the Kerberos note.
    pub fn hint_rect(&self, form: &AddHostForm) -> Option<Rect> {
        self.row_tops(form)
            .into_iter()
            .find(|(r, _)| matches!(r, Row::KeyHint | Row::KerberosHint))
            .map(|(_, top)| Rect::new(self.inner_x(), top, self.inner_width(), HINT_LINE))
    }

    /// The "Generate one" link inside the key hint.
    pub fn generate_link_rect(&self, form: &AddHostForm) -> Option<Rect> {
        if !form.shows_identity() {
            return None;
        }
        let hint = self.hint_rect(form)?;
        Some(Rect::new(
            hint.x + HINT_LINK_X,
            hint.y,
            HINT_LINK_W,
            hint.height,
        ))
    }

    /// The open select's list, under its box.
    pub fn menu_rect(&self, form: &AddHostForm) -> Option<Rect> {
        let field = match form.menu()? {
            SelectMenu::Identity => Field::Identity,
            SelectMenu::Group => Field::Group,
        };
        let input = self.input_rect(form, field)?;
        let rows = form.menu_len().max(1) as f32;
        Some(Rect::new(
            input.x,
            input.bottom() + 4.0,
            input.width,
            rows * MENU_ROW + 2.0 * MENU_PAD,
        ))
    }

    pub fn menu_option_rect(&self, form: &AddHostForm, index: usize) -> Option<Rect> {
        if index >= form.menu_len() {
            return None;
        }
        let menu = self.menu_rect(form)?;
        Some(Rect::new(
            menu.x + MENU_PAD,
            menu.y + MENU_PAD + index as f32 * MENU_ROW,
            menu.width - 2.0 * MENU_PAD,
            MENU_ROW,
        ))
    }

    fn button_y(&self, dialog_height: f32) -> f32 {
        self.y + dialog_height - FOOTER_BOTTOM - BUTTON_HEIGHT
    }

    /// Continue / Save server, right aligned.
    pub fn primary_button_rect(&self, form: &AddHostForm) -> Rect {
        let label = if form.step() == AddHostStep::Details {
            LABEL_SAVE
        } else {
            LABEL_CONTINUE
        };
        let w = ov::action_width(label);
        Rect::new(
            self.x + WIDTH - PAD_X - w,
            self.button_y(form.height()),
            w,
            BUTTON_HEIGHT,
        )
    }

    /// Cancel (first step) / Back, left of the primary button.
    pub fn secondary_button_rect(&self, form: &AddHostForm) -> Rect {
        let label = if form.step() == AddHostStep::Target {
            LABEL_CANCEL
        } else {
            LABEL_BACK
        };
        let w = ov::action_width(label);
        let primary = self.primary_button_rect(form);
        Rect::new(primary.x - BUTTON_GAP - w, primary.y, w, BUTTON_HEIGHT)
    }

    /// "Copy" beside Back / Cancel, only while an error is shown.
    pub fn copy_error_rect(&self, form: &AddHostForm) -> Option<Rect> {
        form.error()?;
        let secondary = self.secondary_button_rect(form);
        let w = ov::action_width(LABEL_COPY);
        Some(Rect::new(
            secondary.x - BUTTON_GAP - w,
            secondary.y,
            w,
            BUTTON_HEIGHT,
        ))
    }

    /// "Step n of 3" / validation message, left of the buttons.
    pub fn footer_text_rect(&self, form: &AddHostForm) -> Rect {
        let buttons_left = self
            .copy_error_rect(form)
            .unwrap_or_else(|| self.secondary_button_rect(form))
            .x;
        Rect::new(
            self.inner_x(),
            self.secondary_button_rect(form).y,
            (buttons_left - FOOTER_TEXT_GAP - self.inner_x()).max(0.0),
            BUTTON_HEIGHT,
        )
    }

    /// Hit-test inside an open dialog. Coordinates are logical pixels.
    pub fn hit_test(&self, form: &AddHostForm, x: f32, y: f32) -> AddHostHit {
        let dialog = self.rect(form.height());
        // An open list may extend past the dialog bottom; its options stay
        // clickable.
        if let Some(menu) = self.menu_rect(form) {
            if menu.contains(x, y) {
                for i in 0..form.menu_len() {
                    if let Some(opt) = self.menu_option_rect(form, i) {
                        if opt.contains(x, y) {
                            return match form.menu() {
                                Some(SelectMenu::Group) => AddHostHit::SelectGroup(i),
                                _ => AddHostHit::SelectIdentity(i),
                            };
                        }
                    }
                }
                return AddHostHit::Consume;
            }
        }
        if !dialog.contains(x, y) {
            return AddHostHit::Consume;
        }
        if self.close_button_rect().contains(x, y) {
            return AddHostHit::Close;
        }
        for &step in &STEPS {
            if self.step_rect(step).contains(x, y) {
                return AddHostHit::StepPill(step);
            }
        }
        if self.copy_error_rect(form).is_some_and(|r| r.contains(x, y)) {
            return AddHostHit::CopyError;
        }
        if self.secondary_button_rect(form).contains(x, y) {
            return if form.step() == AddHostStep::Target {
                AddHostHit::Cancel
            } else {
                AddHostHit::Back
            };
        }
        if self.primary_button_rect(form).contains(x, y) {
            return if form.step() == AddHostStep::Details {
                AddHostHit::Connect
            } else {
                AddHostHit::Next
            };
        }
        if let Some(cards) = self.choice_rects(form) {
            if let Some(i) = cards.iter().position(|c| c.contains(x, y)) {
                return AddHostHit::SelectAuth(i);
            }
        }
        if self
            .generate_link_rect(form)
            .is_some_and(|r| r.contains(x, y))
        {
            return AddHostHit::GenerateKey;
        }
        for field in form.visible_fields() {
            if field == Field::AuthMethod {
                continue;
            }
            let Some(layout) = self.field_layout(form, field) else {
                continue;
            };
            if !layout.box_rect.contains(x, y) {
                continue;
            }
            return match field {
                Field::Password
                    if self
                        .password_toggle_rect(form)
                        .is_some_and(|eye| eye.contains(x, y)) =>
                {
                    AddHostHit::TogglePasswordVisible
                }
                Field::Identity => AddHostHit::ToggleIdentityMenu,
                Field::Group => AddHostHit::ToggleGroupMenu,
                other => AddHostHit::Field(other),
            };
        }
        AddHostHit::Consume
    }
}
