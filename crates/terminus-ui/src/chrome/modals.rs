use super::*;

impl Chrome {
    pub fn add_host_is_open(&self) -> bool {
        self.form.is_open()
    }

    pub fn add_snippet_is_open(&self) -> bool {
        self.snippet_form.is_open()
    }

    pub fn settings_is_open(&self) -> bool {
        self.settings.open
    }

    pub fn vault_unlock_is_open(&self) -> bool {
        self.vault_unlock.is_open()
    }

    /// Overlay dialogs from back to front. Sugarloaf flattens one overlay
    /// layer as (all quads) → (all text), so only the **front** entry may
    /// emit glyphs; lower entries paint shell/scrim quads only.
    pub fn modal_paint_stack(&self) -> Vec<ModalPaintLayer> {
        // Connection progress is not here: the shell paints it inside the
        // Terminal content (see [`Self::connection_area`]).
        let mut stack = Vec::new();
        if self.form.is_open() {
            stack.push(ModalPaintLayer::HostEditor);
        }
        if self.snippet_form.is_open() {
            stack.push(ModalPaintLayer::AddSnippet);
        }
        if self.settings.open {
            stack.push(ModalPaintLayer::Settings);
        }
        if self.vault_unlock.is_open() {
            stack.push(ModalPaintLayer::VaultUnlock);
        }
        if self.confirm.is_some() {
            stack.push(ModalPaintLayer::Confirm);
        }
        stack
    }

    /// Front-most open overlay dialog, if any.
    pub fn top_modal_paint(&self) -> Option<ModalPaintLayer> {
        self.modal_paint_stack().last().copied()
    }

    pub fn confirm_is_open(&self) -> bool {
        self.confirm.is_some()
    }

    /// Show a confirmation dialog; its action runs only on Confirm.
    pub fn open_confirm(&mut self, prompt: ConfirmPrompt) {
        self.context_menu = None;
        self.confirm = Some(prompt);
    }

    pub(super) fn confirm_action(action: ConfirmAction) -> ChromeAction {
        match action {
            ConfirmAction::DeleteHost(id) => ChromeAction::DeleteHost(id),
            ConfirmAction::DeleteGroup(id) => ChromeAction::DeleteGroup(id),
            ConfirmAction::SftpDelete => ChromeAction::SftpDeleteConfirmed,
            ConfirmAction::Quit => ChromeAction::Consumed,
        }
    }

    /// Keyboard on the confirmation dialog: `Some` when it is open (the
    /// action is `Consumed` unless the user confirmed), `None` otherwise.
    pub fn handle_confirm_key(&mut self, key: DialogKey) -> Option<ChromeAction> {
        let prompt = self.confirm.as_mut()?;
        Some(match prompt.key(key) {
            ConfirmOutcome::Confirm => {
                let action = self.confirm.take()?.action;
                Self::confirm_action(action)
            }
            ConfirmOutcome::Cancel => {
                self.confirm = None;
                ChromeAction::Consumed
            }
            ConfirmOutcome::Idle | ConfirmOutcome::Changed => ChromeAction::Consumed,
        })
    }

    /// Ask for the vault passphrase, then retry `pending` after unlock.
    pub fn open_vault_unlock(&mut self, pending: PendingVaultAction) {
        let host_id = match &pending {
            PendingVaultAction::OpenHost(id) | PendingVaultAction::AddHostSession(id) => {
                Some(id.clone())
            }
            PendingVaultAction::OpenSftp { host_id, .. } => Some(host_id.clone()),
            _ => None,
        };
        self.vault_unlock.open(pending);
        let label = host_id.and_then(|id| {
            self.panel
                .rows
                .iter()
                .filter_map(Row::host)
                .find(|h| h.id == id)
                .map(|h| h.name.clone())
        });
        self.vault_unlock.set_host_label(label);
        self.vault_unlock.set_creating(!self.vault_configured);
    }

    /// Where connection progress is shown: the shell's content rect (the
    /// Terminal view's area, under the session pills).
    pub fn connection_area(&self) -> crate::geom::Rect {
        self.shell.content_rect()
    }

    /// Where the lost-connection card is shown: the Terminal view's content,
    /// unless connection progress already covers it. `None` when there is
    /// no card, another view is in front, or a dialog sits over it (that
    /// dialog owns the pointer and the keys).
    pub fn lost_area(&self) -> Option<crate::geom::Rect> {
        (self.lost.is_some()
            && self.connection.is_none()
            && self.shell.view().shows_terminal()
            && self.modal_paint_stack().is_empty())
        .then(|| self.lost_pane.unwrap_or_else(|| self.connection_area()))
    }

    pub(super) fn lost_action(route_id: usize, outcome: LostOutcome) -> ChromeAction {
        match outcome {
            LostOutcome::Reconnect => ChromeAction::ReconnectSession(route_id),
            LostOutcome::Close => ChromeAction::CloseLostSession(route_id),
            LostOutcome::Idle | LostOutcome::Changed => ChromeAction::Consumed,
        }
    }

    /// Whether keys go to the lost-connection card (shown, and its pane is
    /// the focused one).
    pub fn lost_takes_keys(&self) -> bool {
        self.lost_area().is_some() && !self.lost_pane_unfocused
    }

    /// Keyboard on the lost-connection card: `Some` while it is shown (the
    /// session behind it is gone, so every key stops here), `None` otherwise.
    pub fn handle_lost_key(&mut self, key: DialogKey) -> Option<ChromeAction> {
        if !self.lost_takes_keys() {
            return None;
        }
        let lost = self.lost.as_mut()?;
        let outcome = lost.key(key);
        Some(Self::lost_action(lost.route_id, outcome))
    }

    /// Whether `(x, y)` falls on the lost-connection card's area.
    pub(super) fn lost_hit(&self, x: f32, y: f32) -> Option<crate::geom::Rect> {
        self.lost_area().filter(|area| area.contains(x, y))
    }

    /// What a pointer at `(x, y)` hits on the connection progress, or
    /// `None` when there is none or the pointer is outside its area.
    pub(super) fn connection_hit(&self, x: f32, y: f32) -> Option<ConnectionHit> {
        let conn = self.connection.as_ref()?;
        // Painted on the Terminal view only; other views keep their input.
        if !self.shell.view().shows_terminal() {
            return None;
        }
        let area = self.connection_area();
        area.contains(x, y).then(|| conn.hit_test_in(area, x, y))
    }

    /// Open the add-host editor.
    pub fn open_add_host(&mut self) {
        self.panel_visible = true;
        self.form.open();
    }

    /// Open the host editor prefilled for an existing host.
    pub fn open_edit_host(
        &mut self,
        values: crate::add_host::HostFormValues,
        host_id: String,
    ) {
        self.panel_visible = true;
        self.form.open_edit(values, host_id);
    }

    pub fn open_settings(&mut self, tab: SettingsTab) {
        self.settings.open_tab(tab);
        self.settings.keys_notice = None;
    }
}
