//! Content-sized blocks shared by painters and pointer hit-tests.
//!
//! Components expose their intrinsic size. Columns stack them, rows distribute
//! remaining width to flexible children, and padding belongs to the container.
//! Layout is rebuilt from current state and viewport, never stored as offsets.

use crate::Rect;

#[derive(Debug, Clone, PartialEq)]
pub enum Block<K> {
    Component {
        key: K,
        width: Option<f32>,
        height: f32,
    },
    Column {
        gap: f32,
        children: Vec<Self>,
    },
    Row {
        gap: f32,
        children: Vec<Self>,
    },
    Padding {
        inset: f32,
        child: Box<Self>,
    },
    Group {
        key: K,
        child: Box<Self>,
    },
}

impl<K: Copy> Block<K> {
    pub fn component(key: K, height: f32) -> Self {
        Self::Component {
            key,
            width: None,
            height,
        }
    }

    pub fn fixed(key: K, width: f32, height: f32) -> Self {
        Self::Component {
            key,
            width: Some(width),
            height,
        }
    }

    pub fn column(gap: f32, children: Vec<Self>) -> Self {
        Self::Column { gap, children }
    }

    pub fn row(gap: f32, children: Vec<Self>) -> Self {
        Self::Row { gap, children }
    }

    pub fn padded(self, inset: f32) -> Self {
        Self::Padding {
            inset,
            child: Box::new(self),
        }
    }

    pub fn named(self, key: K) -> Self {
        Self::Group {
            key,
            child: Box::new(self),
        }
    }

    pub fn height(&self) -> f32 {
        match self {
            Self::Component { height, .. } => *height,
            Self::Column { gap, children } => {
                children.iter().map(Self::height).sum::<f32>()
                    + gap * children.len().saturating_sub(1) as f32
            }
            Self::Row { children, .. } => {
                children.iter().map(Self::height).fold(0.0, f32::max)
            }
            Self::Padding { inset, child } => child.height() + 2.0 * inset,
            Self::Group { child, .. } => child.height(),
        }
    }

    fn width(&self) -> Option<f32> {
        match self {
            Self::Component { width, .. } => *width,
            Self::Padding { inset, child } => child.width().map(|w| w + 2.0 * inset),
            Self::Column { .. } | Self::Row { .. } => None,
            Self::Group { child, .. } => child.width(),
        }
    }

    pub fn layout(&self, bounds: Rect) -> Vec<(K, Rect)> {
        let mut result = Vec::new();
        self.place(bounds, &mut result);
        result
    }

    fn place(&self, bounds: Rect, result: &mut Vec<(K, Rect)>) {
        match self {
            Self::Component { key, height, .. } => {
                result.push((*key, Rect::new(bounds.x, bounds.y, bounds.width, *height)));
            }
            Self::Column { gap, children } => {
                let mut y = bounds.y;
                for child in children {
                    child.place(
                        Rect::new(bounds.x, y, bounds.width, child.height()),
                        result,
                    );
                    y += child.height() + gap;
                }
            }
            Self::Row { gap, children } => {
                let fixed = children.iter().filter_map(Self::width).sum::<f32>();
                let flexible = children.iter().filter(|c| c.width().is_none()).count();
                let remaining = (bounds.width
                    - fixed
                    - gap * children.len().saturating_sub(1) as f32)
                    .max(0.0);
                let share = remaining / flexible.max(1) as f32;
                let mut x = bounds.x;
                for child in children {
                    let width = child.width().unwrap_or(share);
                    child.place(Rect::new(x, bounds.y, width, child.height()), result);
                    x += width + gap;
                }
            }
            Self::Padding { inset, child } => child.place(
                Rect::new(
                    bounds.x + inset,
                    bounds.y + inset,
                    (bounds.width - 2.0 * inset).max(0.0),
                    child.height(),
                ),
                result,
            ),
            Self::Group { key, child } => {
                result.push((*key, bounds));
                child.place(bounds, result);
            }
        }
    }
}

/// A text component whose lines are measured before blocks are placed.
/// The same lines are painted, so wrapping cannot silently change its height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlock {
    pub lines: Vec<String>,
}

impl TextBlock {
    pub fn measure(text: &str, width: f32, mut measure: impl FnMut(&str) -> f32) -> Self {
        let mut lines = Vec::new();
        for paragraph in text.lines() {
            let mut line = String::new();
            for word in paragraph.split_whitespace() {
                let candidate = format!("{line}{word}");
                if measure(&candidate) <= width {
                    line = candidate;
                    line.push(' ');
                    continue;
                }
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                if measure(word) <= width {
                    line.push_str(word);
                    line.push(' ');
                    continue;
                }
                // Split oversized tokens at Unicode character boundaries.
                for ch in word.chars() {
                    let candidate = format!("{line}{ch}");
                    if !line.is_empty() && measure(&candidate) > width {
                        lines.push(std::mem::take(&mut line));
                    }
                    line.push(ch);
                }
                // A trailing separator is stripped before storing a line.
                line.push(' ');
            }
            lines.push(line.trim_end().to_string());
        }
        for line in &mut lines {
            *line = line.trim_end().to_string();
        }
        Self { lines }
    }

    pub fn height(&self, font_size: f32, line_gap: f32) -> f32 {
        self.lines.len() as f32 * font_size
            + self.lines.len().saturating_sub(1) as f32 * line_gap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_blocks_propagate_content_height_and_container_padding() {
        let tree = Block::column(
            8.0,
            vec![
                Block::component("header", 28.0),
                Block::column(
                    6.0,
                    vec![
                        Block::component("caption", 14.0),
                        Block::component("input", 32.0),
                    ],
                )
                .named("field"),
                Block::component("actions", 32.0),
            ],
        )
        .padded(24.0);
        let rects = tree.layout(Rect::new(10.0, 20.0, 400.0, tree.height()));
        let get = |key| rects.iter().find(|(k, _)| *k == key).unwrap().1;
        assert_eq!(tree.height(), 176.0);
        assert_eq!(get("header"), Rect::new(34.0, 44.0, 352.0, 28.0));
        assert_eq!(get("input").bottom(), get("field").bottom());
        assert_eq!(get("actions").y, get("field").bottom() + 8.0);
    }

    #[test]
    fn rows_reflow_flexible_children_when_viewport_changes() {
        let row = Block::row(
            8.0,
            vec![
                Block::fixed("back", 72.0, 32.0),
                Block::component("space", 0.0),
                Block::fixed("next", 76.0, 32.0),
            ],
        );
        for width in [400.0, 272.0] {
            let rects = row.layout(Rect::new(24.0, 0.0, width, row.height()));
            assert_eq!(rects[0].1.x, 24.0);
            assert_eq!(rects[2].1.right(), 24.0 + width);
            assert!(rects[0].1.right() + 8.0 <= rects[2].1.x);
        }
    }

    #[test]
    fn text_measurement_respects_font_width_newlines_and_long_unicode_tokens() {
        let measure = |s: &str| s.chars().count() as f32 * 7.0;
        let text = TextBlock::measure("one two three\nétéétéété", 49.0, measure);
        assert_eq!(text.lines, ["one two", "three", "étéétéé", "té"]);
        assert!(text.lines.iter().all(|line| measure(line) <= 49.0));
        assert_eq!(text.height(11.0, 3.0), 53.0);
        assert!(
            TextBlock::measure("one two three", 98.0, measure)
                .lines
                .len()
                < text.lines.len()
        );
        assert!(TextBlock::measure("", 98.0, measure).lines.is_empty());
    }
}
