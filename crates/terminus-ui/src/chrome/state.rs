use super::*;

impl Chrome {
    /// Width the chrome takes from the terminal's area, in logical
    /// pixels. This is the value the grid margin reserves.
    /// Left grid inset: the sidebar plus the terminal's padding inside
    /// the main card.
    pub fn reserved_width(&self) -> f32 {
        crate::shell::grid_insets().left
    }

    /// Top edge of the sidebar: it runs the full window height.
    pub fn origin_y(&self) -> f32 {
        0.0
    }

    /// The machine list is always the sidebar's content.
    pub fn hosts_visible(&self) -> bool {
        true
    }

    /// The old snippets drawer is gone; snippets are the Snippets view.
    pub fn snippets_visible(&self) -> bool {
        false
    }

    /// Switch the workspace view. The Settings page replaces the legacy
    /// settings dialog: switching view closes it if something opened it.
    pub fn show_view(&mut self, view: crate::shell::WorkspaceView) -> bool {
        let changed = self.shell.workspace.show(view);
        if changed && self.settings.open {
            self.settings.close();
        }
        changed
    }

    /// Route one input event to the view on screen (not the terminal).
    /// Esc on a machine view that ignores it goes back to the terminal.
    pub fn view_input(
        &mut self,
        input: &crate::screens::ViewInput,
    ) -> crate::screens::ViewOutcome {
        use crate::screens::{ViewInput, ViewKey, ViewOutcome};
        let view = self.shell.view();
        let content = self.shell.content_rect();
        let out = self.screens.handle(view, content, input);
        if out == ViewOutcome::Ignored
            && view.is_machine_view()
            && matches!(
                input,
                ViewInput::Key {
                    key: ViewKey::Escape,
                    ..
                }
            )
            && self.show_view(crate::shell::WorkspaceView::Terminal)
        {
            return ViewOutcome::Redraw;
        }
        out
    }

    /// Replace the host list.
    pub fn set_hosts(&mut self, hosts: Vec<HostItem>) {
        self.panel.set_items(hosts);
    }

    /// Replace the list with grouped rows: section labels and hosts.
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.panel.set_rows(rows);
    }

    /// Replace the host-list filter text.
    pub fn set_filter(&mut self, filter: String) {
        self.panel.filter = crate::components::input::TextDraft::new(filter);
    }

    /// Current host-list filter text.
    pub fn filter(&self) -> &str {
        &self.panel.filter.value
    }

    /// Toggle whether a group id is collapsed in the host list.
    pub fn toggle_group_collapsed(&mut self, id: &str) {
        if self.panel.collapsed_groups.contains(id) {
            self.panel.collapsed_groups.remove(id);
        } else {
            self.panel.collapsed_groups.insert(id.to_string());
        }
    }
}
