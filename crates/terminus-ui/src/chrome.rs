//! The chrome as one object: activity rail + host panel + add-host editor.
//!
//! Everything the mouse and the keyboard can do to the chrome is routed
//! through here, and everything the chrome reserves from the terminal's
//! area is answered by [`Chrome::reserved_width`]. The painters read
//! this state and never own any of it, so a repaint can never disagree
//! with a hit-test.

use crate::activity_bar::{self, ActivityBarState, Section};
use crate::add_host::{AddHostForm, FormInput, FormOutcome};
use crate::sidebar::{HostItem, HostPanel, PanelHit};

/// What a mouse press on the chrome did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChromeAction {
    /// The press missed the chrome; the terminal gets it.
    Ignored,
    /// The press hit the chrome and was consumed.
    Consumed,
    /// The add-host row was pressed: open the editor.
    AddHost,
    /// A host row was pressed: connect to that host.
    OpenHost(String),
}

/// Chrome state for one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    pub activity: ActivityBarState,
    pub panel: HostPanel,
    pub form: AddHostForm,
    /// Unscaled height reserved above the chrome by the tab strip, so
    /// the rail starts under the tabs instead of behind them.
    pub top_inset: f32,
    /// Whether the panel is expanded beside the rail.
    pub panel_visible: bool,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            activity: ActivityBarState::default(),
            panel: HostPanel::default(),
            form: AddHostForm::default(),
            top_inset: 0.0,
            panel_visible: true,
        }
    }
}

impl Chrome {
    /// Width the chrome takes from the terminal's area, in logical
    /// pixels. This is the value the grid margin reserves.
    pub fn reserved_width(&self) -> f32 {
        if self.activity.collapsed {
            return 0.0;
        }
        if self.panel_visible {
            activity_bar::WIDTH + crate::sidebar::WIDTH
        } else {
            activity_bar::WIDTH
        }
    }

    /// Top edge of the chrome, in logical pixels.
    pub fn origin_y(&self) -> f32 {
        self.top_inset
    }

    /// Whether the panel's host list (rather than a placeholder) is the
    /// thing on screen.
    pub fn hosts_visible(&self) -> bool {
        self.panel_visible && self.activity.selected == Section::Hosts
    }

