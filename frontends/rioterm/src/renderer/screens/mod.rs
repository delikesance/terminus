//! Painters of the workspace views shown in place of the terminal: Files,
//! Tunnels, Snippets, History, Settings and Home.
//!
//! Contract (see `SHELL_CONTRACT.md`): one file per view, each exposing
//! `pub fn paint(sugarloaf, theme, content, state, device_scale)` that
//! paints inside `content` only (the shell has already covered it with the
//! card colour), and optionally `pub fn measure(sugarloaf, state)` to record
//! text widths its layout needs. State, layout and input live in
//! `terminus_ui::screens::<view>`; actions are carried out in
//! `crate::screen::workspace::<view>`.
//!
//! `add_server` is the Add a server dialog painter (wizard), not a view.

pub mod add_server;
pub mod files;
pub mod home;

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonSpec, ButtonState};
use terminus_ui::geom::Rect;
use terminus_ui::screens::{empty_state, EMPTY_BODY_SIZE, EMPTY_TITLE_SIZE};
use terminus_ui::shell::WorkspaceView;
use terminus_ui::theme::ChromeTheme;

use crate::renderer::components::button::paint_button;
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

/// View state that lives on the frontend `Screen` (it needs the SFTP
/// session, the process runtime or text measure), handed to the painter.
pub struct ViewStates<'a> {
    /// Open SFTP session of the Files view.
    pub files: Option<(
        &'a terminus_ui::SftpPaneState,
        &'a crate::renderer::views::files::FilesView,
    )>,
    pub settings: &'a terminus_ui::views::settings::SettingsView,
    pub history: &'a terminus_ui::views::history::HistoryState,
    pub tunnels: Option<&'a terminus_ui::views::tunnels::TunnelsState>,
}

/// Paint the current view into `content`.
pub fn paint(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    views: &ViewStates,
    theme: &ChromeTheme,
    content: Rect,
    device_scale: f32,
) {
    let s = &chrome.screens;
    match chrome.shell.view() {
        WorkspaceView::Terminal => {}
        WorkspaceView::Files => match views.files {
            Some((state, view)) => crate::renderer::views::files::paint(
                sugarloaf, theme, content, state, view,
            ),
            None => files::paint(sugarloaf, theme, content, &s.files, device_scale),
        },
        WorkspaceView::Tunnels => {
            let Some(tunnels) = views.tunnels else {
                return;
            };
            let size = sugarloaf.window_size();
            let scale = sugarloaf.scale_factor();
            let window = Rect::new(0.0, 0.0, size.width / scale, size.height / scale);
            crate::renderer::views::tunnels::paint(
                sugarloaf, theme, window, content, tunnels,
            )
        }
        WorkspaceView::Snippets => crate::renderer::views::snippets::paint_measured(
            sugarloaf,
            theme,
            content,
            &s.snippets,
            device_scale,
        ),
        WorkspaceView::History => crate::renderer::views::history::paint(
            sugarloaf,
            theme,
            content,
            views.history,
            chrono::Utc::now().timestamp(),
        ),
        WorkspaceView::Settings(_) => crate::renderer::views::settings::paint(
            sugarloaf,
            theme,
            content,
            views.settings,
        ),
        WorkspaceView::Home => {
            home::paint(sugarloaf, theme, content, &s.home, device_scale)
        }
    }
}

/// Record the text widths the views' layouts need.
pub fn measure(sugarloaf: &mut Sugarloaf, chrome: &mut Chrome) {
    let s = &mut chrome.screens;
    files::measure(sugarloaf, &mut s.files);
    s.snippets.labels = crate::renderer::views::snippets::measure_labels(sugarloaf);
    home::measure(sugarloaf, &mut s.home);
}

/// Width of a Large Primary button label.
pub(crate) fn primary_label_w(sugarloaf: &mut Sugarloaf, label: &str) -> f32 {
    measure_ui_text(
        sugarloaf,
        label,
        ButtonSize::Large.font_size(),
        UiWeight::SemiBold,
    )
}

/// Title + one body line at the top-left of `content` (stub empty state).
pub(crate) fn paint_empty(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    title: &str,
    body: &str,
) {
    let e = empty_state(content);
    draw_ui_text(
        sugarloaf,
        e.title.0,
        e.title.1,
        title,
        EMPTY_TITLE_SIZE,
        theme.text,
        UiWeight::SemiBold,
    );
    draw_ui_text(
        sugarloaf,
        e.body.0,
        e.body.1,
        body,
        EMPTY_BODY_SIZE,
        theme.text_muted,
        UiWeight::Regular,
    );
}

/// A Large Primary button laid at `rect` (from the view's state).
pub(crate) fn paint_primary(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: Rect,
    label: &str,
    label_w: f32,
    hovered: bool,
) {
    let spec = ButtonSpec::label(
        (rect.x, rect.y),
        ButtonKind::Primary,
        ButtonSize::Large,
        label_w,
        false,
    );
    let state = if hovered {
        ButtonState::Hover
    } else {
        ButtonState::Default
    };
    paint_button(sugarloaf, theme, &spec, state, label, None);
}
