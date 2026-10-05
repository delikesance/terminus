//! SFTP name-conflict dialog (With-option: Replace / Keep existing).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::overlay::DialogFocus;
use terminus_ui::sftp_pane::{SftpHit, SftpPaneLayout, SftpPaneState};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::SftpConflictPrompt;

use super::confirm::{paint_confirm, ConfirmView};

/// Paint the conflict dialog over the SFTP pane (scrim covers the pane only).
///
/// Uses the overlay layer: UI text always composites above quads, so
/// without it the list rows' glyphs would draw over the dialog.
pub fn paint_sftp_conflict(
    sugarloaf: &mut Sugarloaf,
    state: &SftpPaneState,
    layout: &SftpPaneLayout,
    theme: &ChromeTheme,
    prompt: &SftpConflictPrompt,
) {
    let hover = match state.hover {
        Some(SftpHit::ConflictOverwrite) => Some(DialogFocus::Confirm),
        Some(SftpHit::ConflictKeep) => Some(DialogFocus::Cancel),
        _ => None,
    };
    let view = ConfirmView {
        focus: Some(prompt.focus),
        hover,
        option_checked: prompt.apply_to_all,
    };
    let spec = prompt.spec();
    let dialog = layout.conflict_layout(prompt);
    sugarloaf.begin_overlay();
    paint_confirm(sugarloaf, theme, &spec, &dialog, view, true);
    sugarloaf.end_overlay();
}
