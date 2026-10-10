use super::*;

impl HostRepository {
    /// Persist a new empty group.
    pub fn create_group(&mut self, name: &str) {
        if self
            .commands
            .send(Command::CreateGroup(name.to_string()))
            .is_err()
        {
            self.error = Some("Host store is unavailable".to_string());
        }
    }

    /// Assign a stored host to a group, or clear membership (`group_id = None`).
    pub fn set_host_group(&mut self, host_id: &str, group_id: Option<&str>) {
        self.send(Command::SetHostGroup {
            host_id: host_id.to_string(),
            group_id: group_id.map(str::to_string),
        });
    }

    /// Reorder an ungrouped host before a root host or group.
    pub fn reorder_host(
        &mut self,
        host_id: &str,
        before_host_id: Option<&str>,
        before_group_id: Option<&str>,
    ) {
        self.send(Command::ReorderHost {
            host_id: host_id.to_string(),
            before_host_id: before_host_id.map(str::to_string),
            before_group_id: before_group_id.map(str::to_string),
        });
    }

    /// Reorder a group before another group or root host.
    pub fn reorder_group(
        &mut self,
        group_id: &str,
        before_group_id: Option<&str>,
        before_host_id: Option<&str>,
    ) {
        self.send(Command::ReorderGroup {
            group_id: group_id.to_string(),
            before_group_id: before_group_id.map(str::to_string),
            before_host_id: before_host_id.map(str::to_string),
        });
    }

    /// Unlock or create the vault (Settings passphrase).
    pub fn unlock_vault(&mut self, passphrase: &str) {
        self.unlock_vault_remember(passphrase, false);
    }

    /// Unlock or create the vault, optionally remembering the passphrase.
    pub fn unlock_vault_remember(&mut self, passphrase: &str, remember: bool) {
        self.send(Command::UnlockVault {
            passphrase: passphrase.to_string(),
            remember,
        });
    }

    /// Unseal the stored SSH password for `host_id`, if any.
    ///
    /// Returns `Ok(None)` when there is no sealed credential yet. Errors when
    /// the vault is locked or the id is invalid.
    pub fn resolve_host_password(&self, host_id: &str) -> Result<Option<String>, String> {
        let (reply_tx, reply_rx) = channel();
        self.commands
            .send(Command::ResolveHostPassword {
                id: host_id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| "Host store is unavailable".to_string())?;
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "Timed out reading the stored password".to_string())?
    }

    /// Ask the worker for `host_id`'s SFTP credentials without waiting:
    /// they arrive through [`Self::drain`], see
    /// [`Self::take_sftp_auth_replies`].
    pub fn request_sftp_auth(&mut self, host_id: &str, auth_method: &str) {
        // Not `send`: no busy indicator for a read.
        if self
            .commands
            .send(Command::ResolveSftpAuth {
                id: host_id.to_string(),
                auth_method: auth_method.to_string(),
            })
            .is_err()
        {
            self.sftp_auth_replies
                .push((host_id.to_string(), Err("Host store is unavailable".into())));
        }
    }

    /// SFTP credentials answered since the last call, `(host id, result)`.
    pub fn take_sftp_auth_replies(&mut self) -> Vec<(String, Result<SftpAuth, String>)> {
        std::mem::take(&mut self.sftp_auth_replies)
    }

    /// Load the managed OpenSSH private key for `host_id` (PEM + optional passphrase).
    ///
    /// Returns `Ok(None)` when the host is not key-auth or has no identity.
    pub fn resolve_host_identity(
        &self,
        host_id: &str,
    ) -> Result<Option<(String, Option<String>)>, String> {
        let (reply_tx, reply_rx) = channel();
        self.commands
            .send(Command::ResolveHostIdentity {
                id: host_id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| "Host store is unavailable".to_string())?;
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "Timed out reading the stored SSH key".to_string())?
    }

    /// Generate and persist a new Ed25519 managed SSH key.
    #[allow(dead_code)]
    pub fn create_ssh_key(&mut self, name: &str) {
        self.create_ssh_key_with_pem(name, None);
    }

    /// Generate, or import `pem` when provided.
    #[allow(dead_code)]
    pub fn create_ssh_key_with_pem(&mut self, name: &str, pem: Option<String>) {
        self.import_ssh_key(name, pem, None);
    }

    /// Generate, or import `pem` (decrypted with `passphrase`) when provided.
    pub fn import_ssh_key(
        &mut self,
        name: &str,
        pem: Option<String>,
        passphrase: Option<String>,
    ) {
        self.send(Command::CreateSshKey {
            name: name.to_string(),
            pem,
            passphrase,
        });
    }

    /// Soft-delete a managed SSH key by id.
    pub fn delete_ssh_key(&mut self, id: &str) {
        self.send(Command::DeleteSshKey { id: id.to_string() });
    }

    /// Soft-delete a stored SSH host by id.
    pub fn delete_host(&mut self, id: &str) {
        self.send(Command::DeleteHost { id: id.to_string() });
    }

