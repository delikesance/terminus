use super::Application;
use rio_window::event_loop::ActiveEventLoop;
use terminus_ui::chrome::ChromeAction;

impl Application<'_> {
    pub(super) fn chrome_action_3(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        action: ChromeAction,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        match action {
            ChromeAction::DismissConnection => {
                route.window.screen.force_end_connecting();
                route.request_overlay_redraw();
            }
            lost @ (ChromeAction::ReconnectSession(_)
            | ChromeAction::CloseLostSession(_)) => {
                route
                    .window
                    .screen
                    .run_lost_session_action(lost, &mut self.router.clipboard);
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
            ChromeAction::DismissSettings => {
                route.request_overlay_redraw();
            }
            ChromeAction::UnlockVault => {
                route.window.screen.settings_unlock_vault();
                route.request_overlay_redraw();
            }
            ChromeAction::ForgetVaultPassphrase => {
                crate::vault_remember::forget_passphrase();
                route
                    .window
                    .screen
                    .chrome
                    .settings
                    .set_passphrase_remembered(false);
                route.window.screen.chrome.vault_unlock.set_remember(false);
                route.window.screen.chrome.settings.sync_status =
                    "Saved passphrase forgotten".into();
                route.request_overlay_redraw();
            }
            ChromeAction::TestSync => {
                let uri = route.window.screen.chrome.settings.sql_uri.value.clone();
                route.window.screen.host_store.test_sync(&uri);
                route.request_overlay_redraw();
            }
            ChromeAction::GenerateSshKey => {
                let name = route.window.screen.chrome.settings.key_label.value.clone();
                let pem = route.window.screen.chrome.settings.key_pem.value.clone();
                if name.trim().is_empty() {
                    route.window.screen.chrome.settings.key_draft_error =
                        Some("Enter a label for the new SSH key".into());
                } else {
                    let pem = if pem.trim().is_empty() {
                        None
                    } else {
                        Some(pem)
                    };
                    let passphrase =
                        route.window.screen.chrome.settings.key_draft_passphrase();
                    route
                        .window
                        .screen
                        .host_store
                        .import_ssh_key(&name, pem, passphrase);
                }
                route.request_overlay_redraw();
            }
            ChromeAction::CopySshCommand(id) => {
                let screen = &mut route.window.screen;
                if let Some(host) = screen.host_store.hosts().iter().find(|h| h.id == id)
                {
                    let command = host.ssh_command();
                    screen.chrome.panel.notice = Some(format!("Copied: {command}"));
                    self.router
                        .clipboard
                        .set(rio_backend::clipboard::ClipboardType::Clipboard, command);
                }
                route.request_overlay_redraw();
            }
            ChromeAction::CopyText(text) => {
                self.router
                    .clipboard
                    .set(rio_backend::clipboard::ClipboardType::Clipboard, text);
                route.request_overlay_redraw();
            }
            ChromeAction::CopyPublicKey(key) => {
                self.router
                    .clipboard
                    .set(rio_backend::clipboard::ClipboardType::Clipboard, key);
                route.request_overlay_redraw();
            }
            ChromeAction::DeleteSshKey(id) => {
                route.window.screen.host_store.delete_ssh_key(&id);
                route.request_overlay_redraw();
            }
            ChromeAction::DeleteHost(id) => {
                route
                    .window
                    .screen
                    .delete_host_closing_sessions(&id, &mut self.router.clipboard);
                route.request_overlay_redraw();
            }
            ChromeAction::OpenGroup(id) => {
                let result = route
                    .window
                    .screen
                    .open_group_sessions(&id, &mut self.router.clipboard);
                route.window.screen.chrome.panel.error = result.err();
                route.request_overlay_redraw();
            }
            ChromeAction::CloseGroup(id) => {
                route
                    .window
                    .screen
                    .close_group_sessions(&id, &mut self.router.clipboard);
                route.request_overlay_redraw();
            }
            ChromeAction::DeleteGroup(id) => {
                route.window.screen.host_store.delete_group(&id);
                route.request_overlay_redraw();
            }
            ChromeAction::EditHost(id) => {
                let Some(host) = route
                    .window
                    .screen
                    .host_store
                    .hosts()
                    .iter()
                    .find(|h| h.id == id)
                    .cloned()
                else {
                    route.request_overlay_redraw();
                    return;
                };
                let port = if host.port == 22 {
                    String::new()
                } else {
                    host.port.to_string()
                };
                let values = terminus_ui::add_host::HostFormValues {
                    name: host.name,
                    hostname: host.hostname,
                    username: host.username,
                    port,
                    auth_method: if host.auth_method.is_empty() {
                        "key".into()
                    } else {
                        host.auth_method
                    },
                    identity_id: host.identity_id,
                    password: String::new(),
                    group_id: host.group_id,
                    tags: host.tags.join(", "),
                    notes: host.notes,
                };
                route.window.screen.chrome.open_edit_host(values, id);
                route.request_overlay_redraw();
            }
            other => self.chrome_action_4(event_loop, window_id, other),
        }
    }
}
