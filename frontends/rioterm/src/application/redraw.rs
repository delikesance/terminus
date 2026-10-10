use super::Application;
use crate::router::routes::RoutePath;
use rio_window::event_loop::ActiveEventLoop;
use rio_window::event_loop::ControlFlow;
#[cfg(target_os = "macos")]
use rio_window::platform::macos::WindowExtMacOS;

impl Application<'_> {
    pub(super) fn handle_redraw(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        route.begin_render();
        route.window.screen.window_maximized = route.window.winit_window.is_maximized();

        match route.path {
            RoutePath::Welcome => {
                route.window.screen.render_welcome();
            }
            RoutePath::Updating => {
                use crate::updater::StartupPhase;
                match route.window.screen.render_updating() {
                    StartupPhase::Downloading { .. } => {}
                    StartupPhase::Relaunch => {
                        // The new binary starts; this one leaves
                        // without asking (nothing ran here yet).
                        match route.window.screen.updater.relaunch_now() {
                            Ok(()) => std::process::exit(0),
                            Err(err) => {
                                tracing::error!("update relaunch: {err}");
                                route.window.screen.updater.end_startup();
                                route.path = RoutePath::Terminal;
                            }
                        }
                    }
                    StartupPhase::Failed(message) => {
                        route.window.screen.updater.end_startup();
                        route.window.screen.chrome.panel.notice = Some(format!(
                            "Update skipped ({message}). Terminus {} opened.",
                            crate::updater::CURRENT_VERSION
                        ));
                        route.path = RoutePath::Terminal;
                    }
                    StartupPhase::None => route.path = RoutePath::Terminal,
                }
                route.request_redraw();
            }
            RoutePath::Terminal => {
                if let Some(window_update) = route.window.screen.render() {
                    use crate::context::renderable::{BackgroundState, WindowUpdate};
                    match window_update {
                        WindowUpdate::Background(bg_state) => {
                            // for now setting this as allowed because it fails on linux builds
                            #[allow(unused_variables)]
                            let bg_color = match bg_state {
                                BackgroundState::Set(color) => color,
                                BackgroundState::Reset => self.config.colors.background.1,
                            };

                            #[cfg(target_os = "macos")]
                            {
                                route.window.winit_window.set_background_color(
                                    bg_color.r, bg_color.g, bg_color.b, bg_color.a,
                                );
                            }

                            #[cfg(target_os = "windows")]
                            {
                                use rio_window::platform::windows::WindowExtWindows;
                                route.window.winit_window.set_title_bar_background_color(
                                    bg_color.r, bg_color.g, bg_color.b, bg_color.a,
                                );
                            }
                        }
                    }
                }

                // Vault unlock just succeeded: retry the action that needed it.
                if let Some(pending) = route.window.screen.take_pending_vault_continue() {
                    match pending {
                        terminus_ui::PendingVaultAction::OpenHost(id) => {
                            match route
                                .window
                                .screen
                                .open_host_session(&id, &mut self.router.clipboard)
                            {
                                Ok(()) => {
                                    route.window.screen.chrome.panel.error = None;
                                }
                                Err(err) => {
                                    route.window.screen.chrome.panel.error = Some(err);
                                }
                            }
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::AddHostSession(id) => {
                            match route
                                .window
                                .screen
                                .add_host_session(&id, &mut self.router.clipboard)
                            {
                                Ok(()) => {
                                    route.window.screen.chrome.panel.error = None;
                                }
                                Err(err) => {
                                    route.window.screen.chrome.panel.error = Some(err);
                                }
                            }
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::OpenGroup(id) => {
                            let result = route
                                .window
                                .screen
                                .open_group_sessions(&id, &mut self.router.clipboard);
                            route.window.screen.chrome.panel.error = result.err();
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::ReconnectSession(route_id) => {
                            route.window.screen.run_lost_session_action(
                                terminus_ui::ChromeAction::ReconnectSession(route_id),
                                &mut self.router.clipboard,
                            );
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::SubmitHostForm => {
                            route.window.screen.submit_host_form();
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::OpenSftp {
                            host_id,
                            other_pane,
                        } => {
                            let screen = &mut route.window.screen;
                            let opened = if other_pane {
                                screen.open_sftp_other_pane(&host_id)
                            } else {
                                screen.open_sftp_pane(&host_id)
                            };
                            screen.chrome.panel.error = opened.err();
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::CreateVault
                        | terminus_ui::PendingVaultAction::UnlockVault => {
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::SaveSshKey => {
                            if !route.window.screen.resubmit_settings_key_draft() {
                                route.window.screen.submit_key_draft();
                            }
                            route.request_overlay_redraw();
                        }
                        terminus_ui::PendingVaultAction::RestoreTabs(tabs) => {
                            route
                                .window
                                .screen
                                .reopen_saved_tabs(tabs, &mut self.router.clipboard);
                            route.request_redraw();
                        }
                    }
                }

                // The hosts just loaded: reopen last launch's tabs.
                if route
                    .window
                    .screen
                    .restore_saved_tabs_if_due(&mut self.router.clipboard)
                {
                    route.request_redraw();
                }

                // Update IME cursor position after rendering to ensure it's current
                route
                    .window
                    .screen
                    .update_ime_cursor_position_if_needed(&route.window.winit_window);
            }
        }

        #[cfg(target_os = "windows")]
        if !route.window.initial_frame_rendered {
            // Keep the native window cloaked until the first complete
            // render has been submitted. Uncloaking earlier can expose
            // the blank native surface during GPU/PTY startup.
            use rio_window::platform::windows::WindowExtWindows;
            route.window.winit_window.set_cloaked(false);
            route.window.initial_frame_rendered = true;
        }

        // let duration = start.elapsed();
        // println!("Time elapsed in render() is: {:?}", duration);
        // }

        // Game mode = unlocked framerate, so keep the event loop
        // spinning. Every other case is vsync-paced: a
        // `request_redraw` tells winit to deliver
        // `RedrawRequested` at the next platform vsync, and the
        // OS parks the thread until that event arrives. Busy-
        // polling between vsyncs here would burn CPU without
        // delivering more frames.
        if self.config.renderer.strategy.is_game() {
            route.request_redraw();
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            if route
                .window
                .screen
                .ctx()
                .current()
                .renderable_content
                .pending_update
                .is_dirty()
            {
                route.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}
