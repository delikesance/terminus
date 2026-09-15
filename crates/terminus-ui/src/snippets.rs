//! Command snippets drawer.

use crate::geom::Rect;
use crate::sidebar::{ORIGIN_X, PAD_X, TITLE_HEIGHT, WIDTH};

/// Height of one snippet card.
pub const ITEM_HEIGHT: f32 = 92.0;
/// Gap between cards (`space-y-2`).
pub const ITEM_GAP: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetItem {
    pub id: String,
    pub name: String,
    pub cmd: String,
    pub desc: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnippetsPanel {
    pub items: Vec<SnippetItem>,
    pub scroll: f32,
    pub hover: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetHit {
    Item(usize),
    Background,
}

impl SnippetsPanel {
    pub fn title_rect(&self, origin_y: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y, WIDTH, TITLE_HEIGHT)
    }

    pub fn body_rect(&self, origin_y: f32, height: f32) -> Rect {
        let top = origin_y + TITLE_HEIGHT;
        Rect::new(ORIGIN_X, top, WIDTH, (origin_y + height - top).max(0.0))
    }

    pub fn item_rect(&self, origin_y: f32, index: usize) -> Rect {
        let body = self.body_rect(origin_y, 10_000.0);
        Rect::new(
            ORIGIN_X + PAD_X,
            body.y + 12.0 + index as f32 * (ITEM_HEIGHT + ITEM_GAP) - self.scroll,
            WIDTH - 2.0 * PAD_X,
            ITEM_HEIGHT,
        )
    }

    pub fn content_height(&self) -> f32 {
        if self.items.is_empty() {
            return 0.0;
        }
        12.0
            + self.items.len() as f32 * ITEM_HEIGHT
            + (self.items.len().saturating_sub(1) as f32) * ITEM_GAP
            + 12.0
    }

    pub fn hit_test(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<SnippetHit> {
        let panel = Rect::new(ORIGIN_X, origin_y, WIDTH, height);
        if !panel.contains(x, y) {
            return None;
        }
        for index in 0..self.items.len() {
            if self.item_rect(origin_y, index).contains(x, y) {
                return Some(SnippetHit::Item(index));
            }
        }
        Some(SnippetHit::Background)
    }

    pub fn set_hover(&mut self, hit: Option<SnippetHit>) -> bool {
        let next = match hit {
            Some(SnippetHit::Item(i)) => Some(i),
            _ => None,
        };
        if next == self.hover {
            return false;
        }
        self.hover = next;
        true
    }

    /// Seed demo snippets matching the HTML mock when the store is empty.
    pub fn with_defaults() -> Self {
        Self {
            items: vec![
                SnippetItem {
                    id: "1".into(),
                    name: "Docker Full Cleanup".into(),
                    cmd: "docker system prune -a --volumes -f".into(),
                    desc: "Remove all unused containers, networks and images.".into(),
                },
                SnippetItem {
                    id: "2".into(),
                    name: "System Update & Clean".into(),
                    cmd: "sudo apt update && sudo apt upgrade -y && sudo apt autoremove -y"
                        .into(),
                    desc: "Fully update system packages and clean caches.".into(),
                },
                SnippetItem {
                    id: "3".into(),
                    name: "Check Disk I/O & Load".into(),
                    cmd: "htop && iostat -xz 1".into(),
                    desc: "Monitor live resource usage and disk bottlenecks.".into(),
                },
                SnippetItem {
                    id: "4".into(),
                    name: "Nginx Reload Config".into(),
                    cmd: "sudo nginx -t && sudo systemctl reload nginx".into(),
                    desc: "Test configuration syntax and gracefully reload nginx.".into(),
                },
            ],
            scroll: 0.0,
            hover: None,
        }
    }
}
