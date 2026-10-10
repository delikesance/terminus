/// Actions that can be triggered from the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    TabCreate,
    TabClose,
    TabCloseUnfocused,
    SelectNextTab,
    SelectPrevTab,
    SplitRight,
    SplitDown,
    SelectNextSplit,
    SelectPrevSplit,
    ConfigEditor,
    WindowCreateNew,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    ToggleViMode,
    ToggleFullscreen,
    ToggleAppearanceTheme,
    Copy,
    Paste,
    SearchForward,
    SearchBackward,
    ClearHistory,
    CloseCurrentSplitOrTab,
    /// Browse the family names of every registered font. Does NOT
    /// execute a one-shot action — the palette stays open with the
    /// font list as its contents. Handled by `router`, not
    /// `Screen::execute_palette_action`.
    ListFonts,
    /// Browse stored SSH hosts. Same stay-open mode-switch pattern as
    /// [`Self::ListFonts`]. Enter on a host opens a session.
    ListHosts,
    /// Open the SFTP dual-pane: directly with one host, else via a picker.
    OpenSftp,
    /// Look for a new Terminus release now.
    CheckForUpdates,
    /// Download and install the release found by the last check.
    InstallUpdate,
    /// Relaunch into an update that is already installed.
    RestartToUpdate,
    Quit,
    /// Shell: switch the workspace view.
    ShowView(terminus_ui::shell::WorkspaceView),
    /// Shell: show the sidebar's server filter field.
    FilterServers,
    /// Fill the query with this text and keep the palette open (the
    /// `>forward start ` entry points). Handled in the confirm path.
    Prefill(&'static str),
}

pub(super) struct Command {
    pub(super) title: &'static str,
    pub(super) action: PaletteAction,
}

/// Every action the command catalog offers.
pub fn command_actions() -> impl Iterator<Item = PaletteAction> {
    COMMANDS.iter().map(|cmd| cmd.action)
}

impl PaletteAction {
    /// The key-binding action this command runs, whose shortcut the
    /// palette shows. `None` for palette-only commands.
    pub fn binding_action(self) -> Option<crate::bindings::Action> {
        use crate::bindings::Action;
        Some(match self {
            PaletteAction::TabCreate => Action::TabCreateNew,
            PaletteAction::TabClose => Action::TabCloseCurrent,
            PaletteAction::TabCloseUnfocused => Action::TabCloseUnfocused,
            PaletteAction::SelectNextTab => Action::SelectNextTab,
            PaletteAction::SelectPrevTab => Action::SelectPrevTab,
            PaletteAction::SplitRight => Action::SplitRight,
            PaletteAction::SplitDown => Action::SplitDown,
            PaletteAction::SelectNextSplit => Action::SelectNextSplit,
            PaletteAction::SelectPrevSplit => Action::SelectPrevSplit,
            PaletteAction::CloseCurrentSplitOrTab => Action::CloseCurrentSplitOrTab,
            PaletteAction::ConfigEditor => Action::ConfigEditor,
            PaletteAction::WindowCreateNew => Action::WindowCreateNew,
            PaletteAction::IncreaseFontSize => Action::IncreaseFontSize,
            PaletteAction::DecreaseFontSize => Action::DecreaseFontSize,
            PaletteAction::ResetFontSize => Action::ResetFontSize,
            PaletteAction::ToggleViMode => Action::ToggleViMode,
            PaletteAction::ToggleFullscreen => Action::ToggleFullscreen,
            PaletteAction::ToggleAppearanceTheme => Action::ToggleAppearanceTheme,
            PaletteAction::Copy => Action::Copy,
            PaletteAction::Paste => Action::Paste,
            PaletteAction::SearchForward => Action::SearchForward,
            PaletteAction::SearchBackward => Action::SearchBackward,
            PaletteAction::ClearHistory => Action::ClearHistory,
            PaletteAction::Quit => Action::Quit,
            _ => return None,
        })
    }
}

