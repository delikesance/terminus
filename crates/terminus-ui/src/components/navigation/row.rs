use super::server_row;
use crate::geom::Rect;

/// Visual state of a server row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Default,
    Hover,
    Selected,
    Focus,
    Dragging,
}

/// Colour role of the right-hand meta text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaTone {
    Muted,
    Success,
}

/// Content of a server row's right meta slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowMeta {
    /// WSL stopped, plain SSH host: nothing on the right.
    None,
    /// WSL distro currently running ("Running", success colour).
    Running,
    /// Number of open sessions on the machine.
    Sessions(u32),
}

impl RowMeta {
    pub fn label(&self) -> Option<String> {
        match self {
            RowMeta::None => None,
            RowMeta::Running => Some("Running".to_owned()),
            RowMeta::Sessions(n) => Some(n.to_string()),
        }
    }

    pub fn tone(&self) -> MetaTone {
        match self {
            RowMeta::Running => MetaTone::Success,
            _ => MetaTone::Muted,
        }
    }
}

/// Resolved paint instructions for a server row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowStyle {
    pub bg: Option<[f32; 4]>,
    /// Name in Medium weight (otherwise Regular).
    pub medium: bool,
    /// Identity tile turns accent with an `on_accent` mark.
    pub tile_active: bool,
    /// Drop shadow under the row (dragging).
    pub shadow: bool,
    /// Focus ring (2px canvas gap + 2px accent).
    pub ring: bool,
}

/// True when `(px, py)` is inside `row`.
pub fn row_hit(row: &Rect, px: f32, py: f32) -> bool {
    row.contains(px, py)
}

/// `count` rows of [`server_row::HEIGHT`] stacked from `(x, y)` with `gap` between.
pub fn stack_rows(x: f32, y: f32, width: f32, count: usize, gap: f32) -> Vec<Rect> {
    (0..count)
        .map(|i| {
            Rect::new(
                x,
                y + i as f32 * (server_row::HEIGHT + gap),
                width,
                server_row::HEIGHT,
            )
        })
        .collect()
}

/// Index of the row under `(px, py)`.
pub fn hit_row(rows: &[Rect], px: f32, py: f32) -> Option<usize> {
    rows.iter().position(|r| row_hit(r, px, py))
}
