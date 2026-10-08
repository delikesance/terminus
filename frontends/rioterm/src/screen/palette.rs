//! `Screen` palette surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::bindings::FontSizeAction;
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_backend::crosswords::pos::Direction;

#[cfg(windows)]
use crate::screen::hint_actions::shell_execute_open;

impl Screen<'_> {
    // return true if the click was handled by the island
    #[inline]
    pub fn handle_palette_click(&mut self, clipboard: &mut Clipboard) -> bool {
        if !self.renderer.command_palette.is_enabled() {
            return false;
        }

        let scale_factor = self.sugarloaf.scale_factor();
        let window_width = self.sugarloaf.window_size().width;
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        match self.renderer.command_palette.hit_test(
            mouse_x,
            mouse_y,
            window_width,
            scale_factor,
        ) {
            Ok(Some(index)) => {
                self.renderer.command_palette.selected_index = index;
                self.confirm_palette_selection(clipboard);
                self.mark_dirty();
                true
            }
            Ok(None) => {
                // "Add server …" on the empty result: hand the query to the wizard.
                if self
                    .renderer
                    .command_palette
                    .add_server_hit(mouse_x, mouse_y)
                {
                    self.palette_add_server();
                    self.mark_dirty();
                }
                // Otherwise: inside the palette but not on a result.
                true
            }
            Err(()) => {
                // Clicked outside — close palette
                self.renderer.command_palette.set_enabled(false);
                self.mark_dirty();
                true
            }
        }
    }

    /// Close the palette and open the add-server wizard with the typed query.
    fn palette_add_server(&mut self) {
        let Some(query) = self.renderer.command_palette.add_server_query() else {
            return;
        };
        self.renderer.command_palette.set_enabled(false);
        self.chrome.open_add_host();
        self.chrome_commit_text(&query);
    }

    /// Snapshot stored SSH hosts for the command palette.
    pub fn palette_host_items(
        &self,
    ) -> Vec<crate::renderer::command_palette::HostPaletteItem> {
        self.host_store
            .hosts()
            .iter()
            .map(|h| crate::renderer::command_palette::HostPaletteItem {
                id: h.id.clone(),
                title: h.name.clone(),
                subtitle: h.endpoint(),
            })
            .collect()
    }

    /// Enter / click confirmation for the open command palette.
    ///
    /// Handles host open, font copy, mode switches (ListFonts / ListHosts),
    /// and one-shot actions — shared by keyboard and mouse so they cannot drift.
    pub fn confirm_palette_selection(&mut self, clipboard: &mut Clipboard) {
        use crate::renderer::command_palette::PaletteAction;

        if self.renderer.command_palette.add_server_query().is_some() {
            self.palette_add_server();
            return;
        }

        if let Some(host_id) = self.renderer.command_palette.get_selected_host_id() {
            use crate::renderer::command_palette::HostPick;
            let pick = self.renderer.command_palette.host_pick();
            self.renderer.command_palette.set_enabled(false);
            let opened = match pick {
                HostPick::Session => self.open_host_session(&host_id, clipboard),
                HostPick::Sftp => self.open_sftp_pane(&host_id),
            };
            if let Err(err) = opened {
                self.chrome.panel.error = Some(err);
            }
            return;
        }

        if let Some(font) = self.renderer.command_palette.get_selected_font() {
            clipboard.set(ClipboardType::Clipboard, font);
            self.renderer.command_palette.set_enabled(false);
            return;
        }

        match self.renderer.command_palette.get_selected_action() {
            Some(PaletteAction::ListFonts) => {
                let fonts = self.sugarloaf.font_family_names();
                self.renderer.command_palette.enter_fonts_mode(fonts);
            }
            Some(PaletteAction::ListHosts) => {
                let hosts = self.palette_host_items();
                self.renderer.command_palette.enter_hosts_mode(hosts);
            }
            // More than one host: let the user pick which one to browse.
            Some(PaletteAction::OpenSftp) if self.palette_host_items().len() > 1 => {
                use crate::renderer::command_palette::HostPick;
                let hosts = self.palette_host_items();
                self.renderer
                    .command_palette
                    .enter_hosts_mode_for(hosts, HostPick::Sftp);
            }
            Some(action) => {
                self.renderer.command_palette.set_enabled(false);
                self.execute_palette_action(action, clipboard);
            }
            None => {
                self.renderer.command_palette.set_enabled(false);
            }
        }
    }

    #[inline]
    pub fn handle_search_click(&mut self, clipboard: &mut Clipboard) -> bool {
        if !self.renderer.search.is_active() {
            return false;
        }

        let scale_factor = self.sugarloaf.scale_factor();
        let window_width = self.sugarloaf.window_size().width;
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        match self
            .renderer
            .search
            .hit_test(mouse_x, mouse_y, window_width, scale_factor)
        {
            Ok(Some(action)) => {
                use crate::renderer::search::SearchOverlayAction;
                match action {
                    SearchOverlayAction::Next => {
                        self.advance_search_origin(self.search_state.direction);
                    }
                    SearchOverlayAction::Previous => {
                        let direction = self.search_state.direction.opposite();
                        self.advance_search_origin(direction);
                    }
                    SearchOverlayAction::Close => {
                        self.cancel_search(clipboard);
                        self.resize_top_or_bottom_line(self.ctx().len());
                    }
                }
                self.mark_dirty();
                true
            }
            Ok(None) => {
                // Clicked inside overlay but not on a button (input area)
                true
            }
            Err(()) => {
                // Clicked outside — don't close search, just pass through
                false
            }
        }
    }

    #[inline]
    pub fn handle_assistant_click(&mut self) -> bool {
        if !self.renderer.assistant.is_active() {
            return false;
        }

        let scale_factor = self.sugarloaf.scale_factor();
        let window_width = self.sugarloaf.window_size().width;
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        match self.renderer.assistant.hit_test(
            mouse_x,
            mouse_y,
            window_width,
            scale_factor,
        ) {
            Ok(Some(action)) => {
                use crate::renderer::assistant::AssistantOverlayAction;
                match action {
                    AssistantOverlayAction::Close => {
                        self.renderer.assistant.clear();
                    }
                    AssistantOverlayAction::OpenDocs => {
                        Self::open_url("https://rioterm.com/docs/config");
                    }
                }
                self.mark_dirty();
                true
            }
            Ok(None) => {
                // Clicked inside overlay but not on a button
                true
            }
            Err(()) if self.renderer.assistant.is_error() => {
                // Clicked outside an error — close it (errors are modal).
                self.renderer.assistant.clear();
                self.mark_dirty();
                true
            }
            // A warning never blocks: the click goes where it was aimed.
            Err(()) => false,
        }
    }

    /// Shortcut labels for palette commands, read from this window's key
    /// bindings so they match the platform and the user's remaps.
    pub(super) fn palette_shortcuts(
        &self,
    ) -> Vec<(crate::renderer::command_palette::PaletteAction, String)> {
        crate::renderer::command_palette::command_actions()
            .filter_map(|action| {
                let binding = action.binding_action()?;
                let label = crate::bindings::shortcut_label(&self.bindings, &binding)?;
                Some((action, label))
            })
            .collect()
    }

    pub(super) fn open_url(url: &str) {
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }
        #[cfg(windows)]
        shell_execute_open(url);
    }

    /// The launch update screen. Returns what the route must do next.
    pub(crate) fn render_updating(&mut self) -> crate::updater::StartupPhase {
        let phase = {
            let _ = self.updater.pump();
            self.updater.startup_phase()
        };
        if let crate::updater::StartupPhase::Downloading {
            version,
            done,
            total,
        } = &phase
        {
            crate::router::routes::updating::screen(
                &mut self.sugarloaf,
                &self.context_manager.current().dimension,
                &self.renderer.chrome_theme,
                version,
                *done,
                *total,
            );
            self.sugarloaf.render();
        }
        phase
    }

    pub(crate) fn render_welcome(&mut self) {
        crate::router::routes::welcome::screen(
            &mut self.sugarloaf,
            &self.context_manager.current().dimension,
        );
        self.sugarloaf.render();
    }

    pub fn execute_palette_action(
        &mut self,
        action: crate::renderer::command_palette::PaletteAction,
        clipboard: &mut Clipboard,
    ) {
        use crate::renderer::command_palette::PaletteAction;
        match action {
            PaletteAction::TabCreate => self.create_tab(clipboard),
            PaletteAction::TabClose => self.close_tab(clipboard),
            PaletteAction::TabCloseUnfocused => {
                if self.ctx().len() > 1 {
                    self.context_manager
                        .close_unfocused_tabs(&mut self.sugarloaf);
                    if let Some(ref mut island) = self.renderer.island {
                        island.dismiss_color_picker();
                    }
                    self.resize_top_or_bottom_line(1);
                }
            }
            PaletteAction::SelectNextTab => {
                self.clear_selection();
                let old = self.context_manager.current_index();
                self.context_manager.switch_to_next();
                let new = self.context_manager.current_index();
                self.switch_visible_context(old, new);
            }
            PaletteAction::SelectPrevTab => {
                self.clear_selection();
                let old = self.context_manager.current_index();
                self.context_manager.switch_to_prev();
                let new = self.context_manager.current_index();
                self.switch_visible_context(old, new);
            }
            PaletteAction::SplitRight => self.split_right(),
            PaletteAction::SplitDown => self.split_down(),
            PaletteAction::SelectNextSplit => {
                self.context_manager.select_next_split();
            }
            PaletteAction::SelectPrevSplit => {
                self.context_manager.select_prev_split();
            }
            PaletteAction::CloseCurrentSplitOrTab => self.close_split_or_tab(clipboard),
            PaletteAction::ConfigEditor => {
                self.context_manager.switch_to_settings();
            }
            PaletteAction::WindowCreateNew => {
                self.context_manager.create_new_window();
            }
            PaletteAction::IncreaseFontSize => {
                self.change_font_size(FontSizeAction::Increase);
            }
            PaletteAction::DecreaseFontSize => {
                self.change_font_size(FontSizeAction::Decrease);
            }
            PaletteAction::ResetFontSize => {
                self.change_font_size(FontSizeAction::Reset);
            }
            PaletteAction::ToggleViMode => {
                let context = self.context_manager.current_mut();
                let mut terminal = context.terminal.lock();
                terminal.toggle_vi_mode();
                drop(terminal);
                context
                    .renderable_content
                    .pending_update
                    .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            }
            PaletteAction::ToggleFullscreen => {
                self.context_manager.toggle_full_screen();
            }
            PaletteAction::ToggleAppearanceTheme => {
                self.context_manager.toggle_appearance_theme();
            }
            PaletteAction::Copy => {
                self.copy_selection(ClipboardType::Clipboard, clipboard);
            }
            PaletteAction::Paste => {
                self.paste_clipboard_or_image(clipboard);
            }
            PaletteAction::SearchForward => {
                self.start_search(Direction::Right);
            }
            PaletteAction::SearchBackward => {
                self.start_search(Direction::Left);
            }
            PaletteAction::ClearHistory => {
                let mut terminal = self.context_manager.current_mut().terminal.lock();
                terminal.clear_saved_history();
            }
            PaletteAction::ListFonts => {
                // Handled in confirm_palette_selection: switches into fonts
                // mode and keeps the palette open. If we land here it's a
                // no-op so the palette just closes without side effects.
            }
            PaletteAction::ListHosts => {
                // Same stay-open mode switch as ListFonts (confirm path).
            }
            PaletteAction::OpenSftp => {
                let hosts = self.palette_host_items();
                match hosts.first() {
                    Some(host) => {
                        if let Err(err) = self.open_sftp_pane(&host.id) {
                            self.chrome.panel.notice = Some(err);
                        }
                    }
                    None => {
                        self.chrome.panel.notice =
                            Some("Add a host first to open SFTP".into());
                    }
                }
            }
            PaletteAction::CheckForUpdates => {
                self.chrome.panel.notice = Some("Checking for updates…".into());
                self.updater.check_now();
            }
            PaletteAction::InstallUpdate => {
                use crate::updater::{install_action, InstallAction};
                match install_action(self.updater.state()) {
                    InstallAction::Install => {
                        self.chrome.panel.notice = Some("Preparing the update…".into());
                        self.updater.install();
                    }
                    InstallAction::CopyCommand(command) => {
                        self.chrome.panel.notice =
                            Some(format!("Copied, paste in a terminal: {command}"));
                        clipboard.set(ClipboardType::Clipboard, command);
                    }
                    InstallAction::OpenPage(url) => Self::open_url(&url),
                }
            }
            PaletteAction::RestartToUpdate => {
                // The new binary starts only once the quit is confirmed.
                match self.updater.arm_exit_action() {
                    Ok(()) => self.context_manager.quit(),
                    Err(err) => self.chrome.panel.notice = Some(err),
                }
            }
            PaletteAction::Quit => {
                self.context_manager.quit();
            }
            PaletteAction::ShowView(view) => {
                self.show_view(view);
            }
            PaletteAction::FilterServers => {
                self.chrome.panel.filter_focused = true;
            }
        }
    }
}
