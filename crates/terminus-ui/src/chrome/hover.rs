use super::*;

impl Chrome {
    /// Drop every hover highlight — used when the pointer leaves the
    /// window, where no move event will arrive to clear it.
    pub fn clear_hover(&mut self) -> bool {
        let lost = self.lost.as_mut().is_some_and(|l| l.hover.take().is_some());
        self.panel.set_hover(None) || lost
    }

    /// Route a mouse move; returns whether anything needs repainting.
    pub fn handle_hover(&mut self, window_height: f32, x: f32, y: f32) -> bool {
        let window_width = { self.last_window_width };
        if let Some(prompt) = self.confirm.as_mut() {
            return prompt.hover_at((window_width, window_height), x, y);
        }
        if let Some(menu) = self.context_menu.as_mut() {
            return menu.hover_at(x, y);
        }
        if self.settings.open {
            return self
                .settings
                .handle_hover(window_width, window_height, x, y);
        }
        if self.connection_hit(x, y).is_some() {
            return false;
        }
        if let Some(area) = self.lost_area() {
            if let Some(lost) = self.lost.as_mut() {
                // Leaving the card clears its hover too.
                let changed = lost.hover_at(area, x, y);
                if area.contains(x, y) {
                    return changed;
                }
                if changed {
                    return true;
                }
            }
        }

        if self.snippet_form.is_open() {
            let layout = crate::dialog_form::DialogFormLayout::compute(
                &self.snippet_form.inner,
                window_width,
                window_height,
            );
            let next = match layout.hit_test(x, y) {
                Some(crate::dialog_form::DynamicFormHit::Save) => {
                    Some(crate::dialog_form::DynamicFormHit::Save)
                }
                Some(crate::dialog_form::DynamicFormHit::Cancel) => {
                    Some(crate::dialog_form::DynamicFormHit::Cancel)
                }
                _ => None,
            };
            if self.snippet_form.inner.btn_hover != next {
                self.snippet_form.inner.btn_hover = next;
                return true;
            }
            return false;
        }

        if self.form.is_open() {
            let layout = self.dialog_layout(window_width, window_height);
            let mut changed = false;
            if self.form.menu().is_some() {
                let hover = (0..self.form.menu_len()).find(|&i| {
                    layout
                        .menu_option_rect(&self.form, i)
                        .is_some_and(|opt| opt.contains(x, y))
                });
                changed |= self.form.set_menu_hover(hover);
            } else {
                changed |= self.form.set_menu_hover(None);
            }
            let target = match layout.hit_test(&self.form, x, y) {
                AddHostHit::Consume | AddHostHit::Field(_) => None,
                other => Some(other),
            };
            changed |= self.form.set_hover(target);
            return changed;
        }
        let origin_y = self.origin_y();
        let height = window_height - origin_y;
        let shell_changed = self.shell.set_hover(self.shell.hit_test(x, y));
        let hover = self.panel.hover_at(origin_y, height, x, y);
        self.panel.set_hover(hover) | shell_changed
    }

    /// Remember the last layout width so hover/cursor can rebuild dialog rects.
    pub fn set_window_size(&mut self, width: f32, height: f32) {
        self.last_window_width = width;
        self.shell.window = (width, height);
    }

