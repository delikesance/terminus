//! The chrome as one object: activity rail + host panel + add-host editor.
//!
//! Everything the mouse and the keyboard can do to the chrome is routed
//! through here, and everything the chrome reserves from the terminal's
//! area is answered by [`Chrome::reserved_width`]. The painters read
//! this state and never own any of it, so a repaint can never disagree
//! with a hit-test.

use crate::activity_bar::{self, ActivityBarState, RailAction, RailHit, Section};
use crate::add_host::{AddHostForm, AddHostHit, FormInput, FormOutcome};
use crate::connection::{ConnectionHit, ConnectionSequence};
use crate::context_menu::{ContextAction, ContextMenu, ContextMenuHit};
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
    Connection,
    HostEditor,
    AddSnippet,
    Settings,
    VaultUnlock,
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
    /// Run a snippet command in the active terminal.
    RunSnippet(String),
    /// Settings modal was dismissed.
    DismissSettings,
}

/// Chrome state for one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    pub activity: ActivityBarState,
    pub panel: HostPanel,
    pub snippets: SnippetsPanel,
    pub settings: SettingsModal,
    pub form: AddHostForm,
    pub snippet_form: crate::add_snippet::AddSnippetForm,
    /// Prompt when a sealed secret is needed and the vault is locked.
    pub vault_unlock: VaultUnlockPrompt,
    /// Live SSH/WSL connecting modal, when a session is starting.
    pub connection: Option<ConnectionSequence>,
    /// Right-click context menu, when open.
    pub context_menu: Option<ContextMenu>,
    /// Unscaled height reserved above the chrome by the tab strip, so
    /// the rail starts under the tabs instead of behind them.
    pub top_inset: f32,
    /// Whether the panel is expanded beside the rail.
    pub panel_visible: bool,
    /// Last known window width (logical), for settings hover/cursor geometry.
    pub last_window_width: f32,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            activity: ActivityBarState::default(),
            panel: HostPanel::default(),
            snippets: SnippetsPanel::with_defaults(),
            settings: SettingsModal::default(),
            form: AddHostForm::default(),
            snippet_form: crate::add_snippet::AddSnippetForm::default(),
            vault_unlock: VaultUnlockPrompt::default(),
            connection: None,
            context_menu: None,
            top_inset: 0.0,
            panel_visible: true,
            last_window_width: 1200.0,
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
        self.panel_visible && self.activity.selected == Section::Servers
    }

    /// Whether the snippets drawer is showing.
    pub fn snippets_visible(&self) -> bool {
        self.panel_visible && self.activity.selected == Section::Snippets
    }

    /// Title shown in the panel header.
    pub fn panel_title(&self) -> &'static str {
        match self.activity.selected {
            Section::Servers => "Servers & Hosts",
            Section::Snippets => "Command Snippets",
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
        let mut stack = Vec::new();
        if self.connection.is_some() {
            stack.push(ModalPaintLayer::Connection);
        }
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
        stack
    }

    /// Front-most open overlay dialog, if any.
    pub fn top_modal_paint(&self) -> Option<ModalPaintLayer> {
        self.modal_paint_stack().last().copied()
    }

    /// Ask for the vault passphrase, then retry `pending` after unlock.
    pub fn open_vault_unlock(&mut self, pending: PendingVaultAction) {
        self.vault_unlock.open(pending);
    }

    /// Open the add-host editor.
    pub fn open_add_host(&mut self) {
        self.activity.selected = Section::Servers;
        self.activity.collapsed = false;
        self.panel_visible = true;
        self.form.open();
    }

    /// Open the host editor prefilled for an existing host.
    pub fn open_edit_host(
        &mut self,
        values: crate::add_host::HostFormValues,
        host_id: String,
    ) {
        self.activity.selected = Section::Servers;
        self.activity.collapsed = false;
        self.panel_visible = true;
        self.form.open_edit(values, host_id);
    }

    pub fn open_settings(&mut self, tab: SettingsTab) {
        self.settings.open_tab(tab);
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
        self.panel.filter = filter;
    }

    /// Current host-list filter text.
    pub fn filter(&self) -> &str {
        &self.panel.filter
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
            || self.connection.is_some()
            || self.form.is_open()
            || self.snippet_form.is_open()
            || self.vault_unlock.is_open()
        {
            self.close_context_menu();
            return ChromeAction::Ignored;
        }
        if !self.hosts_visible() || self.activity.collapsed {
            self.close_context_menu();
            return ChromeAction::Ignored;
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

    fn route_context_menu_press(&mut self, x: f32, y: f32) -> Option<ChromeAction> {
        let Some(menu) = self.context_menu.as_ref() else {
            return None;
        };
        match menu.hit_test(x, y) {
            ContextMenuHit::Dismiss => {
                self.close_context_menu();
                // Swallow the dismiss click so it does not open a host.
                Some(ChromeAction::Consumed)
            }
            ContextMenuHit::Consume => Some(ChromeAction::Consumed),
            ContextMenuHit::Item(index) => {
                let action = menu.take_action(index);
                self.close_context_menu();
                Some(match action {
                    Some(ContextAction::DeleteHost(id)) => ChromeAction::DeleteHost(id),
                    Some(ContextAction::DeleteGroup(id)) => ChromeAction::DeleteGroup(id),
                    Some(ContextAction::EditHost(id)) => ChromeAction::EditHost(id),
                    Some(ContextAction::OpenSftp(id)) => ChromeAction::OpenSftp(id),
                    Some(ContextAction::OpenSftpOtherPane(id)) => {
                        ChromeAction::OpenSftpOtherPane(id)
                    }
                    Some(ContextAction::RenameHost(id)) => ChromeAction::RenameHost(id),
                    Some(ContextAction::RenameGroup(id)) => ChromeAction::RenameGroup(id),
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
        // An open context menu eats the next left click: select or dismiss.
        if let Some(action) = self.route_context_menu_press(x, y) {
            return action;
        }

        // Vault unlock sits above every other dialog: a sealed secret was
        // requested and nothing else can proceed until the user answers.
        if self.vault_unlock.is_open() {
            let layout = VaultUnlockLayout::centered(window_width, window_height);
            return match layout.hit_test(x, y) {
                VaultUnlockHit::Field => ChromeAction::Consumed,
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
                SettingsHit::GenerateKey => match self.settings.take_key_draft_label() {
                    Ok(_name) => ChromeAction::GenerateSshKey,
                    Err(_) => ChromeAction::Consumed,
                },
                SettingsHit::CancelKeyDraft => {
                    self.settings.close_key_draft();
                    ChromeAction::Consumed
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

        // The connection modal sits above everything else: clicks never
        // fall through to the terminal or the add-host form behind it.
        if self.connection.is_some() {
            let hit = self.connection.as_ref().unwrap().hit_test(
                window_width,
                window_height,
                x,
                y,
            );
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
            let on_auth_menu = self.form.auth_menu_open()
                && layout
                    .auth_menu_rect(&self.form)
                    .is_some_and(|m| m.contains(x, y));
            let on_identity_menu = self.form.identity_menu_open()
                && layout
                    .identity_menu_rect(&self.form)
                    .is_some_and(|m| m.contains(x, y));
            if !dialog.contains(x, y) && !on_auth_menu && !on_identity_menu {
                self.form.close();
                return ChromeAction::Consumed;
            }
            return match layout.hit_test(&self.form, x, y) {
                AddHostHit::Field(field) => {
                    self.form.focus_field(field);
                    ChromeAction::Consumed
                }
                AddHostHit::ToggleAuthMenu => {
                    self.form.toggle_auth_menu();
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
                AddHostHit::TogglePasswordVisible => {
                    self.form.toggle_password_visible();
                    ChromeAction::Consumed
                }
                AddHostHit::Cancel => {
                    self.form.close();
                    ChromeAction::Consumed
                }
                AddHostHit::Connect => ChromeAction::SubmitHostForm,
                AddHostHit::Consume => {
                    if self.form.auth_menu_open() {
                        self.form.close_auth_menu();
                    }
                    if self.form.identity_menu_open() {
                        self.form.close_identity_menu();
                    }
                    ChromeAction::Consumed
                }
            };
        }

        if self.activity.collapsed {
            return ChromeAction::Ignored;
        }

        let origin_y = self.origin_y();
        // Tab strip / title band live above the chrome; never steal those hits.
        if y < origin_y {
            return ChromeAction::Ignored;
        }
        let chrome_height = (window_height - origin_y).max(0.0);
        if let Some(hit) = activity_bar::hit_test(origin_y, chrome_height, x, y) {
            return match hit {
                RailHit::Section(section) => {
                    if section == self.activity.selected {
                        self.panel_visible = !self.panel_visible;
                    } else {
                        self.activity.selected = section;
                        self.panel_visible = true;
                    }
                    ChromeAction::Consumed
                }
                RailHit::Action(RailAction::Settings) => {
                    self.open_settings(SettingsTab::Keys);
                    ChromeAction::Consumed
                }
                RailHit::Action(RailAction::CloudSync) => {
                    self.open_settings(SettingsTab::SqlSync);
                    ChromeAction::Consumed
                }
            };
        }

        if !self.panel_visible {
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
        if let Some(menu) = self.context_menu.as_mut() {
            return menu.hover_at(x, y);
        }
        if self.settings.open {
            return self
                .settings
                .handle_hover(window_width, window_height, x, y);
        }
        if self.connection.is_some() || self.activity.collapsed {
            return false;
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
            if self.form.auth_menu_open() {
                let mut hover = None;
                for i in 0..crate::add_host::AUTH_METHODS.len() {
                    if let Some(opt) = layout.auth_option_rect(&self.form, i) {
                        if opt.contains(x, y) {
                            hover = Some(i);
                            break;
                        }
                    }
                }
                changed |= self.form.set_auth_menu_hover(hover);
            } else {
                changed |= self.form.set_auth_menu_hover(None);
            }
            if self.form.identity_menu_open() {
                let mut hover = None;
                for i in 0..self.form.identities().len() {
                    if let Some(opt) = layout.identity_option_rect(&self.form, i) {
                        if opt.contains(x, y) {
                            hover = Some(i);
                            break;
                        }
                    }
                }
                changed |= self.form.set_identity_menu_hover(hover);
            } else {
                changed |= self.form.set_identity_menu_hover(None);
            }
            return changed;
        }
        let origin_y = self.origin_y();
        let height = window_height - origin_y;
        if self.snippets_visible() {
            let hit = self.snippets.hit_test(origin_y, height, x, y);
            return self.snippets.set_hover(hit);
        }
        if !self.hosts_visible() {
            return false;
        }
        let hover = self.panel.hover_at(origin_y, height, x, y);
        self.panel.set_hover(hover)
    }

    /// Remember the last layout width so hover/cursor can rebuild dialog rects.
    pub fn set_window_size(&mut self, width: f32, _height: f32) {
        self.last_window_width = width;
    }

    /// Cursor affordance under `(x, y)`.
    pub fn cursor_at(
        &self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ChromeCursor {
        if let Some(menu) = self.context_menu.as_ref() {
            return match menu.hit_test(x, y) {
                ContextMenuHit::Item(_) => ChromeCursor::Pointer,
                ContextMenuHit::Consume | ContextMenuHit::Dismiss => {
                    ChromeCursor::Default
                }
            };
        }
        if self.vault_unlock.is_open() {
            let layout = VaultUnlockLayout::centered(window_width, window_height);
            return match layout.hit_test(x, y) {
                VaultUnlockHit::Field => ChromeCursor::Text,
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
        if let Some(conn) = self.connection.as_ref() {
            return match conn.hit_test(window_width, window_height, x, y) {
                ConnectionHit::Close | ConnectionHit::ToggleLogs => ChromeCursor::Pointer,
                ConnectionHit::Consume => ChromeCursor::Default,
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
            let on_auth_menu = self.form.auth_menu_open()
                && layout
                    .auth_menu_rect(&self.form)
                    .is_some_and(|m| m.contains(x, y));
            let on_identity_menu = self.form.identity_menu_open()
                && layout
                    .identity_menu_rect(&self.form)
                    .is_some_and(|m| m.contains(x, y));
            if !dialog.contains(x, y) && !on_auth_menu && !on_identity_menu {
                return ChromeCursor::Pointer; // scrim dismiss
            }
            return match layout.hit_test(&self.form, x, y) {
                AddHostHit::Field(_) => ChromeCursor::Text,
                AddHostHit::ToggleAuthMenu
                | AddHostHit::SelectAuth(_)
                | AddHostHit::ToggleIdentityMenu
                | AddHostHit::SelectIdentity(_)
                | AddHostHit::TogglePasswordVisible
                | AddHostHit::Connect
                | AddHostHit::Cancel => ChromeCursor::Pointer,
                AddHostHit::Consume => ChromeCursor::Default,
            };
        }
        if self.activity.collapsed {
            return ChromeCursor::Default;
        }
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        if activity_bar::hit_test(origin_y, height, x, y).is_some() {
            return ChromeCursor::Pointer;
        }
        if self.snippets_visible() {
            return match self.snippets.hit_test(origin_y, height, x, y) {
                Some(SnippetHit::Item(_))
                | Some(SnippetHit::AddButton)
                | Some(SnippetHit::DeleteButton(_)) => ChromeCursor::Pointer,
                Some(SnippetHit::Background) | None => ChromeCursor::Default,
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

    /// Kept for the animation loop; host-drag no longer uses a snap tween.
    pub fn tick_host_drag(&mut self, _dt: f32) -> Option<ChromeAction> {
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

    /// Route keyboard input to the vault unlock prompt.
    ///
    /// Returns `Some(true)` when Unlock should be submitted.

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
            self.form.height(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity_bar::RailAction;
    use crate::add_host::Field;
    use crate::settings::SettingsTab;
    use crate::sidebar::Badge;

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
        let item = menu.item_rect(3).unwrap();
        let action = chrome.handle_press(1200.0, 800.0, item.x + 4.0, item.y + 4.0);
        assert_eq!(action, ChromeAction::DeleteHost("id-0".to_string()));
        assert!(chrome.context_menu.is_none());
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
        assert!(chrome.handle_drag_move(800.0, group.x + 20.0, group.y + 20.0));
        assert!(chrome.panel.host_drag.as_ref().is_some_and(|d| d.started()));
        let action = chrome.handle_release(800.0, group.x + 20.0, group.y + 20.0);
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
        // Including the rail's own background, between its buttons.
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, 32.0, 300.0),
            ChromeAction::Ignored
        );
    }

    #[test]
    fn the_rail_switches_sections_and_toggles_the_panel() {
        let mut chrome = chrome_with_hosts(1);
        let snippets = activity_bar::section_rect(0.0, Section::Snippets);
        let (x, y) = (32.0, snippets.y + 10.0);

        assert_eq!(
            chrome.handle_press(1200.0, 800.0, x, y),
            ChromeAction::Consumed
        );
        assert_eq!(chrome.activity.selected, Section::Snippets);
        assert!(!chrome.hosts_visible());
        assert!(chrome.snippets_visible());

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
        chrome.activity.selected = Section::Snippets;
        // Clear demo snippets so a press in the drawer body is not a RunSnippet.
        chrome.snippets.items.clear();
        let row = chrome.panel.item_rect(0.0, 2);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::Consumed
        );
        assert_eq!(chrome.panel.selected, None);
    }

    #[test]
    fn settings_action_opens_the_settings_modal() {
        let mut chrome = chrome_with_hosts(1);
        let settings = activity_bar::action_rect(0.0, 800.0, RailAction::Settings);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, settings.x + 20.0, settings.y + 10.0),
            ChromeAction::Consumed
        );
        assert!(chrome.settings_is_open());
        assert_eq!(chrome.settings.tab, SettingsTab::Keys);
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

        let cancel = layout.cancel_button_rect(chrome.form.height());
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, cancel.x + 4.0, cancel.y + 4.0),
            ChromeAction::Consumed
        );
        assert!(!chrome.add_host_is_open());

        chrome.open_add_host();
        let connect = layout.connect_button_rect(chrome.form.height());
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
        let row = chrome.panel.item_rect(0.0, 2);
        assert!(chrome.handle_hover(800.0, row.x + 20.0, row.y + 20.0));
        assert!(!chrome.panel.add_hover);
        assert_eq!(chrome.panel.hover, Some(2));
    }

    #[test]
    fn the_client_area_excludes_the_top_inset() {
        let mut chrome = chrome_with_hosts(20);
        // Inset larger than one card so the same absolute Y maps to a
        // different host once the inset is cleared.
        chrome.top_inset = 80.0;
        let row = chrome.panel.item_rect(80.0, 1);
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::Consumed
        );
        assert_eq!(
            chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::OpenHost("id-0".to_string())
        );
        // Clicks in the tab strip (above the chrome) must not be swallowed —
        // otherwise tab close / switch stop working.
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, row.x + 20.0, 40.0),
            ChromeAction::Ignored
        );
        chrome.top_inset = 0.0;
        chrome.panel.selected = None;
        assert_eq!(
            chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::Consumed
        );
        // After inset clear, the same absolute Y is a different host; release
        // still opens the armed id from press (id-0 was wrong — we re-armed).
        let armed = chrome
            .panel
            .host_drag
            .as_ref()
            .map(|d| d.host_id.clone())
            .expect("armed");
        assert_eq!(
            chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0),
            ChromeAction::OpenHost(armed)
        );
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
}