    /// Title shown in the panel header.
    pub fn panel_title(&self) -> &'static str {
        match self.activity.selected {
            Section::Hosts => "Hosts",
            Section::Forwards => "Port forwards",
            Section::Sftp => "SFTP",
            Section::Settings => "Settings",
        }
    }

    pub fn add_host_is_open(&self) -> bool {
        self.form.is_open()
    }

    /// Open the add-host editor.
    pub fn open_add_host(&mut self) {
        // The editor is useless behind a collapsed panel, so selecting
        // the rail's Hosts section is part of opening it.
        self.activity.selected = Section::Hosts;
        self.activity.collapsed = false;
        self.panel_visible = true;
        self.form.open();
    }

    /// Replace the host list.
    pub fn set_hosts(&mut self, hosts: Vec<HostItem>) {
        self.panel.set_items(hosts);
    }

    /// Drop every hover highlight — used when the pointer leaves the
    /// window, where no move event will arrive to clear it.
    pub fn clear_hover(&mut self) -> bool {
        self.panel.set_hover(None)
    }

    // ---- input -----------------------------------------------------

    /// Route a mouse press, in logical pixels.
    ///
    /// Called with the form open too: a click on the scrim dismisses the
    /// editor rather than reaching the terminal behind it, which is what
    /// makes the dialog feel modal.
    pub fn handle_press(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ChromeAction {
        let action = self.route_press(window_width, window_height, x, y);
        // Touching the chrome retires the "Added …" line: it has been
        // read by then, and leaving it up would hide a later failure.
        if action != ChromeAction::Ignored {
            self.panel.notice = None;
        }
        action
    }

    fn route_press(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ChromeAction {
        if self.form.is_open() {
            let layout = self.dialog_layout(window_width, window_height);
            let dialog = layout.rect(self.form.height());
            if dialog.contains(x, y) {
                // Clicks inside the dialog are consumed; field-to-field
                // focus follows the keyboard, which is the only path the
                // editor documents.
                return ChromeAction::Consumed;
            }
            self.form.close();
            return ChromeAction::Consumed;
        }

        if self.activity.collapsed {
            return ChromeAction::Ignored;
        }

        let origin_y = self.origin_y();
        if let Some(section) = activity_bar::hit_test(origin_y, x, y) {
            if section == self.activity.selected {
                // Pressing the active section toggles the panel, the way
                // an activity bar is expected to behave.
                self.panel_visible = !self.panel_visible;
            } else {
                self.activity.selected = section;
                self.panel_visible = true;
            }
            return ChromeAction::Consumed;
        }

        if !self.panel_visible {
            return ChromeAction::Ignored;
        }

        match self
            .panel
            .hit_test(origin_y, window_height - origin_y, x, y)
        {
            // The add-host row is the one control that works whatever
            // section is showing, so it opens the editor from any of them.
            Some(PanelHit::AddHost) => ChromeAction::AddHost,
            Some(PanelHit::Item(index)) if self.hosts_visible() => {
                self.panel.selected = Some(index);
                match self.panel.selected_item() {
                    Some(item) => ChromeAction::OpenHost(item.id.clone()),
                    None => ChromeAction::Consumed,
                }
            }
            Some(_) => ChromeAction::Consumed,
            None => ChromeAction::Ignored,
        }
    }

    /// Route a mouse move; returns whether anything needs repainting.
    pub fn handle_hover(&mut self, window_height: f32, x: f32, y: f32) -> bool {
        if self.form.is_open() || self.activity.collapsed || !self.hosts_visible() {
            return false;
        }
        let origin_y = self.origin_y();
        let hover = self
            .panel
            .hover_at(origin_y, window_height - origin_y, x, y);
        self.panel.set_hover(hover)
    }

    /// Route a wheel notch over the panel; returns whether it was consumed.
    pub fn handle_wheel(
        &mut self,
        window_height: f32,
        x: f32,
        y: f32,
        lines: f32,
    ) -> bool {
        if self.activity.collapsed || !self.hosts_visible() {
            return false;
        }
        let origin_y = self.origin_y();
        let height = window_height - origin_y;
        if !self.panel.rect(origin_y, height).contains(x, y) {
            return false;
        }
        let before = self.panel.scroll;
        // Wheel up (negative lines) scrolls toward the top.
        self.panel.scroll_rows(-lines, origin_y, height);
        self.panel.scroll != before || self.panel.content_height() == 0.0
    }

    /// Route a keyboard input to the editor. `None` when it is closed.
    pub fn handle_form_input(
        &mut self,
        input: FormInput,
        text: &str,
    ) -> Option<FormOutcome> {
        if !self.form.is_open() {
            return None;
        }
        let outcome = self.form.handle_input(input, text);
        if outcome == FormOutcome::Cancel {
            self.form.close();
        }
        Some(outcome)
    }

    /// Where the editor dialog sits for this window size.
    pub fn dialog_layout(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> crate::add_host::AddHostLayout {
        crate::add_host::AddHostLayout::centered(
            window_width,
            window_height,
            self.form.height(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::add_host::Field;

    fn chrome_with_hosts(n: usize) -> Chrome {
        let mut chrome = Chrome::default();
        chrome.set_hosts(
            (0..n)
                .map(|i| HostItem {
                    id: format!("id-{i}"),
                    name: format!("host-{i}"),
                    endpoint: format!("root@host-{i}"),
                })
                .collect(),
        );
        chrome
    }

    #[test]
    fn the_reserved_width_matches_what_the_rail_and_panel_paint() {
        let mut chrome = chrome_with_hosts(1);
        assert_eq!(
            chrome.reserved_width(),
            activity_bar::WIDTH + crate::sidebar::WIDTH
        );

        chrome.activity.collapsed = true;
        assert_eq!(chrome.reserved_width(), 0.0);

        chrome.activity.collapsed = false;
        chrome.panel_visible = false;
        assert_eq!(chrome.reserved_width(), activity_bar::WIDTH);
    }

    #[test]
    fn pressing_a_host_row_reports_which_host() {
        let mut chrome = chrome_with_hosts(3);
        let row = chrome.panel.item_rect(0.0, 1);
        let action = chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
        assert_eq!(action, ChromeAction::OpenHost("id-1".to_string()));
        assert_eq!(chrome.panel.selected, Some(1));
    }

    #[test]
    fn pressing_add_host_opens_the_editor_not_a_connection() {
        let mut chrome = chrome_with_hosts(3);
        let button = chrome.panel.add_button_rect(0.0, 800.0);
        let action = chrome.handle_press(1200.0, 800.0, button.x + 10.0, button.y + 5.0);
        assert_eq!(action, ChromeAction::AddHost);

        chrome.open_add_host();
        assert!(chrome.add_host_is_open());
    }

    #[test]
    fn a_press_outside_the_chrome_is_left_for_the_terminal() {
        let mut chrome = chrome_with_hosts(3);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, 600.0, 400.0),
            ChromeAction::Ignored
        );
        // Including the rail's own background, below its buttons.
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, 24.0, 700.0),
            ChromeAction::Ignored
        );
    }

    #[test]
    fn the_rail_switches_sections_and_toggles_the_panel() {
        let mut chrome = chrome_with_hosts(1);
        let sftp = activity_bar::item_rect(0.0, Section::Sftp.index());
        let (x, y) = (24.0, sftp.y + 10.0);

        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::Consumed
        );
        assert_eq!(chrome.activity.selected, Section::Sftp);
        assert!(!chrome.hosts_visible());

        // Pressing the active section collapses the panel, pressing it
        // again brings it back.
        chrome.handle_press(1200.0, 800.0, x, y);
        assert!(!chrome.panel_visible);
        chrome.handle_press(1200.0, 800.0, x, y);
        assert!(chrome.panel_visible);
    }

    #[test]
    fn a_host_row_is_inert_while_another_section_is_showing() {
        let mut chrome = chrome_with_hosts(3);
        chrome.activity.selected = Section::Forwards;
        let row = chrome.panel.item_rect(0.0, 1);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::Consumed
        );
        assert_eq!(chrome.panel.selected, None);
    }

    #[test]
    fn a_press_outside_the_chrome_leaves_the_notice_alone() {
        let mut chrome = chrome_with_hosts(3);
        chrome.panel.notice = Some("Added web-01".to_string());

        chrome.handle_press(1200.0, 800.0, 600.0, 400.0);
        assert!(chrome.panel.notice.is_some(), "typing must not eat it");

        let row = chrome.panel.item_rect(0.0, 0);
        chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
        assert_eq!(chrome.panel.notice, None);
    }

    #[test]
    fn the_open_dialog_swallows_clicks_and_the_scrim_dismisses_it() {
        let mut chrome = chrome_with_hosts(2);
        chrome.open_add_host();
        let layout = chrome.dialog_layout(1200.0, 800.0);
        let field = layout.field_rect(Field::Hostname);

        assert_eq!(
            chrome.handle_press(1200.0, 800.0, field.x + 5.0, field.y + 5.0),
            ChromeAction::Consumed
        );
        assert!(chrome.add_host_is_open());

        // A click far outside the dialog dismisses it instead of
        // focusing the terminal.
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, 20.0, 780.0),
            ChromeAction::Consumed
        );
        assert!(!chrome.add_host_is_open());
    }

    #[test]
    fn wheel_over_the_panel_scrolls_it_and_over_the_terminal_does_not() {
        let mut chrome = chrome_with_hosts(50);
        let (w, h) = (1200.0, 400.0);
        let _ = w;

        assert!(chrome.handle_wheel(h, 120.0, 300.0, -3.0));
        assert_eq!(chrome.panel.scroll, 3.0 * crate::sidebar::ITEM_HEIGHT);

        // The terminal's half of the window is untouched.
        assert!(!chrome.handle_wheel(h, 900.0, 300.0, -3.0));
        assert_eq!(chrome.panel.scroll, 3.0 * crate::sidebar::ITEM_HEIGHT);

        // And the wheel clamps at the top.
        chrome.handle_wheel(h, 120.0, 300.0, 99.0);
        assert_eq!(chrome.panel.scroll, 0.0);
    }

    #[test]
    fn keyboard_input_only_reaches_the_editor_while_it_is_open() {
        let mut chrome = chrome_with_hosts(1);
        assert_eq!(
            chrome.handle_form_input(FormInput::Enter, ""),
            None,
            "a closed editor must not swallow Enter"
        );

        chrome.open_add_host();
        assert_eq!(
            chrome.handle_form_input(FormInput::Text, "w"),
            Some(FormOutcome::Consumed)
        );
        assert_eq!(chrome.form.value(Field::Name), "w");

        assert_eq!(
            chrome.handle_form_input(FormInput::Enter, ""),
            Some(FormOutcome::Submit)
        );
        // Enter is a request to save, not a dismissal: the caller closes
        // the form only once the repository accepted the host.
        assert!(chrome.add_host_is_open());

        assert_eq!(
            chrome.handle_form_input(FormInput::Escape, ""),
            Some(FormOutcome::Cancel)
        );
        assert!(!chrome.add_host_is_open());
    }

    #[test]
    fn hovering_a_row_is_reported_once() {
        let mut chrome = chrome_with_hosts(3);
        let row = chrome.panel.item_rect(0.0, 2);
        let (x, y) = (row.x + 20.0, row.y + 20.0);

        assert!(chrome.handle_hover(800.0, x, y));
        assert_eq!(chrome.panel.hover, Some(2));
        // Same row again: no repaint.
        assert!(!chrome.handle_hover(800.0, x, y));
        // Up in the header, which is not a row: the highlight clears.
        assert!(chrome.handle_hover(800.0, x, 10.0));
        assert_eq!(chrome.panel.hover, None);
    }

    #[test]
    fn hovering_the_add_host_row_highlights_it_without_selecting_a_host() {
        let mut chrome = chrome_with_hosts(3);
        let button = chrome.panel.add_button_rect(0.0, 800.0);
        let (x, y) = (button.x + 20.0, button.y + button.height / 2.0);

        assert!(chrome.handle_hover(800.0, x, y));
        assert!(chrome.panel.add_hover);
        // Hovering a control must not look like hovering a host, and
        // must not select one either.
        assert_eq!(chrome.panel.hover, None);
        assert_eq!(chrome.panel.selected, None);

        // Moving onto a row moves the highlight off the button.
        let row = chrome.panel.item_rect(0.0, 1);
        assert!(chrome.handle_hover(800.0, row.x + 20.0, row.y + 20.0));
        assert!(!chrome.panel.add_hover);
        assert_eq!(chrome.panel.hover, Some(1));
    }

    #[test]
    fn the_client_area_excludes_the_top_inset() {
        let mut chrome = chrome_with_hosts(20);
        chrome.top_inset = 38.0;
        let row = chrome.panel.item_rect(38.0, 0);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::OpenHost("id-0".to_string())
        );
        // The same pixel with no inset is not the first row.
        chrome.top_inset = 0.0;
        chrome.panel.selected = None;
        assert_ne!(
            chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::OpenHost("id-0".to_string())
        );
    }
}
