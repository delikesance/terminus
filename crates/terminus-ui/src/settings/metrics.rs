pub const MAX_WIDTH: f32 = 768.0;
use crate::geom::Rect;
pub const SIDEBAR_WIDTH: f32 = 224.0;
pub const HEIGHT_RATIO: f32 = 0.78;
pub const RADIUS: f32 = 16.0;

/// Equal inset around the input inside a SqlSync field card.
pub const FIELD_CARD_PAD: f32 = 12.0;
/// Space from card top to the value input (label sits above).
pub const FIELD_INPUT_TOP: f32 = 30.0;
pub const FIELD_INPUT_HEIGHT: f32 = 28.0;
/// Card height: top pad for label + input + matching bottom pad.
pub const FIELD_CARD_HEIGHT: f32 = FIELD_INPUT_TOP + FIELD_INPUT_HEIGHT + FIELD_CARD_PAD;
/// Gap between stacked field cards.
pub const FIELD_CARD_GAP: f32 = 12.0;
pub const FIELD_CARD_STEP: f32 = FIELD_CARD_HEIGHT + FIELD_CARD_GAP;
/// Horizontal inset for value text / placeholder inside the input.
pub const FIELD_TEXT_INSET: f32 = 10.0;
/// Trailing eye-toggle slot inside the passphrase input (keeps text clear).
pub const FIELD_EYE_SLOT: f32 = 28.0;
pub const FIELD_EYE_ICON: f32 = 16.0;
/// Soft error banner under the SqlSync action row (only when flagged).
pub const ERROR_BANNER_HEIGHT: f32 = 44.0;
pub const ERROR_BANNER_GAP: f32 = 10.0;
/// Status text line inside the SqlSync footer block (full content width).
pub const STATUS_TEXT_HEIGHT: f32 = 22.0;
/// Gap between the status line and the Unlock / Test Sync buttons.
pub const STATUS_ACTIONS_GAP: f32 = 10.0;
/// Button row height inside the SqlSync footer block.
pub const STATUS_ACTIONS_HEIGHT: f32 = 28.0;
/// Footer block: status on its own line, then the action buttons.
pub const STATUS_BLOCK_HEIGHT: f32 = FIELD_CARD_PAD
    + STATUS_TEXT_HEIGHT
    + STATUS_ACTIONS_GAP
    + STATUS_ACTIONS_HEIGHT
    + FIELD_CARD_PAD;
/// Shared Unlock / Test Sync button width.
pub const SYNC_ACTION_BTN_WIDTH: f32 = 110.0;
pub const SYNC_ACTION_BTN_GAP: f32 = 8.0;
/// "Forget saved passphrase" when a remembered vault secret exists.
pub const FORGET_PASSPHRASE_BTN_WIDTH: f32 = 168.0;
/// Dashed "New SSH Key" CTA height on the Keys tab.
pub const KEY_CTA_HEIGHT: f32 = 56.0;
pub const KEY_CTA_GAP: f32 = 12.0;
/// Stored key row height.
pub const KEY_ROW_HEIGHT: f32 = 56.0;
pub const KEY_ROW_GAP: f32 = 8.0;
/// Inline generate form under the CTA (three field cards + actions + error).
pub const KEY_DRAFT_HEIGHT: f32 = 252.0 + FIELD_CARD_STEP;
pub const KEY_DRAFT_GENERATE_WIDTH: f32 = 96.0;
pub const KEY_DRAFT_CANCEL_WIDTH: f32 = 72.0;
/// Max bytes for a pasted OpenSSH private key.
pub const KEY_PEM_MAX_BYTES: usize = 16_384;
pub const KEY_LABEL_MAX_BYTES: usize = 64;
/// Max bytes accepted by the SqlSync connection-URI field.
pub const SQL_URI_MAX_BYTES: usize = 1024;
/// Max bytes accepted by the SqlSync encryption-passphrase field.
pub const SQL_PASSPHRASE_MAX_BYTES: usize = 256;

/// Shared input row geometry inside a SqlSync field card (equal card pad).
pub fn field_input_in_card(card: Rect) -> Rect {
    Rect::new(
        card.x + FIELD_CARD_PAD,
        card.y + FIELD_INPUT_TOP,
        (card.width - 2.0 * FIELD_CARD_PAD).max(0.0),
        FIELD_INPUT_HEIGHT,
    )
}

/// Remote database engines exposed by the SqlSync selector.
pub const SQL_ENGINES: [&str; 2] = ["SQLite", "PostgreSQL"];
