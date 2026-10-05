//! Files view painter: empty state / error + Retry. With an SFTP session
//! open the SFTP pane (`renderer::sftp_pane`) is painted in the content
//! rect instead.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::screens::files::{FilesState, CTA, RETRY, TITLE};
use terminus_ui::theme::ChromeTheme;

pub fn measure(sugarloaf: &mut Sugarloaf, state: &mut FilesState) {
    if state.cta_label_w <= 0.0 {
        state.cta_label_w = super::primary_label_w(sugarloaf, CTA);
    }
    if state.retry_label_w <= 0.0 {
        state.retry_label_w = super::primary_label_w(sugarloaf, RETRY);
    }
}

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &FilesState,
    _device_scale: f32,
) {
    if state.session_open {
        return;
    }
    super::paint_empty(sugarloaf, theme, content, TITLE, &state.body());
    if let Some(r) = state.cta_rect(content) {
        super::paint_primary(
            sugarloaf,
            theme,
            r,
            state.cta_label(),
            if state.error.is_some() {
                state.retry_label_w
            } else {
                state.cta_label_w
            },
            state.cta_hover,
        );
    }
}
