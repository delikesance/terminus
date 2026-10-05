//! Home ("Where to?") (stub): a big search field that opens the command
//! palette, and "Add a server". Recent machines come with the next wave.

use super::{estimate_label, primary_button, ViewAction, ViewInput, ViewOutcome};
use crate::geom::Rect;

pub const SEARCH_PLACEHOLDER: &str = "Server name, address or user@host";
pub const RECENT: &str = "Recent";
pub const ADD: &str = "Add a server";
/// Centred column width and its distance from the header.
pub const COLUMN_WIDTH: f32 = 760.0;
pub const COLUMN_TOP: f32 = 72.0;
pub const SEARCH_HEIGHT: f32 = 56.0;
pub const SEARCH_RADIUS: f32 = 14.0;
pub const SEARCH_SIZE: f32 = 17.0;
pub const SEARCH_ICON: f32 = 18.0;
pub const GAP: f32 = 28.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeAction {
    /// Open the command palette on its host list.
    Search,
    /// Open the add-server flow.
    AddServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeHit {
    Search,
    Add,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HomeState {
    pub hover: Option<HomeHit>,
    /// Measured width of [`ADD`] (0 = estimate).
    pub add_label_w: f32,
}

impl HomeState {
    pub fn column(&self, content: Rect) -> Rect {
        let w = COLUMN_WIDTH.min((content.width - 56.0).max(0.0));
        Rect::new(
            content.x + (content.width - w) * 0.5,
            content.y + COLUMN_TOP,
            w,
            (content.height - COLUMN_TOP).max(0.0),
        )
    }

    pub fn search_rect(&self, content: Rect) -> Rect {
        let c = self.column(content);
        Rect::new(c.x, c.y, c.width, SEARCH_HEIGHT)
    }

    pub fn add_rect(&self, content: Rect) -> Rect {
        let s = self.search_rect(content);
        let w = if self.add_label_w > 0.0 {
            self.add_label_w
        } else {
            estimate_label(ADD)
        };
        primary_button((s.x, s.bottom() + GAP), w)
    }

    pub fn hit(&self, content: Rect, x: f32, y: f32) -> Option<HomeHit> {
        if self.search_rect(content).contains(x, y) {
            Some(HomeHit::Search)
        } else if self.add_rect(content).contains(x, y) {
            Some(HomeHit::Add)
        } else {
            None
        }
    }

    pub fn is_clickable(&self, content: Rect, x: f32, y: f32) -> bool {
        self.hit(content, x, y).is_some()
    }

    pub fn handle(&mut self, content: Rect, input: &ViewInput) -> ViewOutcome {
        match *input {
            ViewInput::Press { x, y, .. } => match self.hit(content, x, y) {
                Some(HomeHit::Search) => {
                    ViewOutcome::Action(ViewAction::Home(HomeAction::Search))
                }
                Some(HomeHit::Add) => {
                    ViewOutcome::Action(ViewAction::Home(HomeAction::AddServer))
                }
                None => ViewOutcome::Consumed,
            },
            ViewInput::Move { x, y, .. } => {
                let hover = self.hit(content, x, y);
                if hover != self.hover {
                    self.hover = hover;
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

    #[test]
    fn the_column_is_centred_and_the_search_opens_the_palette() {
        let mut s = HomeState::default();
        let c = s.column(content());
        assert_eq!(c.width, 760.0);
        assert_eq!(c.x + c.width / 2.0, content().x + content().width / 2.0);
        assert_eq!(c.y, content().y + 72.0);
        let r = s.search_rect(content());
        let out = s.handle(
            content(),
            &ViewInput::Press {
                x: r.x + 10.0,
                y: r.y + 10.0,
                double: false,
            },
        );
        assert_eq!(
            out,
            ViewOutcome::Action(ViewAction::Home(HomeAction::Search))
        );
        let a = s.add_rect(content());
        assert!(a.y >= r.bottom() + GAP);
        assert_eq!(s.hit(content(), a.x + 1.0, a.y + 1.0), Some(HomeHit::Add));
    }
}
