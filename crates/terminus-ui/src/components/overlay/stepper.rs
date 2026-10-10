use crate::geom::Rect;

pub const STEPPER_STEPS: [&str; 3] = ["Address", "Sign in", "Organise"];
pub const STEPPER_BAR_HEIGHT: f32 = 3.0;
pub const STEPPER_GAP: f32 = 8.0;
pub const STEPPER_LABEL_GAP: f32 = 8.0;
pub const STEPPER_LABEL_HEIGHT: f32 = 16.0;
pub const STEPPER_HEIGHT: f32 =
    STEPPER_BAR_HEIGHT + STEPPER_LABEL_GAP + STEPPER_LABEL_HEIGHT;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepSegment {
    pub bar: Rect,
    pub label_rect: Rect,
    pub label: &'static str,
    /// Bar painted in accent (this step is at or before the current one).
    pub filled: bool,
    /// Label painted in the text colour.
    pub current: bool,
}

/// Three segments across `area`; `step` is 1-based (clamped to 0..=3).
pub fn stepper_segments(area: Rect, step: usize) -> [StepSegment; 3] {
    let step = step.min(STEPPER_STEPS.len());
    let n = STEPPER_STEPS.len() as f32;
    let w = (area.width - STEPPER_GAP * (n - 1.0)) / n;
    std::array::from_fn(|i| {
        let x = area.x + i as f32 * (w + STEPPER_GAP);
        StepSegment {
            bar: Rect::new(x, area.y, w, STEPPER_BAR_HEIGHT),
            label_rect: Rect::new(
                x,
                area.y + STEPPER_BAR_HEIGHT + STEPPER_LABEL_GAP,
                w,
                STEPPER_LABEL_HEIGHT,
            ),
            label: STEPPER_STEPS[i],
            filled: i < step,
            current: i + 1 == step,
        }
    })
}
