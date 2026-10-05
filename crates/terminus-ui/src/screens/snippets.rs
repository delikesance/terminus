//! Snippets view (stub): the saved command snippets, with New snippet and
//! per-row Paste / Run / Delete.
//!
//! This stub already carries everything the old sidebar drawer did (run,
//! add through the existing dialog, delete) so nothing is lost while the
//! next wave builds the real view (filter field, tags, list cards).

use super::{ViewAction, ViewInput, ViewOutcome, PAD};
use crate::components::button::{ButtonKind, ButtonSize, ButtonSpec};
use crate::geom::Rect;
use crate::snippets::SnippetItem;

pub const TITLE: &str = "Snippets";
pub const NEW: &str = "New snippet";
pub const PASTE: &str = "Paste";
pub const RUN: &str = "Run";
pub const DELETE: &str = "Delete";

/// Card height (16 pad + 15 title + 6 + 15 command + 16 pad).
pub const ROW_HEIGHT: f32 = 68.0;
pub const ROW_GAP: f32 = 10.0;
pub const ROW_RADIUS: f32 = 14.0;
pub const ROW_PAD_X: f32 = 20.0;
pub const BUTTON_GAP: f32 = 8.0;
/// Space between the header row and the first card.
pub const HEAD_GAP: f32 = 16.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnippetsAction {
    /// Open the add-snippet dialog.
    New,
    /// Type the command into the terminal without Enter.
    Paste(String),
    /// Type the command and press Enter.
    Run(String),
    /// Delete the snippet with this id.
    Delete(String),
}

/// What a point of the view is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetsHit {
    New,
    Paste(usize),
    Run(usize),
    Delete(usize),
    Row(usize),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SnippetsState {
    /// Mirror of the stored snippets (synced by the frontend).
    pub items: Vec<SnippetItem>,
    pub scroll: f32,
    pub hover: Option<SnippetsHit>,
    /// Measured label widths of New / Paste / Run / Delete (0 = estimate).
    pub label_w: [f32; 4],
}

fn label_w(measured: f32, label: &str, size: f32) -> f32 {
    if measured > 0.0 {
        measured
    } else {
        label.chars().count() as f32 * size * 0.55
    }
}

impl SnippetsState {
    pub fn new_button(&self, content: Rect) -> ButtonSpec {
        let size = ButtonSize::Large;
        let w = label_w(self.label_w[0], NEW, size.font_size());
        let probe = ButtonSpec::label((0.0, 0.0), ButtonKind::Primary, size, w, false);
        ButtonSpec::label(
            (content.right() - PAD - probe.width(), content.y + PAD),
            ButtonKind::Primary,
            size,
            w,
            false,
        )
    }

    /// Viewport of the scrolling list.
    pub fn list_rect(&self, content: Rect) -> Rect {
        let top = self.new_button(content).rect().bottom() + HEAD_GAP;
        Rect::new(
            content.x + PAD,
            top,
            (content.width - 2.0 * PAD).max(0.0),
            (content.bottom() - PAD - top).max(0.0),
        )
    }

    pub fn row_rect(&self, content: Rect, index: usize) -> Rect {
        let list = self.list_rect(content);
        Rect::new(
            list.x,
            list.y + index as f32 * (ROW_HEIGHT + ROW_GAP) - self.scroll,
            list.width,
            ROW_HEIGHT,
        )
    }

    /// Paste, Run, Delete buttons of a row, left to right.
    pub fn row_buttons(&self, content: Rect, index: usize) -> [ButtonSpec; 3] {
        let row = self.row_rect(content, index);
        let size = ButtonSize::Medium;
        let fs = size.font_size();
        let specs = [
            (ButtonKind::Secondary, label_w(self.label_w[1], PASTE, fs)),
            (ButtonKind::Secondary, label_w(self.label_w[2], RUN, fs)),
            (ButtonKind::Quiet, label_w(self.label_w[3], DELETE, fs)),
        ];
        let y = row.y + (ROW_HEIGHT - size.height()) * 0.5;
        let mut right = row.right() - ROW_PAD_X;
        let mut out =
            [ButtonSpec::label((0.0, 0.0), ButtonKind::Quiet, size, 0.0, false); 3];
        for (i, (kind, w)) in specs.iter().enumerate().rev() {
            let probe = ButtonSpec::label((0.0, 0.0), *kind, size, *w, false);
            right -= probe.width();
            out[i] = ButtonSpec::label((right, y), *kind, size, *w, false);
            right -= BUTTON_GAP;
        }
        out
    }

    pub fn content_height(&self) -> f32 {
        let n = self.items.len() as f32;
        (n * (ROW_HEIGHT + ROW_GAP) - ROW_GAP).max(0.0)
    }

    pub fn hit(&self, content: Rect, x: f32, y: f32) -> Option<SnippetsHit> {
        if self.new_button(content).rect().contains(x, y) {
            return Some(SnippetsHit::New);
        }
        let list = self.list_rect(content);
        if !list.contains(x, y) {
            return None;
        }
        for i in 0..self.items.len() {
            let row = self.row_rect(content, i);
            if !row.contains(x, y) {
                continue;
            }
            let [paste, run, delete] = self.row_buttons(content, i);
            return Some(if paste.rect().contains(x, y) {
                SnippetsHit::Paste(i)
            } else if run.rect().contains(x, y) {
                SnippetsHit::Run(i)
            } else if delete.rect().contains(x, y) {
                SnippetsHit::Delete(i)
            } else {
                SnippetsHit::Row(i)
            });
        }
        None
    }

