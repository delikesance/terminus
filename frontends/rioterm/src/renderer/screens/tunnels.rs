//! Tunnels view painter (stub).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::screens::tunnels::{TunnelsState, TITLE};
use terminus_ui::theme::ChromeTheme;

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &TunnelsState,
    _device_scale: f32,
) {
    super::paint_empty(sugarloaf, theme, content, TITLE, &state.body());
}
