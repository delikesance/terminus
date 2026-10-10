/// Visual state of a session pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillState {
    Default,
    Hover,
    Active,
    /// Output arrived in a session that is not being looked at.
    NewOutput,
}

/// Which part of a pill was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillHit {
    Body,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillStyle {
    pub bg: Option<[f32; 4]>,
    pub text: [u8; 4],
    /// Show the close × (hover and active).
    pub close: bool,
    /// Show the 6px accent activity dot.
    pub dot: bool,
}
