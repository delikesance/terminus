//! General tab: one toggle, `[startup] restore-tabs`.

use super::updates::{toggle_row, ToggleRow, MAX_WIDTH};
use super::{column, Measure, SettingsAction};
use crate::geom::Rect;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GeneralState {
    pub restore_tabs: bool,
    pub hover: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneralLayout {
    pub restore: ToggleRow,
}

impl GeneralState {
    pub fn layout(&self, content: Rect) -> GeneralLayout {
        let col = column(content);
        let w = col.width.min(MAX_WIDTH);
        GeneralLayout {
            restore: toggle_row(col.x, col.y, w),
        }
    }

    pub fn hit(&self, l: &GeneralLayout, x: f32, y: f32) -> bool {
        l.restore.card.contains(x, y)
    }

    pub fn hover(&mut self, content: Rect, _m: Measure, x: f32, y: f32) -> bool {
        let hit = self.hit(&self.layout(content), x, y);
        let changed = hit != self.hover;
        self.hover = hit;
        changed
    }

    pub fn press(
        &mut self,
        content: Rect,
        _m: Measure,
        x: f32,
        y: f32,
    ) -> Option<SettingsAction> {
        if !self.hit(&self.layout(content), x, y) {
            return None;
        }
        self.restore_tabs = !self.restore_tabs;
        Some(SettingsAction::SetRestoreTabs(self.restore_tabs))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_measure;
    use super::*;

    const CONTENT: Rect = Rect::new(260.0, 96.0, 1180.0, 804.0);

    fn center(r: Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    #[test]
    fn restore_tabs_is_off_by_default() {
        assert!(!GeneralState::default().restore_tabs);
    }

    #[test]
    fn toggle_flips_and_reports_the_new_value() {
        let mut s = GeneralState::default();
        let l = s.layout(CONTENT);
        let (x, y) = center(l.restore.toggle);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SetRestoreTabs(true))
        );
        assert!(s.restore_tabs);
        let (x, y) = center(l.restore.card);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SetRestoreTabs(false))
        );
    }

    #[test]
    fn whole_card_is_the_hit_area() {
        let s = GeneralState::default();
        let l = s.layout(CONTENT);
        let (x, y) = (l.restore.text.x + 4.0, l.restore.card.y + 4.0);
        assert!(s.hit(&l, x, y));
        assert!(!s.hit(&l, CONTENT.x, CONTENT.y));
    }

    #[test]
    fn config_edit_writes_the_startup_key() {
        assert_eq!(
            SettingsAction::SetRestoreTabs(true).config_edit(),
            Some(("startup", "restore-tabs", "true".to_string()))
        );
    }
}
