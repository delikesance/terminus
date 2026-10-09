//! The chrome as one object: shell (sidebar chrome, header, pills, views)
//! + host panel + dialogs.
//!
//! Everything the mouse and the keyboard can do to the chrome is routed
//! through here, and everything the chrome reserves from the terminal's
//! area is answered by [`Chrome::reserved_width`]. The painters read
//! this state and never own any of it, so a repaint can never disagree
//! with a hit-test.

use crate::add_host::{AddHostForm, AddHostHit, FormInput, FormOutcome};
use crate::components::overlay::DialogKey;
use crate::confirm::{ConfirmAction, ConfirmOutcome, ConfirmPrompt};
use crate::connection::{ConnectionHit, ConnectionSequence};
use crate::context_menu::{ContextAction, ContextMenu, ContextMenuHit};
use crate::lost_session::{LostOutcome, LostSession};
use crate::settings::{SettingsHit, SettingsModal, SettingsTab};
use crate::sidebar::{HostItem, HostPanel, PanelHit, Row};
use crate::snippets::{SnippetHit, SnippetsPanel};
use crate::vault_unlock::{
    PendingVaultAction, VaultUnlockHit, VaultUnlockLayout, VaultUnlockPrompt,
};

/// Overlay dialog paint / input stacking (back → front).
///
/// Sugarloaf composites **one** overlay pass as all overlay quads, then all
/// overlay text. Nested modals therefore cannot rely on paint call order
/// alone: lower-modal glyphs would float above a higher modal's panel.
/// Painters must emit glyphs only for [`Chrome::top_modal_paint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalPaintLayer {
    HostEditor,
    AddSnippet,
    Settings,
    VaultUnlock,
    /// Delete-host / delete-group confirmation (above everything).
    Confirm,
}

/// Mouse cursor affordance for chrome hit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChromeCursor {
    #[default]
    Default,
    /// Buttons, rows, tabs, links.
    Pointer,
    /// Editable text fields.
    Text,
}

/// What a mouse press on the chrome did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChromeAction {
    /// The press missed the chrome; the terminal gets it.
    Ignored,
    /// The press hit the chrome and was consumed.
    Consumed,
    /// The add-host row was pressed: open the editor.
    AddHost,
    OpenAddSnippet,
    SubmitAddSnippet(crate::add_snippet::SnippetFormValues),
    DeleteSnippet(String),
    /// The new-group control was pressed: toggle the inline form.
    NewGroup,
    /// Confirm creating a group from the inline form.
    CreateGroup,
    /// Cancel the inline new-group form.
    CancelNewGroup,
    /// A group header was pressed: toggle collapse.
    ToggleGroup(String),
    /// The search field was pressed: focus it.
    FocusSearch,
    /// The new-group name field was pressed: focus it.
    FocusNewGroup,
    /// A host row was pressed: connect to that host.
    OpenHost(String),
    /// Open session on a host: focus that tab.
    OpenSession(usize),
    /// Close × on a session row.
    CloseSession(usize),
    /// "+" on a host: open another session for that host.
    AddHostSession(String),
    /// Footer Connect on the add-host dialog (same as Enter).
    SubmitHostForm,
    /// Vault unlock prompt: submit passphrase.
    SubmitVaultUnlock,
    /// Settings: unlock / create vault with the SQL Sync passphrase field.
    UnlockVault,
    /// Settings: clear a remembered vault passphrase from the keyring.
    ForgetVaultPassphrase,
    /// Settings: persist remote URI and run SyncEngine::sync_now.
    TestSync,
    /// Settings: generate a new Ed25519 managed SSH key.
    GenerateSshKey,
    /// Settings: soft-delete a managed SSH key by id.
    DeleteSshKey(String),
    /// Put this OpenSSH public key on the clipboard.
    CopyPublicKey(String),
    /// Put this stored host's `ssh …` command on the clipboard.
    CopySshCommand(String),
    /// Put this text (an error message, say) on the clipboard.
    CopyText(String),
    /// Soft-delete a stored host (context menu).
    DeleteHost(String),
    /// Soft-delete a host group (context menu).
    DeleteGroup(String),
    /// Edit a stored host in the add-host dialog (context menu).
    EditHost(String),
    /// Open the dual-pane SFTP browser for a stored host (context menu).
    OpenSftp(String),
    /// Open a stored host in the other SFTP pane (context menu).
    OpenSftpOtherPane(String),
    /// Begin renaming a stored host (context menu).
    RenameHost(String),
    /// Begin renaming a host group (context menu).
    RenameGroup(String),
    /// Open a session on every host of a group (context menu).
    OpenGroup(String),
    /// Close the sessions of every host of a group (context menu).
    CloseGroup(String),
    /// Commit an inline rename with the draft name.
    CommitRename {
        id: String,
        is_group: bool,
        name: String,
    },
    /// Context menu: copy.
    ContextCopy,
    /// Context menu: paste.
    ContextPaste,
    /// SFTP context: new folder on focused side.
    SftpNewFolder,
    /// SFTP context: rename selection.
    SftpRename,
    /// SFTP context: delete selection.
    SftpDelete,
    /// The SFTP delete confirmation was accepted.
    SftpDeleteConfirmed,
    /// SFTP context: transfer selection to the other pane.
    SftpTransfer,
    /// SFTP context: edit remote file via temp + default app.
    SftpEdit,
    /// SFTP context: enter selected directory.
    SftpOpen,
    /// SFTP context: refresh focused pane.
    SftpRefresh,
    /// Settings: focus the connection URI field.
    FocusSqlUri,
    /// Settings: focus the vault passphrase field.
    FocusSqlPassphrase,
    /// Settings: focus the generate-key label field.
    FocusKeyDraft,
    /// Expand/collapse sessions under a host.
    ToggleHost(String),
    /// Move a stored host into a group (`Some`) or out to the root list (`None`).
    SetHostGroup {
        host_id: String,
        group_id: Option<String>,
    },
    /// Place a host before another ungrouped host or before a group.
    ReorderHost {
        host_id: String,
        before_host_id: Option<String>,
        before_group_id: Option<String>,
    },
    /// Place a group before another group or before a root host.
    ReorderGroup {
        group_id: String,
        before_group_id: Option<String>,
        before_host_id: Option<String>,
    },
    /// Close the connection-progress modal.
    DismissConnection,
    /// Lost-connection card: reopen the host of this terminal (route id)
    /// in place of the dead tab.
    ReconnectSession(usize),
    /// Lost-connection card: close the dead tab of this terminal (route id).
    CloseLostSession(usize),
    /// Run a snippet command in the active terminal.
    RunSnippet(String),
    /// Settings modal was dismissed.
    DismissSettings,
    /// Shell: open the command palette ("Search or run…").
    OpenPalette,
    /// Shell: the workspace view changed (header tab, brand, Settings).
    ViewChanged(crate::shell::WorkspaceView),
    /// Shell: a header window control was pressed.
    WindowControl(crate::shell::WindowButton),
    /// Shell: press on the empty header — drag the window (double-click
    /// maximizes).
    WindowDrag,
    /// Shell: split the focused session (`down` = horizontal divider).
    Split {
        down: bool,
    },
}