    pub fn is_clickable(&self, content: Rect, x: f32, y: f32) -> bool {
        matches!(
            self.hit(content, x, y),
            Some(
                SnippetsHit::New
                    | SnippetsHit::Paste(_)
                    | SnippetsHit::Run(_)
                    | SnippetsHit::Delete(_)
            )
        )
    }

    pub fn handle(&mut self, content: Rect, input: &ViewInput) -> ViewOutcome {
        match *input {
            ViewInput::Press { x, y, .. } => {
                let item = |i: usize| self.items.get(i).cloned();
                let action = match self.hit(content, x, y) {
                    Some(SnippetsHit::New) => Some(SnippetsAction::New),
                    Some(SnippetsHit::Paste(i)) => {
                        item(i).map(|s| SnippetsAction::Paste(s.cmd))
                    }
                    Some(SnippetsHit::Run(i)) => {
                        item(i).map(|s| SnippetsAction::Run(s.cmd))
                    }
                    Some(SnippetsHit::Delete(i)) => {
                        item(i).map(|s| SnippetsAction::Delete(s.id))
                    }
                    _ => None,
                };
                match action {
                    Some(a) => ViewOutcome::Action(ViewAction::Snippets(a)),
                    None => ViewOutcome::Consumed,
                }
            }
            ViewInput::Move { x, y, .. } => {
                let hover = self.hit(content, x, y);
                if hover != self.hover {
                    self.hover = hover;
                    ViewOutcome::Redraw
                } else {
                    ViewOutcome::Consumed
                }
            }
            ViewInput::Wheel { lines, .. } => {
                let max =
                    (self.content_height() - self.list_rect(content).height).max(0.0);
                let next = (self.scroll + lines * (ROW_HEIGHT + ROW_GAP)).clamp(0.0, max);
                if next != self.scroll {
                    self.scroll = next;
                    ViewOutcome::Redraw
                } else {
                    ViewOutcome::Consumed
                }
            }
            _ => ViewOutcome::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Rect {
        Rect::new(260.0, 104.0, 1172.0, 788.0)
    }

    fn state(n: usize) -> SnippetsState {
        SnippetsState {
            items: (0..n)
                .map(|i| SnippetItem {
                    id: format!("s{i}"),
                    name: format!("Snippet {i}"),
                    cmd: format!("echo {i}"),
                    desc: String::new(),
                })
                .collect(),
            ..Default::default()
        }
    }

    fn press(x: f32, y: f32) -> ViewInput {
        ViewInput::Press {
            x,
            y,
            double: false,
        }
    }

    #[test]
    fn new_snippet_sits_top_right() {
        let s = state(0);
        let b = s.new_button(content()).rect();
        assert_eq!(b.right(), content().right() - PAD);
        assert_eq!(b.y, content().y + PAD);
        let mut s = s;
        assert_eq!(
            s.handle(content(), &press(b.x + 2.0, b.y + 2.0)),
            ViewOutcome::Action(ViewAction::Snippets(SnippetsAction::New))
        );
    }

    #[test]
    fn row_buttons_run_paste_and_delete_that_snippet() {
        let mut s = state(3);
        let [paste, run, delete] = s.row_buttons(content(), 1);
        assert!(paste.rect().right() < run.rect().x);
        assert!(run.rect().right() < delete.rect().x);
        assert_eq!(
            delete.rect().right(),
            s.row_rect(content(), 1).right() - ROW_PAD_X
        );
        let at = |r: Rect| press(r.x + 2.0, r.y + 2.0);
        assert_eq!(
            s.handle(content(), &at(run.rect())),
            ViewOutcome::Action(ViewAction::Snippets(SnippetsAction::Run(
                "echo 1".into()
            )))
        );
        assert_eq!(
            s.handle(content(), &at(paste.rect())),
            ViewOutcome::Action(ViewAction::Snippets(SnippetsAction::Paste(
                "echo 1".into()
            )))
        );
        assert_eq!(
            s.handle(content(), &at(delete.rect())),
            ViewOutcome::Action(ViewAction::Snippets(SnippetsAction::Delete(
                "s1".into()
            )))
        );
        let row = s.row_rect(content(), 2);
        assert_eq!(
            s.hit(content(), row.x + 5.0, row.y + 5.0),
            Some(SnippetsHit::Row(2))
        );
    }

    #[test]
    fn the_list_scrolls_within_its_content() {
        let mut s = state(40);
        let y0 = s.row_rect(content(), 0).y;
        let w = ViewInput::Wheel {
            x: 500.0,
            y: 500.0,
            lines: 2.0,
        };
        assert_eq!(s.handle(content(), &w), ViewOutcome::Redraw);
        assert_eq!(
            s.row_rect(content(), 0).y,
            y0 - 2.0 * (ROW_HEIGHT + ROW_GAP)
        );
        let up = ViewInput::Wheel {
            x: 500.0,
            y: 500.0,
            lines: -100.0,
        };
        s.handle(content(), &up);
        assert_eq!(s.scroll, 0.0);
        let mut few = state(1);
        assert_eq!(few.handle(content(), &w), ViewOutcome::Consumed);
    }
}
