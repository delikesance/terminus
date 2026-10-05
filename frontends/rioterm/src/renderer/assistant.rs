// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use std::time::Duration;

use rio_backend::error::{RioError, RioErrorLevel};
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::feedback::{
    stack_rects, toast_height, Toast, ToastHit, ToastKind, ToastLayout,
};
use terminus_ui::theme::ChromeTheme;

use crate::renderer::components::feedback::{measure_toast, paint_toast};

const DOCS_URL: &str = "rioterm.com/docs/config";
/// Longest body shown (wrapped lines); the rest is elided.
const MAX_BODY_LINES: usize = 7;

/// Actions triggered by clicking assistant overlay buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantOverlayAction {
    Close,
    OpenDocs,
}

pub struct AssistantOverlay {
    error: Option<RioError>,
    hovered_button: Option<AssistantOverlayAction>,
    /// Geometry of the last painted toast: hit-testing walks exactly this.
    last_layout: Option<ToastLayout>,
}

impl Default for AssistantOverlay {
    fn default() -> Self {
        Self {
            error: None,
            hovered_button: None,
            last_layout: None,
        }
    }
}

impl AssistantOverlay {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.error.is_some()
    }

    /// Whether the active toast is a hard error. Errors are modal
    /// (keys and IME blocked, Enter dismisses); warnings render but
    /// must never block input over a working terminal.
    #[inline]
    pub fn is_error(&self) -> bool {
        self.error
            .as_ref()
            .is_some_and(|error| error.level == RioErrorLevel::Error)
    }

    #[inline]
    pub fn set_error(&mut self, error: RioError) {
        self.error = Some(error);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.error = None;
        self.last_layout = None;
        self.hovered_button = None;
    }

    #[inline]
    pub fn hovered_button(&self) -> Option<AssistantOverlayAction> {
        self.hovered_button
    }

    fn action_at(&self, mouse_x: f32, mouse_y: f32) -> Option<AssistantOverlayAction> {
        match self.last_layout.as_ref()?.hit_test(mouse_x, mouse_y)? {
            ToastHit::Dismiss => Some(AssistantOverlayAction::Close),
            ToastHit::Action(_) => Some(AssistantOverlayAction::OpenDocs),
            ToastHit::Body => None,
        }
    }

    /// Hit-test a mouse click (logical px). Returns Some(action) if a
    /// control was clicked; Err(()) if clicked outside the toast.
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        _window_width: f32,
        _scale_factor: f32,
    ) -> Result<Option<AssistantOverlayAction>, ()> {
        let Some(layout) = self.last_layout.as_ref() else {
            return Err(());
        };
        if !layout.rect.contains(mouse_x, mouse_y) {
            return Err(());
        }
        Ok(self.action_at(mouse_x, mouse_y))
    }

    /// Update hover state based on mouse position. Returns true if changed.
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        _window_width: f32,
        _scale_factor: f32,
    ) -> bool {
        if !self.is_active() {
            return false;
        }
        let new_hover = self.action_at(mouse_x, mouse_y);
        if new_hover != self.hovered_button {
            self.hovered_button = new_hover;
            return true;
        }
        false
    }

    /// The error as a feedback toast (kind, copy, one "Open docs" action).
    fn toast(&self) -> Option<Toast> {
        let error = self.error.as_ref()?;
        let (kind, title) = if error.level == RioErrorLevel::Error {
            (ToastKind::Error, "Error")
        } else {
            (ToastKind::Warning, "Warning")
        };
        let report = error.report.to_string();
        let body = report.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut toast = Toast::new(
            kind,
            title,
            body,
            vec![format!("Open docs ({DOCS_URL})")],
            Duration::ZERO,
        );
        // The error persists until dismissed, whatever its level.
        toast.ttl = None;
        Some(toast)
    }

    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        theme: &ChromeTheme,
        dimensions: (f32, f32, f32),
    ) {
        let Some(mut toast) = self.toast() else {
            // Immediate mode: not drawing == not visible.
            self.last_layout = None;
            return;
        };
        let (window_width, window_height, scale) = dimensions;
        let viewport = (window_width / scale, window_height / scale);

        sugarloaf.begin_overlay();
        let (lines, _) = measure_toast(sugarloaf, &toast);
        if lines.len() > MAX_BODY_LINES {
            let mut kept = lines[..MAX_BODY_LINES].join(" ");
            kept.push('\u{2026}');
            toast.body = kept;
        }
        let (lines, _) = measure_toast(sugarloaf, &toast);
        let height = toast_height(lines.len(), true);
        let rect = stack_rects(viewport.0, viewport.1, toast.width, &[height])[0];
        self.last_layout = Some(paint_toast(sugarloaf, theme, &toast, rect.x, rect.y));
        sugarloaf.end_overlay();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terminus_ui::components::feedback::toast_layout;

    fn painted(overlay: &mut AssistantOverlay) -> ToastLayout {
        let toast = overlay.toast().unwrap();
        let layout = toast_layout(&toast, 900.0, 700.0, 2, &[120.0]);
        overlay.last_layout = Some(layout.clone());
        layout
    }

    #[test]
    fn clicks_map_to_dismiss_docs_and_outside() {
        let mut o = AssistantOverlay::default();
        o.set_error(RioError::configuration_not_found());
        let l = painted(&mut o);
        let d = l.dismiss;
        assert_eq!(
            o.hit_test(d.x + 2.0, d.y + 2.0, 0.0, 1.0),
            Ok(Some(AssistantOverlayAction::Close))
        );
        let a = l.actions[0];
        assert_eq!(
            o.hit_test(a.x + 2.0, a.y + 2.0, 0.0, 1.0),
            Ok(Some(AssistantOverlayAction::OpenDocs))
        );
        assert_eq!(o.hit_test(l.title.x, l.title.y, 0.0, 1.0), Ok(None));
        assert_eq!(o.hit_test(1.0, 1.0, 0.0, 1.0), Err(()));
        assert!(o.hover(d.x + 2.0, d.y + 2.0, 0.0, 1.0));
        assert_eq!(o.hovered_button(), Some(AssistantOverlayAction::Close));
    }

    #[test]
    fn errors_and_warnings_persist_until_dismissed() {
        let mut o = AssistantOverlay::default();
        o.set_error(RioError::configuration_not_found());
        assert!(o.toast().unwrap().is_persistent());
        assert!(o.is_active());
        o.clear();
        assert!(o.toast().is_none() && !o.is_active());
    }
}
