//! History view (stub): commands run on the selected machine, with
//! "Run again".

use super::{ViewInput, ViewOutcome};
use crate::geom::Rect;

pub const TITLE: &str = "History";

/// What the History view asks the frontend to do (none yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryAction {}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistoryState {
    pub machine_name: String,
}

impl HistoryState {
    pub fn body(&self) -> String {
        "Commands you run here will show up in this list.".into()
    }

    pub fn is_clickable(&self, _content: Rect, _x: f32, _y: f32) -> bool {
        false
    }

    pub fn handle(&mut self, _content: Rect, _input: &ViewInput) -> ViewOutcome {
        ViewOutcome::Ignored
    }
}