    /// Soft-delete a host group by id.
    pub fn delete_group(&mut self, id: &str) {
        self.send(Command::DeleteGroup { id: id.to_string() });
    }

    /// Rename a stored SSH host.
    pub fn rename_host(&mut self, id: &str, name: &str) {
        self.send(Command::RenameHost {
            id: id.to_string(),
            name: name.to_string(),
        });
    }

    /// Rename a host group.
    pub fn rename_group(&mut self, id: &str, name: &str) {
        self.send(Command::RenameGroup {
            id: id.to_string(),
            name: name.to_string(),
        });
    }

    /// Persist without probing (tests). Prefer [`Self::probe_and_create`] in UI.
    #[allow(dead_code)]
    pub fn create(&mut self, draft: &HostDraft) -> Result<(), String> {
        match draft.normalize() {
            Ok(normalized) => {
                self.send(Command::Create(normalized));
                Ok(())
            }
            Err(message) => {
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Probe SSH with the draft credentials, then persist on success.
    pub fn probe_and_create(&mut self, draft: &HostDraft) -> Result<(), String> {
        match draft.normalize() {
            Ok(normalized) => {
                self.send(Command::ProbeAndCreate(normalized));
                Ok(())
            }
            Err(message) => {
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Probe SSH, then update an existing host.
    pub fn probe_and_update(
        &mut self,
        id: &str,
        draft: &HostDraft,
    ) -> Result<(), String> {
        let mut draft = draft.clone();
        draft.id = Some(id.to_string());
        match draft.normalize() {
            Ok(normalized) => {
                self.send(Command::ProbeAndUpdate {
                    id: id.to_string(),
                    draft: normalized,
                });
                Ok(())
            }
            Err(message) => {
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Background: SSH-exec OS probe and persist `os_id` for sidebar / tab icons.
    ///
    /// No-op for local / WSL row ids. Failures are silent (keep the generic SSH badge).
    pub fn detect_os(&mut self, id: &str) {
        if id == LOCAL_ID || id.starts_with(WSL_PREFIX) {
            return;
        }
        self.send(Command::DetectOs { id: id.to_string() });
    }

    fn send(&mut self, command: Command) {
        if self.commands.send(command).is_ok() {
            self.in_flight += 1;
        } else {
            self.error = Some("Host store is unavailable".to_string());
        }
    }

    /// Apply every answer that has arrived. Returns whether the caller should
    /// repaint.
    pub fn drain(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.events.try_recv() {
                Ok(HostEvent::Loaded(hosts)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.loading = false;
                    self.error = None;
                    self.hosts = hosts;
                    changed = true;
                }
                Ok(HostEvent::GroupsLoaded(groups)) => {
                    self.groups = groups;
                    changed = true;
                }
                Ok(HostEvent::IdentitiesLoaded(identities)) => {
                    self.identities = identities;
                    changed = true;
                }
                Ok(HostEvent::SyncStatus {
                    uri,
                    connected,
                    vault_unlocked,
                    status_line,
                    is_error,
                }) => {
                    self.sync_uri = uri;
                    self.sync_connected = connected;
                    self.vault_unlocked = vault_unlocked;
                    self.sync_status_line = status_line;
                    self.sync_status_is_error = is_error;
                    changed = true;
                }
                Ok(HostEvent::Platform(platform)) => {
                    self.platform = platform;
                    changed = true;
                }
                Ok(HostEvent::Stored(label)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.notice = Some(label);
                    self.error = None;
                    changed = true;
                }
                Ok(HostEvent::SftpAuth { id, result }) => {
                    self.sftp_auth_replies.push((id, result));
                    changed = true;
                }
                Ok(HostEvent::Failed(message)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.loading = false;
                    self.error = Some(message);
                    changed = true;
                }
                Ok(HostEvent::SnippetsLoaded(snippets)) => {
                    self.snippet_items = snippets;
                }
                Ok(HostEvent::VaultStatus {
                    unlocked,
                    configured,
                    message,
                }) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.vault_unlocked = unlocked;
                    self.vault_configured = configured;
                    if let Some(msg) = message {
                        // Surface vault unlock/create results on the SqlSync
                        // status row (not the host-list notice band).
                        self.sync_status_line = msg.clone();
                        self.sync_status_is_error = !unlocked;
                        self.vault_message = Some(msg);
                    }
                    changed = true;
                }
                Ok(HostEvent::OsDetected { id, os_id }) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    if apply_os_detected(&mut self.hosts, &id, &os_id) {
                        changed = true;
                    }
                }
                Ok(HostEvent::CollapsedGroupsLoaded(set)) => {
                    // Accept only the first load; local state is authoritative after seed.
                    if !self.collapsed_groups_seeded {
                        self.collapsed_groups_seed = Some(set);
                        self.collapsed_groups_seeded = true;
                        changed = true;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.in_flight = 0;
                    self.loading = false;
                    break;
                }
            }
        }
        changed
    }
}