    /// Cursor affordance under `(x, y)`.
    pub fn cursor_at(
        &self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ChromeCursor {
        if let Some(prompt) = self.confirm.as_ref() {
            use crate::components::overlay::DialogHit;
            return match prompt.layout((window_width, window_height)).hit_test(x, y) {
                DialogHit::Confirm | DialogHit::Cancel | DialogHit::Option => {
                    ChromeCursor::Pointer
                }
                DialogHit::Inside | DialogHit::Scrim => ChromeCursor::Default,
            };
        }
        if let Some(menu) = self.context_menu.as_ref() {
            return match menu.hit_test(x, y) {
                MenuHit::Item(_) => ChromeCursor::Pointer,
                MenuHit::Consume | MenuHit::Dismiss => ChromeCursor::Default,
            };
        }
        if self.vault_unlock.is_open() {
            let layout = VaultUnlockLayout::for_prompt(
                window_width,
                window_height,
                &self.vault_unlock,
            );
            return match layout.hit_test_labels(x, y, self.vault_unlock.action_label()) {
                VaultUnlockHit::Field | VaultUnlockHit::ConfirmField => {
                    ChromeCursor::Text
                }
                VaultUnlockHit::ToggleVisible
                | VaultUnlockHit::ToggleRemember
                | VaultUnlockHit::Unlock
                | VaultUnlockHit::Cancel => ChromeCursor::Pointer,
                VaultUnlockHit::Consume => ChromeCursor::Default,
            };
        }
        if self.settings.open {
            return self.settings.cursor_at(window_width, window_height, x, y);
        }
        if let Some(hit) = self.connection_hit(x, y) {
            return match hit {
                ConnectionHit::Close | ConnectionHit::ToggleLogs => ChromeCursor::Pointer,
                ConnectionHit::Consume => ChromeCursor::Default,
            };
        }
        if let (Some(area), Some(lost)) = (self.lost_hit(x, y), self.lost.as_ref()) {
            return match lost.button_at(area, x, y) {
                Some(_) => ChromeCursor::Pointer,
                None => ChromeCursor::Default,
            };
        }

        if self.snippet_form.is_open() {
            let layout = crate::dialog_form::DialogFormLayout::compute(
                &self.snippet_form.inner,
                window_width,
                window_height,
            );
            return match layout.hit_test(x, y) {
                Some(crate::dialog_form::DynamicFormHit::Field(_)) => ChromeCursor::Text,
                Some(crate::dialog_form::DynamicFormHit::Save)
                | Some(crate::dialog_form::DynamicFormHit::Cancel) => {
                    ChromeCursor::Pointer
                }
                _ => ChromeCursor::Default,
            };
        }

        if self.form.is_open() {
            let layout = self.dialog_layout(window_width, window_height);
            let dialog = layout.rect(self.form.height());
            let on_menu = layout
                .menu_rect(&self.form)
                .is_some_and(|m| m.contains(x, y));
            if !dialog.contains(x, y) && !on_menu {
                return ChromeCursor::Pointer; // scrim dismiss
            }
            return match layout.hit_test(&self.form, x, y) {
                AddHostHit::Field(_) => ChromeCursor::Text,
                AddHostHit::StepPill(_)
                | AddHostHit::Back
                | AddHostHit::CopyError
                | AddHostHit::Next
                | AddHostHit::SelectAuth(_)
                | AddHostHit::ToggleIdentityMenu
                | AddHostHit::SelectIdentity(_)
                | AddHostHit::ToggleGroupMenu
                | AddHostHit::SelectGroup(_)
                | AddHostHit::TogglePasswordVisible
                | AddHostHit::GenerateKey
                | AddHostHit::Close
                | AddHostHit::Connect
                | AddHostHit::Cancel => ChromeCursor::Pointer,
                AddHostHit::Consume => ChromeCursor::Default,
            };
        }
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        if let Some(hit) = self.shell.hit_test(x, y) {
            return if hit.is_control() {
                ChromeCursor::Pointer
            } else {
                ChromeCursor::Default
            };
        }
        if self.shell.view_owns(x, y) {
            let view = self.shell.view();
            let content = self.shell.content_rect();
            return if self.screens.is_clickable(view, content, x, y) {
                ChromeCursor::Pointer
            } else {
                ChromeCursor::Default
            };
        }
        if self.hosts_visible() {
            return match self.panel.hit_test(origin_y, height, x, y) {
                Some(PanelHit::Search) | Some(PanelHit::NewGroupField) => {
                    ChromeCursor::Text
                }
                Some(PanelHit::Background) | None => ChromeCursor::Default,
                Some(_) => ChromeCursor::Pointer,
            };
        }
        ChromeCursor::Default
    }
}
