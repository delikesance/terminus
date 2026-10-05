// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! "Quit Terminus?" dialog (Linux; macOS/Windows use the native confirm).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::overlay::DialogKey;
use terminus_ui::confirm::{ConfirmOutcome, ConfirmPrompt};
use terminus_ui::theme::ChromeTheme;

use crate::renderer::dialogs::confirm::{paint_confirm, ConfirmView};

#[derive(Default)]
pub struct ConfirmQuit {
    prompt: Option<ConfirmPrompt>,
    sessions: usize,
}

impl ConfirmQuit {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.prompt.is_some()
    }

    /// Open (fresh focus and hover) or close the dialog.
    pub fn set_active(&mut self, active: bool) {
        self.prompt = active.then(|| ConfirmPrompt::quit(self.sessions));
    }

    /// Open sessions to mention in the body (set before activating).
    pub fn set_sessions(&mut self, sessions: usize) {
        self.sessions = sessions;
    }

    pub fn key(&mut self, key: DialogKey) -> ConfirmOutcome {
        self.prompt
            .as_mut()
            .map_or(ConfirmOutcome::Idle, |p| p.key(key))
    }

    /// Left press at logical `(x, y)` in a window of `window` logical px.
    pub fn press(&mut self, window: (f32, f32), x: f32, y: f32) -> ConfirmOutcome {
        self.prompt
            .as_mut()
            .map_or(ConfirmOutcome::Idle, |p| p.press(window, x, y))
    }

    pub fn hover_at(&mut self, window: (f32, f32), x: f32, y: f32) -> bool {
        self.prompt
            .as_mut()
            .is_some_and(|p| p.hover_at(window, x, y))
    }

    /// Whether `(x, y)` is over one of the two buttons (pointer cursor).
    pub fn over_button(&self, window: (f32, f32), x: f32, y: f32) -> bool {
        use terminus_ui::components::overlay::DialogHit;
        self.prompt.as_ref().is_some_and(|p| {
            matches!(
                p.layout(window).hit_test(x, y),
                DialogHit::Confirm | DialogHit::Cancel
            )
        })
    }

    /// `dimensions` is `(window_width, window_height, scale_factor)`,
    /// matching the other overlays' `render` signature.
    pub fn render(
        &self,
        sugarloaf: &mut Sugarloaf,
        theme: &ChromeTheme,
        dimensions: (f32, f32, f32),
    ) {
        let Some(prompt) = self.prompt.as_ref() else {
            return;
        };
        let (width, height, scale) = dimensions;
        let layout = prompt.layout((width / scale, height / scale));
        let view = ConfirmView {
            focus: Some(prompt.focus),
            hover: prompt.hover,
            option_checked: false,
        };
        sugarloaf.begin_overlay();
        paint_confirm(sugarloaf, theme, &prompt.spec, &layout, view, true);
        sugarloaf.end_overlay();
    }
}
