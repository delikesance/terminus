use crate::geom::Rect;

pub const HEADER_PAD_LEFT: f32 = 4.0;
pub const HEADER_GAP: f32 = 28.0;
pub const HEADER_TITLE_SIZE: f32 = 28.0;
pub const HEADER_ADDRESS_SIZE: f32 = 11.0;
/// Line box of the address row.
pub const HEADER_ADDRESS_H: f32 = 14.0;
pub const HEADER_TITLE_ADDRESS_GAP: f32 = 4.0;
pub const HEADER_TITLE_PAD_BOTTOM: f32 = 14.0;
/// Height of a view tab (text, 14 gap, 2 underline).
pub const HEADER_TAB_H: f32 = 34.0;
/// Gap between view tabs.
pub const HEADER_TAB_GAP: f32 = 22.0;
pub const HEADER_BORDER: f32 = 1.0;
/// Text line box is 1.25 x font size; the mock's name uses `line-height: 1`,
/// so the title is drawn this much higher than its rect top.
pub const HEADER_TITLE_INK_DY: f32 = -HEADER_TITLE_SIZE * 0.25 / 2.0;

/// Header rects. The tabs row sits on the bottom border.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeaderRects {
    pub bounds: Rect,
    pub title: Rect,
    /// `None` for the Settings variant (title only).
    pub address: Option<Rect>,
    pub tabs: Rect,
    pub border: Rect,
}

/// Total header height (including the 1px border).
pub fn header_height(has_address: bool) -> f32 {
    let text = HEADER_TITLE_SIZE
        + if has_address {
            HEADER_TITLE_ADDRESS_GAP + HEADER_ADDRESS_H
        } else {
            0.0
        };
    text + HEADER_TITLE_PAD_BOTTOM + HEADER_BORDER
}

/// Lay the header out at `(x, y)` over `width`. `block_w` is the measured
/// width of the widest of title and address, `tabs_w` the tabs row width.
pub fn header_rects(
    x: f32,
    y: f32,
    width: f32,
    has_address: bool,
    block_w: f32,
    tabs_w: f32,
) -> HeaderRects {
    let h = header_height(has_address);
    let bounds = Rect::new(x, y, width, h);
    let tx = x + HEADER_PAD_LEFT;
    let title = Rect::new(tx, y, block_w, HEADER_TITLE_SIZE);
    let address = has_address.then(|| {
        Rect::new(
            tx,
            y + HEADER_TITLE_SIZE + HEADER_TITLE_ADDRESS_GAP,
            block_w,
            HEADER_ADDRESS_H,
        )
    });
    let border = Rect::new(x, y + h - HEADER_BORDER, width, HEADER_BORDER);
    let tabs = Rect::new(
        tx + block_w + HEADER_GAP,
        border.y - HEADER_TAB_H,
        tabs_w,
        HEADER_TAB_H,
    );
    HeaderRects {
        bounds,
        title,
        address,
        tabs,
        border,
    }
}