pub(super) const COMMANDS: &[Command] = &[
    Command {
        title: "New Tab",
        action: PaletteAction::TabCreate,
    },
    Command {
        title: "Close Tab",
        action: PaletteAction::TabClose,
    },
    Command {
        title: "Close Other Tabs",
        action: PaletteAction::TabCloseUnfocused,
    },
    Command {
        title: "Next Tab",
        action: PaletteAction::SelectNextTab,
    },
    Command {
        title: "Previous Tab",
        action: PaletteAction::SelectPrevTab,
    },
    Command {
        title: "Split Right",
        action: PaletteAction::SplitRight,
    },
    Command {
        title: "Split Down",
        action: PaletteAction::SplitDown,
    },
    Command {
        title: "Next Split",
        action: PaletteAction::SelectNextSplit,
    },
    Command {
        title: "Previous Split",
        action: PaletteAction::SelectPrevSplit,
    },
    Command {
        title: "Close Split or Tab",
        action: PaletteAction::CloseCurrentSplitOrTab,
    },
    Command {
        title: "Settings",
        action: PaletteAction::ConfigEditor,
    },
    Command {
        title: "New Window",
        action: PaletteAction::WindowCreateNew,
    },
    Command {
        title: "Increase Font Size",
        action: PaletteAction::IncreaseFontSize,
    },
    Command {
        title: "Decrease Font Size",
        action: PaletteAction::DecreaseFontSize,
    },
    Command {
        title: "Reset Font Size",
        action: PaletteAction::ResetFontSize,
    },
    Command {
        title: "Toggle Vi Mode",
        action: PaletteAction::ToggleViMode,
    },
    Command {
        title: "Toggle Fullscreen",
        action: PaletteAction::ToggleFullscreen,
    },
    Command {
        title: "Toggle Appearance Theme",
        action: PaletteAction::ToggleAppearanceTheme,
    },
    Command {
        title: "Copy",
        action: PaletteAction::Copy,
    },
    Command {
        title: "Paste",
        action: PaletteAction::Paste,
    },
    Command {
        title: "Search Forward",
        action: PaletteAction::SearchForward,
    },
    Command {
        title: "Search Backward",
        action: PaletteAction::SearchBackward,
    },
    Command {
        title: "Clear History",
        action: PaletteAction::ClearHistory,
    },
    Command {
        title: "List Fonts",
        action: PaletteAction::ListFonts,
    },
    Command {
        title: "Open Host…",
        action: PaletteAction::ListHosts,
    },
    Command {
        title: "Open SFTP",
        action: PaletteAction::OpenSftp,
    },
    Command {
        title: "Check for Updates",
        action: PaletteAction::CheckForUpdates,
    },
    Command {
        title: "Install Update",
        action: PaletteAction::InstallUpdate,
    },
    Command {
        title: "Restart to Update",
        action: PaletteAction::RestartToUpdate,
    },
    Command {
        title: "Quit",
        action: PaletteAction::Quit,
    },
    Command {
        title: "Show Terminal",
        action: PaletteAction::ShowView(terminus_ui::shell::WorkspaceView::Terminal),
    },
    Command {
        title: "Show Files",
        action: PaletteAction::ShowView(terminus_ui::shell::WorkspaceView::Files),
    },
    Command {
        title: "Show Tunnels",
        action: PaletteAction::ShowView(terminus_ui::shell::WorkspaceView::Tunnels),
    },
    Command {
        title: "Show Snippets",
        action: PaletteAction::ShowView(terminus_ui::shell::WorkspaceView::Snippets),
    },
    Command {
        title: "Show History",
        action: PaletteAction::ShowView(terminus_ui::shell::WorkspaceView::History),
    },
    Command {
        title: "Go Home",
        action: PaletteAction::ShowView(terminus_ui::shell::WorkspaceView::Home),
    },
    Command {
        title: "Open Settings",
        action: PaletteAction::ShowView(terminus_ui::shell::WorkspaceView::Settings(
            terminus_ui::shell::SettingsPage::Keys,
        )),
    },
    Command {
        title: "Filter Servers",
        action: PaletteAction::FilterServers,
    },
    Command {
        title: "Start Tunnel…",
        action: PaletteAction::Prefill(">forward start "),
    },
    Command {
        title: "Stop Tunnel…",
        action: PaletteAction::Prefill(">forward stop "),
    },
];
