/// Dialog metrics, in logical pixels (the mock's 560 wide dialog).
pub const WIDTH: f32 = 560.0;
pub const PAD_X: f32 = 28.0;
pub const HEADER_TOP: f32 = 24.0;
pub const CLOSE_SIZE: f32 = 36.0;
pub const CLOSE_RIGHT: f32 = 24.0;
pub const STEPPER_TOP: f32 = 18.0;
pub const BODY_PAD: f32 = 26.0;
pub const ROW_GAP: f32 = 18.0;
pub const PAIR_GAP: f32 = 12.0;
pub const CARD_GAP: f32 = 8.0;
pub const FOOTER_TOP: f32 = 16.0;
pub const FOOTER_BOTTOM: f32 = 24.0;
pub const BUTTON_HEIGHT: f32 = 44.0;
pub const BUTTON_GAP: f32 = 10.0;
/// Gap between the footer hint and the buttons.
pub const FOOTER_TEXT_GAP: f32 = 16.0;
/// One 13px line of hint text under the key select.
pub const HINT_LINE: f32 = 18.0;
pub const MENU_ROW: f32 = 36.0;
pub const MENU_PAD: f32 = 4.0;
/// Title size (Sora SemiBold, violet revision).
pub const TITLE_FONT: f32 = 24.0;
/// Dialog corner radius.
pub const DIALOG_RADIUS: f32 = crate::tokens::radius::DIALOG;

/// Measured label widths (Sora 14, Medium for Cancel/Back, SemiBold for
/// the primary): buttons are `2 * 20` wider than their label.
pub const LABEL_CANCEL: f32 = 47.0;
pub const LABEL_BACK: f32 = 33.0;
pub const LABEL_CONTINUE: f32 = 67.0;
pub const LABEL_SAVE: f32 = 82.0;
pub const LABEL_COPY: f32 = 34.0;
/// Where the "Generate one" link starts / how wide it is on the hint line
/// ("No key yet? " then the link, Sora 13).
pub const HINT_LINK_X: f32 = 76.0;
pub const HINT_LINK_W: f32 = 86.0;
