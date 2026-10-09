//! The chrome as one object: shell (sidebar chrome, header, pills, views)
//! + host panel + dialogs.
//!
//! Everything the mouse and the keyboard can do to the chrome is routed
//! through here, and everything the chrome reserves from the terminal's
//! area is answered by [`Chrome::reserved_width`]. The painters read
//! this state and never own any of it, so a repaint can never disagree
//! with a hit-test.

use crate::action_menu::{ActionMenu, ContextAction};
use crate::add_host::{AddHostForm, AddHostHit, FormInput, FormOutcome};
use crate::components::overlay::{DialogKey, MenuHit};
use crate::confirm::{ConfirmAction, ConfirmOutcome, ConfirmPrompt};
use crate::connection::{ConnectionHit, ConnectionSequence};
use crate::lost_session::{LostOutcome, LostSession};
use crate::settings::{SettingsHit, SettingsModal, SettingsTab};
use crate::sidebar::{HostItem, HostPanel, PanelHit, Row};
use crate::snippets::{SnippetHit, SnippetsPanel};
use crate::vault_unlock::{
    PendingVaultAction, VaultUnlockHit, VaultUnlockLayout, VaultUnlockPrompt,
};

mod actions;
mod drag;
mod hover;
mod input;
mod modals;
mod press;
mod route_press;
mod state;

pub use actions::{ChromeAction, ChromeCursor, ModalPaintLayer};

#[cfg(test)]
mod connection_tests;
#[cfg(test)]
mod host_menu_tests;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod modal_tests;
#[cfg(test)]
mod shell_tests;
#[cfg(test)]
mod test_support;

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
    pub context_menu: Option<ActionMenu>,
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