/// Chrome state for one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    pub panel: HostPanel,
    pub snippets: SnippetsPanel,
    pub settings: SettingsModal,
    pub form: AddHostForm,
    pub snippet_form: crate::add_snippet::AddSnippetForm,
    /// Prompt when a sealed secret is needed and the vault is locked.
    pub vault_unlock: VaultUnlockPrompt,
    /// Destructive-action confirmation (delete host / group), when open.
    pub confirm: Option<ConfirmPrompt>,
    /// Whether a vault already exists; when not, the prompt creates one.
    pub vault_configured: bool,
    /// Live SSH/WSL connecting modal, when a session is starting.
    pub connection: Option<ConnectionSequence>,
    /// "Connection lost" card for the session in front, when its link
    /// dropped. The frontend keeps one per dead tab and mirrors the front
    /// tab's here.
    pub lost: Option<LostSession>,
    /// Where `lost`'s card sits when its session is one pane of a split
    /// tab: that pane's rect. `None` for a whole-tab session, whose card
    /// covers the Terminal content.
    pub lost_pane: Option<crate::geom::Rect>,
    /// The dead pane is not the focused one: its card is clickable but the
    /// keys belong to the live pane in focus.
    pub lost_pane_unfocused: bool,
    /// Right-click context menu, when open.
    pub context_menu: Option<ContextMenu>,
    /// Unscaled height reserved above the chrome by the tab strip, so
    /// the rail starts under the tabs instead of behind them.
    pub top_inset: f32,
    /// Whether the panel is expanded beside the rail.
    pub panel_visible: bool,
    /// Last known window width (logical), for settings hover/cursor geometry.
    pub last_window_width: f32,
    /// Sidebar chrome, header, pills and the workspace view.
    pub shell: crate::shell::Shell,
    /// State of the non-terminal views (Files, Snippets, Settings, …).
    pub screens: crate::screens::Screens,
    /// Machine id last scrolled into view by [`Self::reveal_machine`].
    pub revealed_machine: Option<String>,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            panel: HostPanel::default(),
            snippets: SnippetsPanel::with_defaults(),
            settings: SettingsModal::default(),
            form: AddHostForm::default(),
            snippet_form: crate::add_snippet::AddSnippetForm::default(),
            vault_unlock: VaultUnlockPrompt::default(),
            confirm: None,
            vault_configured: true,
            connection: None,
            lost: None,
            lost_pane: None,
            lost_pane_unfocused: false,
            context_menu: None,
            top_inset: 0.0,
            panel_visible: true,
            last_window_width: 1200.0,
            shell: crate::shell::Shell::default(),
            screens: crate::screens::Screens::default(),
            revealed_machine: None,
        }
    }
}

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

    fn press_shell(&mut self, hit: crate::shell::ShellHit) -> ChromeAction {
        use crate::shell::{SettingsPage, ShellHit, WorkspaceView};
        let view = |chrome: &mut Self, v: WorkspaceView| {
            chrome.show_view(v);
            ChromeAction::ViewChanged(v)
        };
        match hit {
            ShellHit::Brand => view(self, WorkspaceView::Home),
            ShellHit::CommandBar => ChromeAction::OpenPalette,
            ShellHit::AddServer => ChromeAction::AddHost,
            ShellHit::Settings => view(self, WorkspaceView::Settings(SettingsPage::Keys)),
            ShellHit::Tab(v) => view(self, v),
            ShellHit::Control(b) => ChromeAction::WindowControl(b),
            // Clicks inside the field being edited stay with it.
            ShellHit::Pill(tab) | ShellHit::PillClose(tab)
                if self.shell.is_renaming(tab) =>
            {
                ChromeAction::Consumed
            }
            ShellHit::Pill(tab) => ChromeAction::OpenSession(tab),
            ShellHit::PillClose(tab) => ChromeAction::CloseSession(tab),
            ShellHit::NewSession => ChromeAction::AddHostSession(
                self.shell
                    .machine
                    .as_ref()
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| "local".to_string()),
            ),
            ShellHit::SplitRight => ChromeAction::Split { down: false },
            ShellHit::SplitDown => ChromeAction::Split { down: true },
            ShellHit::Drag => ChromeAction::WindowDrag,
            ShellHit::Inert => ChromeAction::Consumed,
        }
    }

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

    fn confirm_action(action: ConfirmAction) -> ChromeAction {
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

    fn lost_action(route_id: usize, outcome: LostOutcome) -> ChromeAction {
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
    fn lost_hit(&self, x: f32, y: f32) -> Option<crate::geom::Rect> {
        self.lost_area().filter(|area| area.contains(x, y))
    }

    /// What a pointer at `(x, y)` hits on the connection progress, or
    /// `None` when there is none or the pointer is outside its area.
    fn connection_hit(&self, x: f32, y: f32) -> Option<ConnectionHit> {
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
        self.panel.filter = crate::text_field::TextDraft::new(filter);
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

    /// Drop every hover highlight — used when the pointer leaves the
    /// window, where no move event will arrive to clear it.
    pub fn clear_hover(&mut self) -> bool {
        let lost = self.lost.as_mut().is_some_and(|l| l.hover.take().is_some());
        self.panel.set_hover(None) || lost
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

    /// Right-click: open a context menu over a host or group row.
    ///
    /// Kept separate from [`Self::handle_press`] so a right-click never
    /// arms a host drag or opens a session.
    ///
    /// `sftp_open` adds "Open in other pane" on host rows when an SFTP
    /// session is already active.
    pub fn handle_context_press(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
        sftp_open: bool,
    ) -> ChromeAction {
        // Modals / overlays own the pointer; don't open under them.
        if self.settings.open
            || self.connection_hit(x, y).is_some()
            || self.lost_hit(x, y).is_some()
            || self.form.is_open()
            || self.snippet_form.is_open()
            || self.vault_unlock.is_open()
            || self.confirm.is_some()
        {
            self.close_context_menu();
            return ChromeAction::Ignored;
        }
        if !self.hosts_visible() {
            self.close_context_menu();
            return ChromeAction::Ignored;
        }

        // Session pills: rename / close.
        use crate::shell::ShellHit;
        if let Some(ShellHit::Pill(tab) | ShellHit::PillClose(tab)) =
            self.shell.hit_test(x, y)
        {
            let closable = self
                .shell
                .pills
                .iter()
                .find(|p| p.tab_index == tab)
                .is_some_and(|p| p.closable);
            self.context_menu = ContextMenu::for_session(x, y, tab, closable)
                .map(|m| m.clamped(window_width, window_height));
            return ChromeAction::Consumed;
        }

        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let menu = match self.panel.hit_test(origin_y, height, x, y) {
            Some(PanelHit::Item(index)) => self
                .panel
                .rows
                .get(index)
                .and_then(Row::host)
                .filter(|h| h.stored)
                .and_then(|h| {
                    ContextMenu::for_host_with_sftp(x, y, h.id.clone(), sftp_open)
                }),
            Some(PanelHit::Group(index)) => self
                .panel
                .rows
                .get(index)
                .and_then(|row| match row {
                    Row::Group { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .and_then(|id| ContextMenu::for_group(x, y, id)),
            _ => None,
        };

        match menu {
            Some(m) => {
                self.context_menu = Some(m.clamped(window_width, window_height));
                ChromeAction::Consumed
            }
            None => {
                self.close_context_menu();
                ChromeAction::Ignored
            }
        }
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    /// The confirmation a context-menu row raises, if it is destructive.
    fn delete_prompt_for(&self, index: usize) -> Option<ConfirmPrompt> {
        let action = self.context_menu.as_ref()?.take_action(index)?;
        match action {
            ContextAction::DeleteHost(id) => {
                let host = self
                    .panel
                    .rows
                    .iter()
                    .filter_map(Row::host)
                    .find(|h| h.id == id);
                let name = host.map_or("this server", |h| h.name.as_str());
                let sessions = host.map_or(0, |h| h.session_count);
                Some(ConfirmPrompt::delete_host(&id, name, sessions))
            }
            ContextAction::DeleteGroup(id) => {
                let group = self.panel.rows.iter().find_map(|r| match r {
                    Row::Group {
                        id: gid,
                        name,
                        host_count,
                        ..
                    } if *gid == id => Some((name.as_str(), *host_count)),
                    _ => None,
                });
                let (name, hosts) = group.unwrap_or(("this group", 0));
                Some(ConfirmPrompt::delete_group(&id, name, hosts))
            }
            _ => None,
        }
    }

    fn route_context_menu_press(&mut self, x: f32, y: f32) -> Option<ChromeAction> {
        let menu = self.context_menu.as_mut()?;
        match menu.hit_test(x, y) {
            ContextMenuHit::Dismiss => {
                self.close_context_menu();
                // Swallow the dismiss click so it does not open a host.
                Some(ChromeAction::Consumed)
            }
            ContextMenuHit::Consume => Some(ChromeAction::Consumed),
            ContextMenuHit::Item(index) => {
                // Deleting a host or group cannot be undone: ask first.
                if let Some(prompt) = self.delete_prompt_for(index) {
                    self.open_confirm(prompt);
                    return Some(ChromeAction::Consumed);
                }
                let Some(menu) = self.context_menu.as_mut() else {
                    return Some(ChromeAction::Consumed);
                };
                let action = menu.take_action(index);
                self.close_context_menu();
                Some(match action {
                    Some(ContextAction::NewSession(id)) => {
                        ChromeAction::AddHostSession(id)
                    }
                    Some(ContextAction::CopySshCommand(id)) => {
                        ChromeAction::CopySshCommand(id)
                    }
                    Some(ContextAction::DeleteHost(id)) => ChromeAction::DeleteHost(id),
                    Some(ContextAction::DeleteGroup(id)) => ChromeAction::DeleteGroup(id),
                    Some(ContextAction::EditHost(id)) => ChromeAction::EditHost(id),
                    Some(ContextAction::OpenSftp(id)) => ChromeAction::OpenSftp(id),
                    Some(ContextAction::OpenSftpOtherPane(id)) => {
                        ChromeAction::OpenSftpOtherPane(id)
                    }
                    Some(ContextAction::RenameHost(id)) => ChromeAction::RenameHost(id),
                    Some(ContextAction::RenameGroup(id)) => ChromeAction::RenameGroup(id),
                    Some(ContextAction::OpenGroup(id)) => ChromeAction::OpenGroup(id),
                    Some(ContextAction::CloseGroup(id)) => ChromeAction::CloseGroup(id),
                    Some(ContextAction::RenameSession(tab)) => {
                        self.shell.begin_rename(tab);
                        ChromeAction::Consumed
                    }
                    Some(ContextAction::CloseSession(tab)) => {
                        ChromeAction::CloseSession(tab)
                    }
                    Some(ContextAction::Copy) => ChromeAction::ContextCopy,
                    Some(ContextAction::Paste) => ChromeAction::ContextPaste,
                    Some(ContextAction::SftpNewFolder) => ChromeAction::SftpNewFolder,
                    Some(ContextAction::SftpRename) => ChromeAction::SftpRename,
                    Some(ContextAction::SftpDelete) => ChromeAction::SftpDelete,
                    Some(ContextAction::SftpTransfer) => ChromeAction::SftpTransfer,
                    Some(ContextAction::SftpEdit) => ChromeAction::SftpEdit,
                    Some(ContextAction::SftpOpen) => ChromeAction::SftpOpen,
                    Some(ContextAction::SftpRefresh) => ChromeAction::SftpRefresh,
                    None => ChromeAction::Consumed,
                })
            }
        }
    }

    fn route_press(
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

    /// Route a mouse move; returns whether anything needs repainting.
    pub fn handle_hover(&mut self, window_height: f32, x: f32, y: f32) -> bool {
        let window_width = { self.last_window_width };
        if let Some(prompt) = self.confirm.as_mut() {
            return prompt.hover_at((window_width, window_height), x, y);
        }
        if let Some(menu) = self.context_menu.as_mut() {
            return menu.hover_at(x, y);
        }
        if self.settings.open {
            return self
                .settings
                .handle_hover(window_width, window_height, x, y);
        }
        if self.connection_hit(x, y).is_some() {
            return false;
        }
        if let Some(area) = self.lost_area() {
            if let Some(lost) = self.lost.as_mut() {
                // Leaving the card clears its hover too.
                let changed = lost.hover_at(area, x, y);
                if area.contains(x, y) {
                    return changed;
                }
                if changed {
                    return true;
                }
            }
        }

        if self.snippet_form.is_open() {
            let layout = crate::dialog_form::DialogFormLayout::compute(
                &self.snippet_form.inner,
                window_width,
                window_height,
            );
            let next = match layout.hit_test(x, y) {
                Some(crate::dialog_form::DynamicFormHit::Save) => {
                    Some(crate::dialog_form::DynamicFormHit::Save)
                }
                Some(crate::dialog_form::DynamicFormHit::Cancel) => {
                    Some(crate::dialog_form::DynamicFormHit::Cancel)
                }
                _ => None,
            };
            if self.snippet_form.inner.btn_hover != next {
                self.snippet_form.inner.btn_hover = next;
                return true;
            }
            return false;
        }

        if self.form.is_open() {
            let layout = self.dialog_layout(window_width, window_height);
            let mut changed = false;
            if self.form.menu().is_some() {
                let hover = (0..self.form.menu_len()).find(|&i| {
                    layout
                        .menu_option_rect(&self.form, i)
                        .is_some_and(|opt| opt.contains(x, y))
                });
                changed |= self.form.set_menu_hover(hover);
            } else {
                changed |= self.form.set_menu_hover(None);
            }
            let target = match layout.hit_test(&self.form, x, y) {
                AddHostHit::Consume | AddHostHit::Field(_) => None,
                other => Some(other),
            };
            changed |= self.form.set_hover(target);
            return changed;
        }
        let origin_y = self.origin_y();
        let height = window_height - origin_y;
        let shell_changed = self.shell.set_hover(self.shell.hit_test(x, y));
        let hover = self.panel.hover_at(origin_y, height, x, y);
        self.panel.set_hover(hover) | shell_changed
    }

    /// Remember the last layout width so hover/cursor can rebuild dialog rects.
    pub fn set_window_size(&mut self, width: f32, height: f32) {
        self.last_window_width = width;
        self.shell.window = (width, height);
    }

    /// Cursor affordance under `(x, y)`.
    pub fn cursor_at(
        &self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ChromeCursor {
        if let Some(prompt) = self.confirm.as_ref() {
            use crate::components::overlay::DialogHit;
            return match prompt.layout((window_width, window_height)).hit_test(x, y) {
                DialogHit::Confirm | DialogHit::Cancel | DialogHit::Option => {
                    ChromeCursor::Pointer
                }
                DialogHit::Inside | DialogHit::Scrim => ChromeCursor::Default,
            };
        }
        if let Some(menu) = self.context_menu.as_ref() {
            return match menu.hit_test(x, y) {
                ContextMenuHit::Item(_) => ChromeCursor::Pointer,
                ContextMenuHit::Consume | ContextMenuHit::Dismiss => {
                    ChromeCursor::Default
                }
            };
        }
        if self.vault_unlock.is_open() {
            let layout = VaultUnlockLayout::for_prompt(
                window_width,
                window_height,
                &self.vault_unlock,
            );
            return match layout.hit_test_labels(x, y, self.vault_unlock.action_label()) {
                VaultUnlockHit::Field | VaultUnlockHit::ConfirmField => {
                    ChromeCursor::Text
                }
                VaultUnlockHit::ToggleVisible
                | VaultUnlockHit::ToggleRemember
                | VaultUnlockHit::Unlock
                | VaultUnlockHit::Cancel => ChromeCursor::Pointer,
                VaultUnlockHit::Consume => ChromeCursor::Default,
            };
        }
        if self.settings.open {
            return self.settings.cursor_at(window_width, window_height, x, y);
        }
        if let Some(hit) = self.connection_hit(x, y) {
            return match hit {
                ConnectionHit::Close | ConnectionHit::ToggleLogs => ChromeCursor::Pointer,
                ConnectionHit::Consume => ChromeCursor::Default,
            };
        }
        if let (Some(area), Some(lost)) = (self.lost_hit(x, y), self.lost.as_ref()) {
            return match lost.button_at(area, x, y) {
                Some(_) => ChromeCursor::Pointer,
                None => ChromeCursor::Default,
            };
        }

        if self.snippet_form.is_open() {
            let layout = crate::dialog_form::DialogFormLayout::compute(
                &self.snippet_form.inner,
                window_width,
                window_height,
            );
            return match layout.hit_test(x, y) {
                Some(crate::dialog_form::DynamicFormHit::Field(_)) => ChromeCursor::Text,
                Some(crate::dialog_form::DynamicFormHit::Save)
                | Some(crate::dialog_form::DynamicFormHit::Cancel) => {
                    ChromeCursor::Pointer
                }
                _ => ChromeCursor::Default,
            };
        }

        if self.form.is_open() {
            let layout = self.dialog_layout(window_width, window_height);
            let dialog = layout.rect(self.form.height());
            let on_menu = layout
                .menu_rect(&self.form)
                .is_some_and(|m| m.contains(x, y));
            if !dialog.contains(x, y) && !on_menu {
                return ChromeCursor::Pointer; // scrim dismiss
            }
            return match layout.hit_test(&self.form, x, y) {
                AddHostHit::Field(_) => ChromeCursor::Text,
                AddHostHit::StepPill(_)
                | AddHostHit::Back
                | AddHostHit::CopyError
                | AddHostHit::Next
                | AddHostHit::SelectAuth(_)
                | AddHostHit::ToggleIdentityMenu
                | AddHostHit::SelectIdentity(_)
                | AddHostHit::ToggleGroupMenu
                | AddHostHit::SelectGroup(_)
                | AddHostHit::TogglePasswordVisible
                | AddHostHit::GenerateKey
                | AddHostHit::Close
                | AddHostHit::Connect
                | AddHostHit::Cancel => ChromeCursor::Pointer,
                AddHostHit::Consume => ChromeCursor::Default,
            };
        }
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        if let Some(hit) = self.shell.hit_test(x, y) {
            return if hit.is_control() {
                ChromeCursor::Pointer
            } else {
                ChromeCursor::Default
            };
        }
        if self.shell.view_owns(x, y) {
            let view = self.shell.view();
            let content = self.shell.content_rect();
            return if self.screens.is_clickable(view, content, x, y) {
                ChromeCursor::Pointer
            } else {
                ChromeCursor::Default
            };
        }
        if self.hosts_visible() {
            return match self.panel.hit_test(origin_y, height, x, y) {
                Some(PanelHit::Search) | Some(PanelHit::NewGroupField) => {
                    ChromeCursor::Text
                }
                Some(PanelHit::Background) | None => ChromeCursor::Default,
                Some(_) => ChromeCursor::Pointer,
            };
        }
        ChromeCursor::Default
    }

    /// Update an armed host drag while the primary button is held.
    ///
    /// Returns whether the chrome needs a repaint.
    pub fn handle_drag_move(&mut self, window_height: f32, x: f32, y: f32) -> bool {
        let Some(drag) = self.panel.host_drag.as_mut() else {
            return false;
        };
        // Ignore pointer while the ghost is snapping home.
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Snapping { .. }) {
            return true;
        }
        drag.current_x = x;
        drag.current_y = y;
        let dx = x - drag.press_x;
        let dy = y - drag.press_y;
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Armed)
            && dx * dx + dy * dy >= crate::sidebar::HOST_DRAG_THRESHOLD.powi(2)
        {
            drag.phase = crate::sidebar::HostDragPhase::Dragging;
        }
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Dragging) {
            let w = drag.source_rect.width;
            let h = drag.source_rect.height;
            drag.ghost_rect =
                crate::geom::Rect::new(x - drag.grab_dx, y - drag.grab_dy, w, h);
        }
        let dragging = matches!(drag.phase, crate::sidebar::HostDragPhase::Dragging);
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let target = if dragging {
            self.panel.drop_target_at(origin_y, height, x, y)
        } else {
            None
        };
        if let Some(drag) = self.panel.host_drag.as_mut() {
            drag.drop_target = target;
        }
        let _ = self.handle_hover(window_height, x, y);
        true
    }

    /// Finish a host press/drag on primary-button release.
    pub fn handle_release(&mut self, window_height: f32, x: f32, y: f32) -> ChromeAction {
        let Some(drag) = self.panel.host_drag.as_ref() else {
            return ChromeAction::Ignored;
        };
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Armed) {
            let id = drag.host_id.clone();
            let is_group = drag.is_group();
            self.panel.host_drag = None;
            return if is_group {
                ChromeAction::ToggleGroup(id)
            } else {
                ChromeAction::OpenHost(id)
            };
        }
        // Dragging → apply drop immediately (no snap tween).
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let cached = self
            .panel
            .host_drag
            .as_ref()
            .and_then(|d| d.drop_target.clone());
        let host_id = self
            .panel
            .host_drag
            .as_ref()
            .map(|d| d.host_id.clone())
            .unwrap_or_default();
        let is_group = self
            .panel
            .host_drag
            .as_ref()
            .is_some_and(crate::sidebar::HostDrag::is_group);
        let target = self.panel.drop_target_at(origin_y, height, x, y).or(cached);
        let Some(target) = target else {
            self.panel.host_drag = None;
            return ChromeAction::Consumed;
        };
        if let crate::sidebar::HostDropTarget::Group(ref group_id) = target {
            self.panel.collapsed_groups.remove(group_id);
        }
        self.panel.host_drag = None;
        Self::action_from_drop(host_id, is_group, target)
    }

    fn action_from_drop(
        host_id: String,
        is_group: bool,
        pending: crate::sidebar::HostDropTarget,
    ) -> ChromeAction {
        match pending {
            crate::sidebar::HostDropTarget::Group(group_id) => {
                ChromeAction::SetHostGroup {
                    host_id,
                    group_id: Some(group_id),
                }
            }
            crate::sidebar::HostDropTarget::Ungroup => {
                if is_group {
                    ChromeAction::ReorderGroup {
                        group_id: host_id,
                        before_group_id: None,
                        before_host_id: None,
                    }
                } else {
                    ChromeAction::ReorderHost {
                        host_id,
                        before_host_id: None,
                        before_group_id: None,
                    }
                }
            }
            crate::sidebar::HostDropTarget::BeforeHost(before_host_id) => {
                if is_group {
                    ChromeAction::ReorderGroup {
                        group_id: host_id,
                        before_group_id: None,
                        before_host_id: Some(before_host_id),
                    }
                } else {
                    ChromeAction::ReorderHost {
                        host_id,
                        before_host_id: Some(before_host_id),
                        before_group_id: None,
                    }
                }
            }
            crate::sidebar::HostDropTarget::BeforeGroup(before_group_id) => {
                if is_group {
                    ChromeAction::ReorderGroup {
                        group_id: host_id,
                        before_group_id: Some(before_group_id),
                        before_host_id: None,
                    }
                } else {
                    ChromeAction::ReorderHost {
                        host_id,
                        before_host_id: None,
                        before_group_id: Some(before_group_id),
                    }
                }
            }
        }
    }

    /// Per-frame drag tick: while a host is dragged near the top or
    /// bottom edge of the machine list, scroll the list and retarget the
    /// drop under the (still) pointer. Never produces an action.
    pub fn tick_host_drag(&mut self, dt: f32) -> Option<ChromeAction> {
        let drag = self.panel.host_drag.as_ref()?;
        if !matches!(drag.phase, crate::sidebar::HostDragPhase::Dragging) {
            return None;
        }
        let (x, y) = (drag.current_x, drag.current_y);
        // The frontend keeps the window size current every frame.
        let window_height = self.shell.window.1;
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let speed = self.panel.drag_autoscroll_speed(origin_y, height, y);
        if speed == 0.0 {
            return None;
        }
        let before = self.panel.scroll;
        self.panel.scroll_by(speed * dt, origin_y, height);
        if self.panel.scroll != before {
            let target = self.panel.drop_target_at(origin_y, height, x, y);
            if let Some(drag) = self.panel.host_drag.as_mut() {
                drag.drop_target = target;
            }
        }
        None
    }

    /// Whether the chrome needs continuous frames.
    pub fn needs_animation_frames(&self) -> bool {
        self.connection.is_some()
            || self
                .panel
                .host_drag
                .as_ref()
                .is_some_and(|d| d.started() && !d.is_snapping())
    }

    /// Scroll the selected machine's row into view when the selection
    /// changed (palette, tab switch, a new session). Only once per
    /// change: the user may then scroll the list away freely. An id
    /// without a row (yet) is retried on the next call.
    pub fn reveal_machine(&mut self, id: &str, window_height: f32) -> bool {
        if self.revealed_machine.as_deref() == Some(id) {
            return false;
        }
        let Some(index) = self.panel.row_of_host(id) else {
            return false;
        };
        if !self.panel.visible_row_indices().contains(&index) {
            return false;
        }
        self.revealed_machine = Some(id.to_string());
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        self.panel.reveal_row(index, origin_y, height)
    }

    /// Route a pixel wheel delta (touchpads; positive = content moves
    /// down, i.e. scroll toward the top) over the panel.
    pub fn handle_wheel_pixels(
        &mut self,
        window_height: f32,
        x: f32,
        y: f32,
        dy: f32,
    ) -> bool {
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        if !self.panel.rect(origin_y, height).contains(x, y) {
            return false;
        }
        let before = self.panel.scroll;
        self.panel.scroll_by(-dy, origin_y, height);
        self.panel.scroll != before || self.panel.content_height() == 0.0
    }
    /// Route a wheel notch over the panel; returns whether it was consumed.
    pub fn handle_wheel(
        &mut self,
        window_height: f32,
        x: f32,
        y: f32,
        lines: f32,
    ) -> bool {
        if !self.hosts_visible() {
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

    /// Route a keyboard input to the snippet editor. `None` when it is closed.
    pub fn handle_snippet_form_input(
        &mut self,
        input: crate::add_snippet::FormInput,
        text: &str,
    ) -> Option<crate::add_snippet::FormOutcome> {
        if !self.snippet_form.is_open() {
            return None;
        }
        let outcome = self.snippet_form.handle_input(input, text);
        if outcome == crate::add_snippet::FormOutcome::Cancel {
            self.snippet_form.inner.closing = true;
        }
        Some(outcome)
    }

    /// Shared text edit (Delete, caret, selection…) for the unlock prompt.
    pub fn edit_vault_unlock(&mut self, edit: crate::text_field::TextEdit) -> bool {
        if !self.vault_unlock.is_open() {
            return false;
        }
        self.vault_unlock.edit(edit);
        true
    }

    pub fn handle_vault_unlock_input(
        &mut self,
        input: FormInput,
        text: &str,
    ) -> Option<bool> {
        if !self.vault_unlock.is_open() {
            return None;
        }
        match input {
            FormInput::Text => {
                self.vault_unlock.insert(text);
                Some(false)
            }
            FormInput::Backspace => {
                self.vault_unlock.backspace();
                Some(false)
            }
            FormInput::Enter => Some(true),
            FormInput::Escape => {
                self.vault_unlock.close();
                Some(false)
            }
            FormInput::Next | FormInput::Previous => {
                self.vault_unlock.toggle_field();
                Some(false)
            }
            _ => Some(false),
        }
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
            self.form.anchor_height(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::add_host::Field;
    use crate::settings::SettingsTab;
    use crate::sidebar::Badge;

    #[test]
    fn a_key_row_copies_its_public_key() {
        let mut chrome = Chrome::default();
        chrome.open_settings(SettingsTab::Keys);
        chrome.settings.set_keys(vec![crate::settings::SshKeyItem {
            id: "k1".into(),
            name: "laptop".into(),
            fingerprint: "SHA256:abc".into(),
            created: "2026-09-28".into(),
            public_key: "ssh-ed25519 AAAA laptop".into(),
        }]);
        let copy = chrome.settings.key_copy_rect(1200.0, 800.0, 0);
        let delete = chrome.settings.key_delete_rect(1200.0, 800.0, 0);
        assert!(copy.right() <= delete.x, "beside delete, not over it");
        let action = chrome.handle_press(1200.0, 800.0, copy.x + 4.0, copy.y + 4.0);
        assert_eq!(
            action,
            ChromeAction::CopyPublicKey("ssh-ed25519 AAAA laptop".into())
        );
        let notice = chrome.settings.keys_notice.clone().expect("confirmation");
        assert!(notice.contains("authorized_keys"), "{notice}");
    }

    fn centre(r: &crate::geom::Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    #[test]
    fn the_reserved_width_is_the_sidebar() {
        let chrome = chrome_with_hosts(1);
        assert_eq!(chrome.reserved_width(), crate::shell::grid_insets().left);
        assert!(chrome.reserved_width() > crate::shell::layout::SIDEBAR_WIDTH);
        assert_eq!(chrome.origin_y(), 0.0);
    }

    #[test]
    fn settings_button_opens_the_settings_page_and_its_dialog() {
        let mut chrome = chrome_with_hosts(1);
        let (x, y) = centre(&crate::shell::sidebar::settings_rect(800.0));
        let action = chrome.handle_press(1200.0, 800.0, x, y);
        let page =
            crate::shell::WorkspaceView::Settings(crate::shell::SettingsPage::Keys);
        assert_eq!(action, ChromeAction::ViewChanged(page));
        assert_eq!(chrome.shell.view(), page);
        // The J5 Settings page replaces the legacy dialog.
        assert!(!chrome.settings_is_open());
    }

    #[test]
    fn settings_pages_never_open_the_legacy_dialog_and_leaving_closes_it() {
        use crate::shell::{SettingsPage, WorkspaceView};
        let mut chrome = chrome_with_hosts(1);
        for page in SettingsPage::ALL {
            chrome.show_view(WorkspaceView::Settings(page));
            assert!(!chrome.settings_is_open(), "{page:?}");
        }
        // Opened some other way (palette): leaving Settings closes it.
        chrome.open_settings(SettingsTab::Keys);
        chrome.show_view(WorkspaceView::Terminal);
        assert!(!chrome.settings_is_open());
    }

    #[test]
    fn the_command_bar_opens_the_palette_and_brand_goes_home() {
        let mut chrome = chrome_with_hosts(1);
        let (x, y) = centre(&crate::shell::sidebar::command_bar_rect());
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::OpenPalette
        );
        let (x, y) = centre(&crate::shell::sidebar::brand_rect());
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::ViewChanged(crate::shell::WorkspaceView::Home)
        );
        assert_eq!(chrome.shell.view(), crate::shell::WorkspaceView::Home);
    }

    #[test]
    fn header_tabs_switch_views_and_cover_the_terminal() {
        use crate::shell::WorkspaceView;
        let mut chrome = chrome_with_hosts(1);
        let tab = chrome.shell.header_geom().tabs[3];
        let (x, y) = centre(&tab);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::ViewChanged(WorkspaceView::Snippets)
        );
        // The content is now the view's, not the terminal's.
        assert!(chrome.shell.view_owns(700.0, 400.0));
        // Esc on a view that ignores it returns to the terminal.
        let out = chrome.view_input(&crate::screens::ViewInput::Key {
            key: crate::screens::ViewKey::Escape,
            mods: Default::default(),
        });
        assert_eq!(out, crate::screens::ViewOutcome::Redraw);
        assert_eq!(chrome.shell.view(), WorkspaceView::Terminal);
    }

    #[test]
    fn pills_focus_close_and_add_sessions_of_the_selected_machine() {
        let mut chrome = chrome_with_hosts(1);
        chrome.shell.machine = Some(crate::shell::MachineInfo {
            id: "id-0".into(),
            name: "host-0".into(),
            address: "root@host-0".into(),
        });
        chrome.shell.pills = vec![crate::shell::SessionPill {
            tab_index: 7,
            label: "shell".into(),
            active: false,
            new_output: false,
            closable: true,
        }];
        let g = chrome.shell.pills_geom().unwrap();
        let (x, y) = centre(&g.pills[0]);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::OpenSession(7)
        );
        let (x, y) = centre(&g.plus);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::AddHostSession("id-0".into())
        );
        let (x, y) = centre(&g.split_right);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::Split { down: false }
        );
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, 900.0, 30.0),
            ChromeAction::WindowDrag
        );
    }

    fn chrome_with_pills() -> Chrome {
        let mut chrome = chrome_with_hosts(1);
        chrome.shell.machine = Some(crate::shell::MachineInfo {
            id: "id-0".into(),
            name: "host-0".into(),
            address: "root@host-0".into(),
        });
        chrome.shell.pills = vec![
            crate::shell::SessionPill {
                tab_index: 0,
                label: "home".into(),
                active: false,
                new_output: false,
                closable: false,
            },
            crate::shell::SessionPill {
                tab_index: 7,
                label: "~".into(),
                active: true,
                new_output: false,
                closable: true,
            },
        ];
        chrome
    }

    #[test]
    fn right_clicking_a_pill_offers_rename_and_close() {
        let mut chrome = chrome_with_pills();
        let g = chrome.shell.pills_geom().unwrap();
        let (x, y) = centre(&g.pills[1]);
        assert_eq!(
            chrome.handle_context_press(1200.0, 800.0, x, y, false),
            ChromeAction::Consumed
        );
        let menu = chrome.context_menu.clone().expect("session menu");
        let labels: Vec<&str> = menu.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Rename", "Close"]);

        // "Rename" starts the inline draft on that pill.
        let (x, y) = centre(&menu.item_rect(0).unwrap());
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::Consumed
        );
        assert!(chrome.context_menu.is_none());
        assert!(chrome.shell.is_renaming(7));

        // "Close" closes the session like its ×.
        let g = chrome.shell.pills_geom().unwrap();
        let (x, y) = centre(&g.pills[1]);
        chrome.handle_context_press(1200.0, 800.0, x, y, false);
        let menu = chrome.context_menu.clone().unwrap();
        let (x, y) = centre(&menu.item_rect(1).unwrap());
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::CloseSession(7)
        );
    }

    #[test]
    fn a_pinned_pill_can_be_renamed_but_not_closed() {
        let mut chrome = chrome_with_pills();
        let g = chrome.shell.pills_geom().unwrap();
        let (x, y) = centre(&g.pills[0]);
        chrome.handle_context_press(1200.0, 800.0, x, y, false);
        let labels: Vec<String> = chrome
            .context_menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.label.clone())
            .collect();
        assert_eq!(labels, ["Rename"]);
    }

    #[test]
    fn pressing_the_pill_being_renamed_keeps_editing() {
        let mut chrome = chrome_with_pills();
        chrome.shell.begin_rename(7);
        let g = chrome.shell.pills_geom().unwrap();
        let (x, y) = centre(&g.pills[1]);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::Consumed
        );
        assert!(chrome.shell.is_renaming(7));
        // The field covers the pill: its × does not close mid-edit.
        let c = crate::components::navigation::session_pill::close_rect(&g.pills[1]);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, c.x + 2.0, c.y + 2.0),
            ChromeAction::Consumed
        );
        // Another pill still focuses its session.
        let (x, y) = centre(&g.pills[0]);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::OpenSession(0)
        );
    }

    #[test]
    fn hovering_shell_controls_repaints_and_sets_the_cursor() {
        let mut chrome = chrome_with_hosts(1);
        let (x, y) = centre(&crate::shell::sidebar::add_server_rect(800.0));
        assert!(chrome.handle_hover(800.0, x, y));
        assert_eq!(chrome.shell.hover, Some(crate::shell::ShellHit::AddServer));
        assert_eq!(chrome.cursor_at(1200.0, 800.0, x, y), ChromeCursor::Pointer);
        assert_eq!(
            chrome.cursor_at(1200.0, 800.0, 900.0, 30.0),
            ChromeCursor::Default
        );
    }

    fn chrome_with_hosts(n: usize) -> Chrome {
        let mut chrome = Chrome::default();
        let mut rows = vec![Row::Section("Hosts".to_string())];
        rows.extend((0..n).map(|i| {
            Row::Host(HostItem {
                id: format!("id-{i}"),
                name: format!("host-{i}"),
                endpoint: format!("root@host-{i}"),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: crate::os_icons::HostStatus::Idle,
                nested: false,
                session_count: 0,
            })
        }));
        chrome.set_rows(rows);
        chrome
    }

    #[test]
    fn right_click_host_opens_delete_menu_and_selects() {
        let mut chrome = chrome_with_hosts(2);
        let row = chrome.panel.item_rect(0.0, 1);
        let open =
            chrome.handle_context_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0, false);
        assert_eq!(open, ChromeAction::Consumed);
        let menu = chrome.context_menu.as_ref().expect("menu open");
        let delete = menu
            .items
            .iter()
            .position(|i| {
                matches!(i.action, crate::context_menu::ContextAction::DeleteHost(_))
            })
            .expect("delete item");
        let item = menu.item_rect(delete).unwrap();
        // The destructive row opens a confirmation instead of deleting.
        let action = chrome.handle_press(1200.0, 800.0, item.x + 4.0, item.y + 4.0);
        assert_eq!(action, ChromeAction::Consumed);
        assert!(chrome.context_menu.is_none(), "menu closes");
        let prompt = chrome.confirm.as_ref().expect("confirm dialog open");
        assert_eq!(prompt.spec.title, "Delete host-0?");
        assert_eq!(
            chrome.top_modal_paint(),
            Some(ModalPaintLayer::Confirm),
            "the dialog paints above everything"
        );
        // Enter on the default (Cancel) focus keeps the host.
        assert_eq!(
            chrome.handle_confirm_key(DialogKey::Enter),
            Some(ChromeAction::Consumed)
        );
        assert!(chrome.confirm.is_none());
    }

    #[test]
    fn confirming_the_delete_dialog_emits_the_delete_action() {
        let mut chrome = chrome_with_hosts(2);
        chrome.open_confirm(ConfirmPrompt::delete_host("id-0", "host-0", 0));
        let confirm = chrome
            .confirm
            .as_ref()
            .unwrap()
            .layout((1200.0, 800.0))
            .dialog
            .confirm;
        let action = chrome.handle_press(1200.0, 800.0, confirm.x + 4.0, confirm.y + 4.0);
        assert_eq!(action, ChromeAction::DeleteHost("id-0".to_string()));
        assert!(chrome.confirm.is_none());

        chrome.open_confirm(ConfirmPrompt::delete_group("g", "prod", 1));
        assert_eq!(
            chrome.handle_confirm_key(DialogKey::Tab),
            Some(ChromeAction::Consumed)
        );
        assert_eq!(
            chrome.handle_confirm_key(DialogKey::Enter),
            Some(ChromeAction::DeleteGroup("g".to_string()))
        );
    }

    #[test]
    fn sftp_delete_runs_once_on_confirm_and_never_on_cancel() {
        let mut chrome = chrome_with_hosts(1);
        chrome.open_confirm(ConfirmPrompt::sftp_delete("logs", true));
        let prompt = chrome.confirm.as_ref().unwrap();
        assert_eq!(prompt.spec.title, "Delete logs?");
        assert!(prompt.spec.body.contains("everything inside"));
        assert_eq!(
            chrome.handle_confirm_key(DialogKey::Enter),
            Some(ChromeAction::Consumed),
            "the default focus is Cancel"
        );
        assert!(chrome.confirm.is_none());

        chrome.open_confirm(ConfirmPrompt::sftp_delete("a.txt", false));
        assert_eq!(
            chrome.handle_confirm_key(DialogKey::Escape),
            Some(ChromeAction::Consumed)
        );
        assert!(chrome.confirm.is_none());

        chrome.open_confirm(ConfirmPrompt::sftp_delete("a.txt", false));
        chrome.handle_confirm_key(DialogKey::Tab);
        assert_eq!(
            chrome.handle_confirm_key(DialogKey::Enter),
            Some(ChromeAction::SftpDeleteConfirmed)
        );
        assert_eq!(chrome.handle_confirm_key(DialogKey::Enter), None);
    }

    #[test]
    fn the_scrim_and_escape_dismiss_the_delete_dialog() {
        let mut chrome = chrome_with_hosts(1);
        chrome.open_confirm(ConfirmPrompt::delete_host("id-0", "host-0", 0));
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, 3.0, 3.0),
            ChromeAction::Consumed
        );
        assert!(chrome.confirm.is_none());
        chrome.open_confirm(ConfirmPrompt::delete_host("id-0", "host-0", 0));
        assert_eq!(
            chrome.handle_confirm_key(DialogKey::Escape),
            Some(ChromeAction::Consumed)
        );
        assert!(chrome.confirm.is_none());
        assert_eq!(chrome.handle_confirm_key(DialogKey::Escape), None);
    }

    #[test]
    fn deleting_a_host_with_open_sessions_says_so() {
        let mut chrome = chrome_with_hosts(1);
        if let Some(Row::Host(h)) = chrome
            .panel
            .rows
            .iter_mut()
            .find(|r| matches!(r, Row::Host(_)))
        {
            h.session_count = 2;
        }
        let row = chrome.panel.item_rect(0.0, 1);
        chrome.handle_context_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0, false);
        let menu = chrome.context_menu.as_ref().unwrap();
        let delete = menu
            .items
            .iter()
            .position(|i| {
                matches!(i.action, crate::context_menu::ContextAction::DeleteHost(_))
            })
            .unwrap();
        let item = menu.item_rect(delete).unwrap();
        chrome.handle_press(1200.0, 800.0, item.x + 4.0, item.y + 4.0);
        assert!(chrome
            .confirm
            .as_ref()
            .unwrap()
            .spec
            .body
            .contains("2 open sessions"));
    }

    #[test]
    fn connection_progress_fills_the_content_and_leaves_the_sidebar_live() {
        let mut chrome = chrome_with_hosts(3);
        chrome.set_window_size(1200.0, 800.0);
        chrome.connection = Some(ConnectionSequence::start_ssh(
            "id-0",
            "host-0",
            "SSH root@host-0",
        ));
        // Painted by the shell in the Terminal content, not as a window modal.
        assert!(chrome.modal_paint_stack().is_empty());
        let content = chrome.connection_area();
        assert_eq!(content, chrome.shell.content_rect());
        let conn = chrome.connection.clone().unwrap();
        let dialog = conn.dialog_rect_in(content);
        assert!(content.contains(dialog.x, dialog.y));
        assert!(content.contains(dialog.right() - 1.0, dialog.bottom() - 1.0));

        // Cancel, inside the content, dismisses; the rest of it swallows.
        let (cx, cy) = centre(&conn.close_button_rect(dialog));
        assert_eq!(
            chrome.cursor_at(1200.0, 800.0, cx, cy),
            ChromeCursor::Pointer
        );
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, content.x + 4.0, content.bottom() - 4.0),
            ChromeAction::Consumed
        );
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, cx, cy),
            ChromeAction::DismissConnection
        );

        // The sidebar still answers while connecting.
        let row = chrome.panel.item_rect(0.0, 2);
        assert_eq!(
            chrome.cursor_at(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
            ChromeCursor::Pointer
        );
        chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
        assert_eq!(
            chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::OpenHost("id-1".to_string())
        );

        // Only the Terminal view shows it: another view's content is live.
        chrome.show_view(crate::shell::WorkspaceView::Home);
        let home = chrome.connection_area();
        let (hx, hy) = (home.x + home.width / 2.0, home.y + 40.0);
        let conn = chrome.connection.take();
        let without = chrome.handle_press(1200.0, 800.0, hx, hy);
        chrome.connection = conn;
        assert_eq!(chrome.handle_press(1200.0, 800.0, hx, hy), without);
    }

    #[test]
    fn lost_connection_card_reconnects_or_closes_its_tab() {
        let mut chrome = chrome_with_hosts(3);
        chrome.set_window_size(1200.0, 800.0);
        chrome.lost = Some(LostSession::new(9, "id-0", "host-0", &[]));
        // Painted in the Terminal content like the connection progress.
        assert!(chrome.modal_paint_stack().is_empty());
        let area = chrome.lost_area().expect("card shown on the Terminal view");
        assert_eq!(area, chrome.shell.content_rect());
        let layout = chrome.lost.as_ref().unwrap().layout_in(area).dialog;

        let (rx, ry) = centre(&layout.confirm);
        assert_eq!(
            chrome.cursor_at(1200.0, 800.0, rx, ry),
            ChromeCursor::Pointer
        );
        assert!(chrome.handle_hover(800.0, rx, ry));
        assert_eq!(
            chrome.lost.as_ref().unwrap().hover,
            Some(crate::components::overlay::DialogFocus::Confirm)
        );
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, rx, ry),
            ChromeAction::ReconnectSession(9)
        );
        let (cx, cy) = centre(&layout.cancel);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, cx, cy),
            ChromeAction::CloseLostSession(9)
        );
        // The dead terminal around the card swallows the press.
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, area.x + 4.0, area.bottom() - 4.0),
            ChromeAction::Consumed
        );
        assert_eq!(
            chrome.handle_context_press(1200.0, 800.0, area.x + 4.0, area.y + 4.0, false),
            ChromeAction::Ignored
        );
        assert!(chrome.context_menu.is_none());

        // The sidebar stays live.
        let row = chrome.panel.item_rect(0.0, 2);
        chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
        assert_eq!(
            chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::OpenHost("id-1".to_string())
        );
    }

    #[test]
    fn lost_connection_card_keys_reconnect_and_tab_to_close() {
        let mut chrome = Chrome::default();
        assert_eq!(chrome.handle_lost_key(DialogKey::Enter), None);
        chrome.lost = Some(LostSession::new(4, "h", "h", &[]));
        assert_eq!(
            chrome.handle_lost_key(DialogKey::Escape),
            Some(ChromeAction::Consumed)
        );
        assert_eq!(
            chrome.handle_lost_key(DialogKey::Enter),
            Some(ChromeAction::ReconnectSession(4))
        );
        assert_eq!(
            chrome.handle_lost_key(DialogKey::Tab),
            Some(ChromeAction::Consumed)
        );
        assert_eq!(
            chrome.handle_lost_key(DialogKey::Enter),
            Some(ChromeAction::CloseLostSession(4))
        );
    }

    #[test]
    fn split_pane_card_covers_only_its_pane() {
        let mut chrome = chrome_with_hosts(1);
        chrome.set_window_size(1200.0, 800.0);
        let content = chrome.shell.content_rect();
        let pane = crate::geom::Rect {
            x: content.x,
            y: content.y,
            width: content.width / 2.0,
            height: content.height,
        };
        chrome.lost = Some(LostSession::new(5, "id-0", "host-0", &[]));
        let whole_tab_area = chrome.lost_area().unwrap();
        assert_eq!(whole_tab_area, content);

        chrome.lost_pane = Some(pane);
        assert_eq!(chrome.lost_area(), Some(pane));

        // A click in the live half is not swallowed by the card.
        let (lx, ly) = (content.x + content.width * 0.75, content.y + 40.0);
        let with_card = chrome.handle_press(1200.0, 800.0, lx, ly);
        chrome.lost = None;
        let without_card = chrome.handle_press(1200.0, 800.0, lx, ly);
        assert_eq!(with_card, without_card);
        chrome.lost = Some(LostSession::new(5, "id-0", "host-0", &[]));

        // The card's buttons are laid out inside the pane.
        let layout = chrome.lost.as_ref().unwrap().layout_in(pane).dialog;
        assert!(layout.dialog.right() <= pane.right());
        let (rx, ry) = centre(&layout.confirm);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, rx, ry),
            ChromeAction::ReconnectSession(5)
        );
    }

    #[test]
    fn keys_skip_the_card_of_an_unfocused_dead_pane() {
        let mut chrome = chrome_with_hosts(1);
        chrome.set_window_size(1200.0, 800.0);
        chrome.lost = Some(LostSession::new(5, "id-0", "host-0", &[]));
        chrome.lost_pane = Some(chrome.shell.content_rect());
        chrome.lost_pane_unfocused = true;
        assert_eq!(chrome.handle_lost_key(DialogKey::Enter), None);
        assert!(!chrome.lost_takes_keys());
        chrome.lost_pane_unfocused = false;
        assert!(chrome.lost_takes_keys());
        assert_eq!(
            chrome.handle_lost_key(DialogKey::Enter),
            Some(ChromeAction::ReconnectSession(5))
        );
    }

    #[test]
    fn lost_connection_card_hides_behind_other_views_and_connecting() {
        let mut chrome = chrome_with_hosts(1);
        chrome.lost = Some(LostSession::new(1, "id-0", "host-0", &[]));
        assert!(chrome.lost_area().is_some());
        chrome.connection = Some(ConnectionSequence::start_ssh("id-0", "host-0", "SSH"));
        assert!(chrome.lost_area().is_none());
        chrome.connection = None;
        // A dialog over the terminal (add snippet, add host) owns the
        // pointer and keys, not the card under it.
        chrome.snippet_form.inner.closing = false;
        assert!(chrome.lost_area().is_none());
        chrome.snippet_form.inner.closing = true;
        chrome.open_add_host();
        assert!(chrome.lost_area().is_none());
        chrome.form.close();
        assert!(chrome.lost_area().is_some());
        chrome.show_view(crate::shell::WorkspaceView::Home);
        assert!(chrome.lost_area().is_none());
        assert_eq!(chrome.handle_lost_key(DialogKey::Enter), None);
    }

    #[test]
    fn pressing_a_host_row_arms_drag_and_opens_on_release() {
        let mut chrome = chrome_with_hosts(3);
        let row = chrome.panel.item_rect(0.0, 2);
        let action = chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
        assert_eq!(action, ChromeAction::Consumed);
        assert_eq!(chrome.panel.selected, Some(2));
        assert!(chrome.panel.host_drag.is_some());
        let release = chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0);
        assert_eq!(release, ChromeAction::OpenHost("id-1".to_string()));
        assert!(chrome.panel.host_drag.is_none());
    }

    #[test]
    fn group_menu_open_and_close_all_reach_the_app() {
        let group_row = Row::Group {
            id: "g1".to_string(),
            name: "jeremy".to_string(),
            host_count: 2,
            session_count: 0,
            collapsed: false,
        };
        for (item, expected) in [
            (0, ChromeAction::OpenGroup("g1".to_string())),
            (1, ChromeAction::CloseGroup("g1".to_string())),
        ] {
            let mut chrome = Chrome::default();
            chrome.set_rows(vec![Row::Section("Hosts".to_string()), group_row.clone()]);
            let card = chrome.panel.card_rect(0.0, 1);
            chrome.handle_context_press(
                1200.0,
                800.0,
                card.x + 20.0,
                card.y + card.height / 2.0,
                false,
            );
            let rect = chrome
                .context_menu
                .as_ref()
                .expect("group menu open")
                .item_rect(item)
                .unwrap();
            let action = chrome.handle_press(1200.0, 800.0, rect.x + 4.0, rect.y + 4.0);
            assert_eq!(action, expected);
        }
    }

    #[test]
    fn dragging_a_host_onto_a_group_emits_set_host_group() {
        let mut chrome = Chrome::default();
        chrome.set_rows(vec![
            Row::Section("Hosts".to_string()),
            Row::Group {
                id: "g1".to_string(),
                name: "jeremy".to_string(),
                host_count: 0,
                session_count: 0,
                collapsed: false,
            },
            Row::Host(HostItem {
                id: "solo".to_string(),
                name: "solo".to_string(),
                endpoint: "root@solo".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: crate::os_icons::HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
        ]);
        let host = chrome.panel.card_rect(0.0, 2);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, host.x + 20.0, host.y + 20.0),
            ChromeAction::Consumed
        );
        let group = chrome.panel.card_rect(0.0, 1);
        let gy = group.y + group.height / 2.0;
        assert!(chrome.handle_drag_move(800.0, group.x + 20.0, gy));
        assert!(chrome.panel.host_drag.as_ref().is_some_and(|d| d.started()));
        let action = chrome.handle_release(800.0, group.x + 20.0, gy);
        assert_eq!(
            action,
            ChromeAction::SetHostGroup {
                host_id: "solo".to_string(),
                group_id: Some("g1".to_string()),
            }
        );
        assert!(chrome.panel.host_drag.is_none());
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
        // Including the sidebar list's own background, below its rows.
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, 130.0, 600.0),
            ChromeAction::Consumed
        );
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
        let input = layout.input_rect(&chrome.form, Field::Hostname).unwrap();

        assert_eq!(
            chrome.handle_press(1200.0, 800.0, input.x + 5.0, input.y + 5.0),
            ChromeAction::Consumed
        );
        assert!(chrome.add_host_is_open());
        assert_eq!(chrome.form.focused_field(), Field::Hostname);

        let cancel = layout.secondary_button_rect(&chrome.form);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, cancel.x + 4.0, cancel.y + 4.0),
            ChromeAction::Consumed
        );
        assert!(!chrome.add_host_is_open());

        chrome.open_add_host();
        chrome.form.insert("srv.local");
        let next_btn = layout.primary_button_rect(&chrome.form);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, next_btn.x + 4.0, next_btn.y + 4.0),
            ChromeAction::Consumed
        );
        assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Auth);

        chrome.form.set_step(crate::add_host::AddHostStep::Details);
        let layout_details = chrome.dialog_layout(1200.0, 800.0);
        let connect = layout_details.primary_button_rect(&chrome.form);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, connect.x + 4.0, connect.y + 4.0),
            ChromeAction::SubmitHostForm
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
        assert_eq!(
            chrome.panel.scroll,
            3.0 * (crate::sidebar::ITEM_HEIGHT + crate::sidebar::CARD_GAP)
        );

        // The terminal's half of the window is untouched.
        assert!(!chrome.handle_wheel(h, 900.0, 300.0, -3.0));
        assert_eq!(
            chrome.panel.scroll,
            3.0 * (crate::sidebar::ITEM_HEIGHT + crate::sidebar::CARD_GAP)
        );

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
        assert_eq!(chrome.form.value(Field::Hostname), "w");

        // Step 1 (Target) Enter advances to Auth
        assert_eq!(
            chrome.handle_form_input(FormInput::Enter, ""),
            Some(FormOutcome::Consumed)
        );
        assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Auth);

        // Configure valid auth for Auth step
        chrome.form.select_auth_method(2);

        // Step 2 (Auth) Enter advances to Details
        assert_eq!(
            chrome.handle_form_input(FormInput::Enter, ""),
            Some(FormOutcome::Consumed)
        );
        assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Details);

        // Step 3 (Details) Enter submits
        assert_eq!(
            chrome.handle_form_input(FormInput::Enter, ""),
            Some(FormOutcome::Submit)
        );
        // Enter is a request to save, not a dismissal: the caller closes
        // the form only once the repository accepted the host.
        assert!(chrome.add_host_is_open());

        // Escape on Details goes back to Auth, then Target, then dismisses
        assert_eq!(
            chrome.handle_form_input(FormInput::Escape, ""),
            Some(FormOutcome::Consumed)
        );
        assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Auth);

        assert_eq!(
            chrome.handle_form_input(FormInput::Escape, ""),
            Some(FormOutcome::Consumed)
        );
        assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Target);

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
        let row = chrome.panel.item_rect(0.0, 2);
        assert!(chrome.handle_hover(800.0, row.x + 20.0, row.y + 20.0));
        assert!(!chrome.panel.add_hover);
        assert_eq!(chrome.panel.hover, Some(2));
    }

    #[test]
    fn modal_paint_stack_puts_vault_above_host_editor() {
        let mut chrome = chrome_with_hosts(1);
        chrome.open_edit_host(
            crate::add_host::HostFormValues {
                name: "h".into(),
                hostname: "1.2.3.4".into(),
                username: "u".into(),
                port: "22".into(),
                auth_method: "password".into(),
                password: String::new(),
                identity_id: None,
                ..crate::add_host::HostFormValues::default()
            },
            "id-0".into(),
        );
        assert_eq!(chrome.top_modal_paint(), Some(ModalPaintLayer::HostEditor));
        chrome.open_vault_unlock(PendingVaultAction::SubmitHostForm);
        assert_eq!(
            chrome.modal_paint_stack(),
            vec![ModalPaintLayer::HostEditor, ModalPaintLayer::VaultUnlock]
        );
        assert_eq!(chrome.top_modal_paint(), Some(ModalPaintLayer::VaultUnlock));
    }

    #[test]
    fn settings_sits_above_host_editor_in_paint_stack() {
        let mut chrome = chrome_with_hosts(1);
        chrome.open_add_host();
        chrome.open_settings(SettingsTab::Keys);
        assert_eq!(
            chrome.modal_paint_stack(),
            vec![ModalPaintLayer::HostEditor, ModalPaintLayer::Settings]
        );
        assert_eq!(chrome.top_modal_paint(), Some(ModalPaintLayer::Settings));
    }

    // ---- Polish 4: overflowing machine list ----

    #[test]
    fn a_machine_selected_elsewhere_is_scrolled_into_view_once() {
        let mut chrome = chrome_with_hosts(30);
        let h = 630.0;
        // Selected from the palette: row 26 ("id-25") is below the fold.
        assert!(chrome.reveal_machine("id-25", h));
        let body = chrome.panel.body_rect(0.0, h);
        assert!(chrome.panel.row_painted(0.0, h, 26));
        assert!(chrome.panel.card_rect(0.0, 26).bottom() <= body.bottom() + 0.01);

        // The user then scrolls away: the same selection does not pull
        // the list back on every frame.
        chrome.panel.scroll = 0.0;
        assert!(!chrome.reveal_machine("id-25", h));
        assert_eq!(chrome.panel.scroll, 0.0);

        // Unknown ids (Local without a row, a collapsed group) are a no-op.
        assert!(!chrome.reveal_machine("nope", h));
    }

    #[test]
    fn pixel_wheel_deltas_scroll_the_list_too() {
        let mut chrome = chrome_with_hosts(30);
        let h = 630.0;
        let row = chrome.panel.item_rect(0.0, 3);
        // Touchpads send pixels, not lines (positive = content moves down).
        assert!(chrome.handle_wheel_pixels(h, row.x + 10.0, row.y + 5.0, -30.0));
        assert_eq!(chrome.panel.scroll, 30.0);
        assert!(chrome.handle_wheel_pixels(h, row.x + 10.0, row.y + 5.0, 100.0));
        assert_eq!(chrome.panel.scroll, 0.0);
        // Outside the sidebar the terminal keeps the wheel.
        assert!(!chrome.handle_wheel_pixels(h, 600.0, 300.0, -30.0));
    }

    #[test]
    fn dragging_a_host_to_the_bottom_edge_auto_scrolls_and_retargets() {
        let mut chrome = chrome_with_hosts(30);
        let h = 630.0;
        chrome.set_window_size(922.0, h);
        let row = chrome.panel.card_rect(0.0, 2);
        chrome.handle_press(1200.0, h, row.x + 20.0, row.y + 20.0);
        let body = chrome.panel.body_rect(0.0, h);
        let edge_y = body.bottom() - 3.0;
        assert!(chrome.handle_drag_move(h, row.x + 20.0, edge_y));
        let before = chrome.panel.drop_target_at(0.0, h, row.x + 20.0, edge_y);
        for _ in 0..30 {
            chrome.tick_host_drag(1.0 / 60.0);
        }
        assert!(
            chrome.panel.scroll > 0.0,
            "the list follows the dragged host"
        );
        let after = chrome.panel.host_drag.as_ref().unwrap().drop_target.clone();
        assert!(after.is_some());
        assert_ne!(after, before, "the drop target follows the scrolled rows");
        // Released there, the drop lands on the row now under the pointer.
        let action = chrome.handle_release(h, row.x + 20.0, edge_y);
        assert!(matches!(action, ChromeAction::ReorderHost { .. }));
    }

    #[test]
    fn copy_button_on_the_add_host_error_copies_the_full_message() {
        let mut chrome = chrome_with_hosts(0);
        chrome.open_add_host();
        let message = "Could not save the host: Database error: error returned from \
                       database: (code: 1) table hosts has no column named os_id";
        chrome.form.set_error(message);
        let layout = chrome.dialog_layout(1200.0, 800.0);
        let copy = layout.copy_error_rect(&chrome.form).expect("copy button");

        let action = chrome.handle_press(1200.0, 800.0, copy.x + 2.0, copy.y + 2.0);

        assert_eq!(action, ChromeAction::CopyText(message.into()));
        assert!(chrome.form.is_open(), "copying keeps the dialog open");
    }
}
