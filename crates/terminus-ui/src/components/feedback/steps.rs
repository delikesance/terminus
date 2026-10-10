use crate::connection::{NodeVisual, STEP_COUNT, TRACK_LINE_HEIGHT};
use crate::geom::Rect;
use crate::theme::ChromeTheme;

// ----------------------------------------------------------- connection steps

pub const STEP_NODE: f32 = 40.0;
/// Width of the translucent halo around the active node.
pub const STEP_HALO: f32 = 6.0;
/// Alpha of the halo (accent at 20 %).
pub const STEP_HALO_ALPHA: f32 = 0.2;
pub const STEP_LABELS: [&str; STEP_COUNT] = ["Local", "Network", "Handshake", "Shell"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Active,
    Done,
    Failed,
}

impl From<NodeVisual> for StepState {
    fn from(v: NodeVisual) -> Self {
        match v {
            NodeVisual::Pending => StepState::Pending,
            NodeVisual::Active => StepState::Active,
            NodeVisual::Done => StepState::Done,
        }
    }
}

/// Colours of one node: fill, glyph (number or icon), optional halo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepPaint {
    pub fill: [f32; 4],
    pub glyph: [f32; 4],
    pub halo: Option<[f32; 4]>,
}

pub fn step_paint(state: StepState, theme: &ChromeTheme) -> StepPaint {
    match state {
        StepState::Pending => StepPaint {
            fill: theme.surface,
            glyph: theme.text_faint.map(|c| c as f32 / 255.0),
            halo: None,
        },
        StepState::Active => StepPaint {
            fill: theme.accent,
            glyph: theme.on_accent,
            halo: Some([
                theme.accent[0],
                theme.accent[1],
                theme.accent[2],
                STEP_HALO_ALPHA,
            ]),
        },
        StepState::Done => StepPaint {
            fill: theme.step_done_bg,
            glyph: theme.success,
            halo: None,
        },
        StepState::Failed => StepPaint {
            fill: theme.step_failed_bg,
            glyph: theme.danger_text.map(|c| c as f32 / 255.0),
            halo: None,
        },
    }
}

/// States of the four nodes for a sequence stopped at `active` (0-based).
/// `failed` marks the active node as failed instead.
pub fn steps_at(active: usize, failed: bool) -> [StepState; STEP_COUNT] {
    let mut out = [StepState::Pending; STEP_COUNT];
    for (i, s) in out.iter_mut().enumerate() {
        *s = if i < active {
            StepState::Done
        } else if i == active {
            if failed {
                StepState::Failed
            } else {
                StepState::Active
            }
        } else {
            StepState::Pending
        };
    }
    out
}

/// Node + connector geometry of the four-node line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepLine {
    pub nodes: [Rect; STEP_COUNT],
    /// `segments[i]` joins node `i` to node `i + 1` (edge to edge).
    pub segments: [Rect; STEP_COUNT - 1],
}

impl StepLine {
    /// From node centre x positions (e.g. the output of
    /// [`crate::connection::track_layout_from_labels`] plus the dialog's x)
    /// and the shared centre line `cy`.
    pub fn from_centers(node_cx: &[f32; STEP_COUNT], cy: f32) -> Self {
        let node = |cx: f32| {
            Rect::new(
                cx - STEP_NODE * 0.5,
                cy - STEP_NODE * 0.5,
                STEP_NODE,
                STEP_NODE,
            )
        };
        let nodes = node_cx.map(node);
        let mut segments = [Rect::new(0.0, 0.0, 0.0, 0.0); STEP_COUNT - 1];
        for (i, seg) in segments.iter_mut().enumerate() {
            let x0 = nodes[i].right();
            let x1 = nodes[i + 1].x;
            *seg = Rect::new(
                x0,
                cy - TRACK_LINE_HEIGHT * 0.5,
                (x1 - x0).max(0.0),
                TRACK_LINE_HEIGHT,
            );
        }
        Self { nodes, segments }
    }

    /// Evenly spaced across `[x, x + width]` with node centres at the
    /// midpoints of four equal columns.
    pub fn evenly(x: f32, width: f32, cy: f32) -> Self {
        let col = width / STEP_COUNT as f32;
        let mut cx = [0.0; STEP_COUNT];
        for (i, c) in cx.iter_mut().enumerate() {
            *c = x + col * (i as f32 + 0.5);
        }
        Self::from_centers(&cx, cy)
    }

    /// Halo box around node `i` (grown by [`STEP_HALO`]).
    pub fn halo(&self, i: usize) -> Rect {
        let n = self.nodes[i];
        Rect::new(
            n.x - STEP_HALO,
            n.y - STEP_HALO,
            n.width + 2.0 * STEP_HALO,
            n.height + 2.0 * STEP_HALO,
        )
    }

    /// A segment is "lit" once the node it leaves is Done.
    pub fn segment_lit(states: &[StepState; STEP_COUNT], i: usize) -> bool {
        states[i] == StepState::Done
    }
}

// ------------------------------------------------------------------ progress
