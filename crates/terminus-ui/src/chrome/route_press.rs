use super::*;

impl Chrome {
    pub(super) fn route_press(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ChromeAction {
        // A confirmation sits above every other dialog.
        if let Some(prompt) = self.confirm.as_mut() {
            return match prompt.press((window_width, window_height), x, y) {
                ConfirmOutcome::Confirm => match self.confirm.take() {
                    Some(p) => Self::confirm_action(p.action),
                    None => ChromeAction::Consumed,
                },
                ConfirmOutcome::Cancel => {
                    self.confirm = None;
                    ChromeAction::Consumed
                }
                ConfirmOutcome::Idle | ConfirmOutcome::Changed => ChromeAction::Consumed,
            };
        }

        // An open context menu eats the next left click: select or dismiss.
        if let Some(action) = self.route_context_menu_press(x, y) {
            return action;
        }

        // Vault unlock sits above every other dialog: a sealed secret was
        // requested and nothing else can proceed until the user answers.
        if self.vault_unlock.is_open() {
            let layout = VaultUnlockLayout::for_prompt(
                window_width,
                window_height,
                &self.vault_unlock,
            );
            return match layout.hit_test_labels(x, y, self.vault_unlock.action_label()) {
                VaultUnlockHit::Field => {
                    self.vault_unlock.focus_passphrase();
                    ChromeAction::Consumed
                }
                VaultUnlockHit::ConfirmField => {
                    self.vault_unlock.focus_confirm();
                    ChromeAction::Consumed
                }
                VaultUnlockHit::ToggleVisible => {
                    self.vault_unlock.toggle_visible();
                    ChromeAction::Consumed
                }
                VaultUnlockHit::ToggleRemember => {
                    self.vault_unlock.toggle_remember();
                    ChromeAction::Consumed
                }
                VaultUnlockHit::Unlock => ChromeAction::SubmitVaultUnlock,
                VaultUnlockHit::Cancel => {
                    self.vault_unlock.close();
                    ChromeAction::Consumed
                }
                VaultUnlockHit::Consume => ChromeAction::Consumed,
            };
        }

        // Settings modal sits above connection and add-host.
        if self.settings.open {
            return match self.settings.hit_test(window_width, window_height, x, y) {
                SettingsHit::Close | SettingsHit::Done => {
                    self.settings.close();
                    ChromeAction::DismissSettings
                }
                SettingsHit::Tab(tab) => {
                    self.settings.close_engine_menu();
                    self.settings.open_tab(tab);
                    // Keep the Settings page header on the same section.
                    if self.shell.view().is_settings() {
                        let page = match tab {
                            SettingsTab::Keys => crate::shell::SettingsPage::Keys,
                            SettingsTab::SqlSync => crate::shell::SettingsPage::Sync,
                        };
                        self.shell
                            .workspace
                            .show(crate::shell::WorkspaceView::Settings(page));
                    }
                    ChromeAction::Consumed
                }
                SettingsHit::FocusUri => {
                    self.settings.focus_uri();
                    ChromeAction::FocusSqlUri
                }
                SettingsHit::FocusPassphrase => {
                    self.settings.focus_passphrase();
                    ChromeAction::FocusSqlPassphrase
                }
                SettingsHit::TogglePassphrase => {
                    self.settings.toggle_passphrase_visible();
                    ChromeAction::Consumed
                }
                SettingsHit::ToggleEngineMenu => {
                    self.settings.toggle_engine_menu();
                    ChromeAction::Consumed
                }
                SettingsHit::SelectEngine(index) => {
                    self.settings.select_engine(index);
                    ChromeAction::Consumed
                }
                SettingsHit::UnlockVault => {
                    self.settings.close_engine_menu();
                    ChromeAction::UnlockVault
                }
                SettingsHit::ForgetPassphrase => {
                    self.settings.close_engine_menu();
                    ChromeAction::ForgetVaultPassphrase
                }
                SettingsHit::TestSync => {
                    self.settings.close_engine_menu();
                    ChromeAction::TestSync
                }
                SettingsHit::NewKey => {
                    self.settings.open_key_draft();
                    ChromeAction::FocusKeyDraft
                }
                SettingsHit::FocusKeyDraft => {
                    self.settings.focus_key_draft();
                    ChromeAction::FocusKeyDraft
                }
                SettingsHit::FocusKeyPem => {
                    self.settings.focus_key_pem();
                    ChromeAction::FocusKeyDraft
                }
                SettingsHit::FocusKeyPassphrase => {
                    self.settings.focus_key_passphrase();
                    ChromeAction::FocusKeyDraft
                }
                SettingsHit::GenerateKey => match self.settings.take_key_draft_label() {
                    Ok(_name) => ChromeAction::GenerateSshKey,
                    Err(_) => ChromeAction::Consumed,
                },
                SettingsHit::CancelKeyDraft => {
                    self.settings.close_key_draft();
                    ChromeAction::Consumed
                }
                SettingsHit::CopyPublicKey(index) => {
                    match self.settings.keys.get(index) {
                        Some(key) if !key.public_key.is_empty() => {
                            self.settings.keys_notice = Some(format!(
                                "Copied {}'s public key. Add it to ~/.ssh/authorized_keys on your server.",
                                key.name
                            ));
                            ChromeAction::CopyPublicKey(key.public_key.clone())
                        }
                        _ => ChromeAction::Consumed,
                    }
                }
                SettingsHit::DeleteKey(index) => {
                    if let Some(key) = self.settings.keys.get(index) {
                        ChromeAction::DeleteSshKey(key.id.clone())
                    } else {
                        ChromeAction::Consumed
                    }
                }
                SettingsHit::Consume => {
                    self.settings.close_engine_menu();
                    self.settings.clear_sql_focus();
                    self.settings.key_draft_focused = false;
                    self.settings.key_draft_pem_focused = false;
                    ChromeAction::Consumed
                }
            };
        }

        // Connection progress owns the Terminal content: clicks there never
        // fall through to the terminal behind it. The sidebar and header
        // stay live.
        if let Some(hit) = self.connection_hit(x, y) {
            return match hit {
                ConnectionHit::ToggleLogs => {
                    if let Some(conn) = self.connection.as_mut() {
                        conn.toggle_logs();
                    }
                    ChromeAction::Consumed
                }
                ConnectionHit::Close => ChromeAction::DismissConnection,
                ConnectionHit::Consume => ChromeAction::Consumed,
            };
        }

        // The lost-connection card owns the dead terminal the same way.
        if let Some(area) = self.lost_hit(x, y) {
            if let Some(lost) = self.lost.as_mut() {
                let outcome = lost.press(area, x, y);
                return Self::lost_action(lost.route_id, outcome);
            }
        }

        if self.snippet_form.is_open() {
            let layout = crate::dialog_form::DialogFormLayout::compute(
                &self.snippet_form.inner,
                window_width,
                window_height,
            );
            let action = match layout.hit_test(x, y) {
                Some(hit) => match hit {
                    crate::dialog_form::DynamicFormHit::Field(i) => {
                        self.snippet_form.inner.focused_index = i;
                        ChromeAction::Consumed
                    }
                    crate::dialog_form::DynamicFormHit::Save => {
                        ChromeAction::SubmitAddSnippet(self.snippet_form.values())
                    }
                    crate::dialog_form::DynamicFormHit::Cancel
                    | crate::dialog_form::DynamicFormHit::Background => {
                        self.snippet_form.inner.closing = true;
                        ChromeAction::Consumed
                    }
                },
                None => ChromeAction::Ignored,
            };
            if action != ChromeAction::Ignored {
                return action;
            }
        }

        if self.form.is_open() {
            let layout = self.dialog_layout(window_width, window_height);
            let dialog = layout.rect(self.form.height());
            let on_menu = layout
                .menu_rect(&self.form)
                .is_some_and(|m| m.contains(x, y));
            if !dialog.contains(x, y) && !on_menu {
                self.form.close();
                return ChromeAction::Consumed;
            }
            return match layout.hit_test(&self.form, x, y) {
                AddHostHit::Field(field) => {
                    self.form.focus_field(field);
                    ChromeAction::Consumed
                }
                AddHostHit::StepPill(step) => {
                    if step.index() < self.form.step().index() {
                        self.form.set_step(step);
                    } else if step.index() > self.form.step().index() {
                        let mut curr = self.form.step();
                        while curr.index() < step.index() {
                            if !self.form.next_step() {
                                break;
                            }
                            curr = self.form.step();
                        }
                    }
                    ChromeAction::Consumed
                }
                AddHostHit::SelectAuth(index) => {
                    self.form.select_auth_method(index);
                    ChromeAction::Consumed
                }
                AddHostHit::ToggleIdentityMenu => {
                    self.form.toggle_identity_menu();
                    ChromeAction::Consumed
                }
                AddHostHit::SelectIdentity(index) => {
                    self.form.select_identity(index);
                    ChromeAction::Consumed
                }
                AddHostHit::ToggleGroupMenu => {
                    self.form.toggle_group_menu();
                    ChromeAction::Consumed
                }
                AddHostHit::SelectGroup(index) => {
                    self.form.select_group(index);
                    ChromeAction::Consumed
                }
                AddHostHit::GenerateKey => {
                    // The key draft opens over the wizard; a new key shows
                    // up in the select as soon as the store reports it.
                    self.open_settings(SettingsTab::Keys);
                    self.settings.open_key_draft();
                    ChromeAction::FocusKeyDraft
                }
                AddHostHit::TogglePasswordVisible => {
                    self.form.toggle_password_visible();
                    ChromeAction::Consumed
                }
                AddHostHit::Back => {
                    self.form.prev_step();
                    ChromeAction::Consumed
                }
                AddHostHit::CopyError => match self.form.error() {
                    Some(error) => ChromeAction::CopyText(error.to_string()),
                    None => ChromeAction::Consumed,
                },
                AddHostHit::Next => {
                    self.form.next_step();
                    ChromeAction::Consumed
                }
                AddHostHit::Cancel | AddHostHit::Close => {
                    self.form.close();
                    ChromeAction::Consumed
                }
                AddHostHit::Connect => ChromeAction::SubmitHostForm,
                AddHostHit::Consume => {
                    self.form.close_menu();
                    ChromeAction::Consumed
                }
            };
        }

        // Sidebar chrome, header and pills come before the machine list.
        if let Some(hit) = self.shell.hit_test(x, y) {
            return self.press_shell(hit);
        }

        let origin_y = self.origin_y();
        let chrome_height = (window_height - origin_y).max(0.0);
        if !self.panel.rect(origin_y, chrome_height).contains(x, y) {
            return ChromeAction::Ignored;
        }

        if self.snippets_visible() {
            return match self.snippets.hit_test(origin_y, chrome_height, x, y) {
                Some(SnippetHit::Item(index)) => match self.snippets.items.get(index) {
                    Some(item) => ChromeAction::RunSnippet(item.cmd.clone()),
                    None => ChromeAction::Consumed,
                },
                Some(SnippetHit::AddButton) => {
                    self.snippet_form.inner.closing = false;
                    ChromeAction::OpenAddSnippet
                }
                Some(SnippetHit::DeleteButton(index)) => {
                    match self.snippets.items.get(index) {
                        Some(item) => ChromeAction::DeleteSnippet(item.id.clone()),
                        None => ChromeAction::Consumed,
                    }
                }
                Some(SnippetHit::Background) => ChromeAction::Consumed,
                None => ChromeAction::Ignored,
            };
        }

        match self.panel.hit_test(origin_y, chrome_height, x, y) {
            Some(PanelHit::Search) => {
                self.panel.filter_focused = true;
                self.panel.new_group_focused = false;
                ChromeAction::FocusSearch
            }
            hit => {
                // A fresh press cancels any leftover armed drag.
                self.panel.clear_host_drag();
                // Any other drawer press (or miss) drops the search caret.
                self.panel.filter_focused = false;
                if !matches!(
                    hit,
                    Some(PanelHit::NewGroupField)
                        | Some(PanelHit::NewGroupCreate)
                        | Some(PanelHit::NewGroupCancel)
                        | Some(PanelHit::NewGroup)
                ) {
                    self.panel.new_group_focused = false;
                }
                match hit {
                    Some(PanelHit::Group(index)) if self.hosts_visible() => {
                        match self.panel.rows.get(index) {
                            Some(Row::Group {
                                id,
                                name,
                                host_count,
                                ..
                            }) => {
                                let card = self.panel.card_rect(self.origin_y(), index);
                                let endpoint = if *host_count == 0 {
                                    "Empty group".to_string()
                                } else if *host_count == 1 {
                                    "1 host".to_string()
                                } else {
                                    format!("{host_count} hosts")
                                };
                                self.panel.host_drag = Some(crate::sidebar::HostDrag {
                                    host_id: id.clone(),
                                    host_name: name.clone(),
                                    endpoint,
                                    kind: crate::sidebar::HostDragKind::Group,
                                    row_index: index,
                                    press_x: x,
                                    press_y: y,
                                    current_x: x,
                                    current_y: y,
                                    grab_dx: x - card.x,
                                    grab_dy: y - card.y,
                                    source_rect: card,
                                    ghost_rect: card,
                                    phase: crate::sidebar::HostDragPhase::Armed,
                                    drop_target: None,
                                });
                                ChromeAction::Consumed
                            }
                            _ => ChromeAction::Consumed,
                        }
                    }
                    Some(PanelHit::Item(index)) if self.hosts_visible() => {
                        self.panel.selected = Some(index);
                        match self.panel.rows.get(index).and_then(Row::host) {
                            // Stored SSH hosts: arm a drag; open on release if
                            // the pointer never moved past the threshold.
                            Some(item) if item.stored => {
                                let card = self.panel.card_rect(self.origin_y(), index);
                                self.panel.host_drag = Some(crate::sidebar::HostDrag {
                                    host_id: item.id.clone(),
                                    host_name: item.name.clone(),
                                    endpoint: item.endpoint.clone(),
                                    kind: crate::sidebar::HostDragKind::Host,
                                    row_index: index,
                                    press_x: x,
                                    press_y: y,
                                    current_x: x,
                                    current_y: y,
                                    grab_dx: x - card.x,
                                    grab_dy: y - card.y,
                                    source_rect: card,
                                    ghost_rect: card,
                                    phase: crate::sidebar::HostDragPhase::Armed,
                                    drop_target: None,
                                });
                                ChromeAction::Consumed
                            }
                            Some(item) => ChromeAction::OpenHost(item.id.clone()),
                            None => ChromeAction::Consumed,
                        }
                    }
                    Some(PanelHit::Session(index)) if self.hosts_visible() => {
                        match self.panel.rows.get(index).and_then(Row::session) {
                            Some(session) => {
                                self.panel.selected_session = Some(session.tab_index);
                                ChromeAction::OpenSession(session.tab_index)
                            }
                            None => ChromeAction::Consumed,
                        }
                    }
                    Some(PanelHit::CloseSession(index)) if self.hosts_visible() => {
                        match self.panel.rows.get(index).and_then(Row::session) {
                            Some(session) => {
                                ChromeAction::CloseSession(session.tab_index)
                            }
                            None => ChromeAction::Consumed,
                        }
                    }
                    Some(PanelHit::HostAddSession(index)) if self.hosts_visible() => {
                        match self.panel.rows.get(index).and_then(Row::host) {
                            Some(host) => ChromeAction::AddHostSession(host.id.clone()),
                            None => ChromeAction::Consumed,
                        }
                    }
                    Some(PanelHit::ToggleHost(index)) if self.hosts_visible() => {
                        match self.panel.rows.get(index).and_then(Row::host) {
                            Some(host) => {
                                let id = host.id.clone();
                                self.panel.toggle_host_collapsed(&id);
                                ChromeAction::ToggleHost(id)
                            }
                            None => ChromeAction::Consumed,
                        }
                    }
                    Some(PanelHit::AddHost) => ChromeAction::AddHost,
                    Some(PanelHit::NewGroup) => {
                        self.panel.toggle_new_group_form();
                        ChromeAction::NewGroup
                    }
                    Some(PanelHit::NewGroupCreate) => ChromeAction::CreateGroup,
                    Some(PanelHit::NewGroupCancel) => {
                        self.panel.close_new_group_form();
                        ChromeAction::CancelNewGroup
                    }
                    Some(PanelHit::NewGroupField) => {
                        self.panel.new_group_focused = true;
                        ChromeAction::FocusNewGroup
                    }
                    Some(_) => ChromeAction::Consumed,
                    None => ChromeAction::Ignored,
                }
            }
        }
    }
}
