/// Visual state of a view tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    Default,
    Hover,
    Active,
}

/// Measured widths of one tab (text widths come from the painter).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabSize {
    pub label_w: f32,
    /// Badge text width, 0 when there is no badge.
    pub badge_w: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStyle {
    pub text: [u8; 4],
    pub medium: bool,
    /// 2px underline colour, if any.
    pub underline: Option<[f32; 4]>,
}
