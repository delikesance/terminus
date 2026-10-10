use crate::geom::Rect;

pub const MENU_WIDTH: f32 = 236.0;
pub const MENU_RADIUS: f32 = 12.0;
pub const MENU_ITEM_RADIUS: f32 = 8.0;
pub const MENU_PAD: f32 = 6.0;
pub const MENU_ITEM_HEIGHT: f32 = 36.0;
pub const MENU_ITEM_PAD_X: f32 = 10.0;
/// 1px rule with 5px margins above and below.
pub const SEPARATOR_HEIGHT: f32 = 11.0;
const WINDOW_MARGIN: f32 = 8.0;
/// Approximate label advance; the ui never measures fonts.
const LABEL_ESTIMATE: f32 = 7.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Default,
    Disabled,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub label: String,
    pub state: EntryState,
    pub separator: bool,
}

impl MenuEntry {
    pub fn item(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: EntryState::Default,
            separator: false,
        }
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            state: EntryState::Default,
            separator: true,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.state = EntryState::Disabled;
        self
    }

    pub fn danger(mut self) -> Self {
        self.state = EntryState::Danger;
        self
    }

    /// Can be hovered / activated.
    pub fn enabled(&self) -> bool {
        !self.separator && self.state != EntryState::Disabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuHit {
    Item(usize),
    /// Inside the menu but not on an enabled row.
    Consume,
    Dismiss,
}

/// How a row is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuVisual {
    Default,
    Hover,
    Disabled,
    Danger,
    DangerHover,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub x: f32,
    pub y: f32,
    pub entries: Vec<MenuEntry>,
    pub hover: Option<usize>,
    width: f32,
}

impl Menu {
    pub fn open(x: f32, y: f32, entries: Vec<MenuEntry>) -> Option<Self> {
        if entries.is_empty() {
            return None;
        }
        let width = entries
            .iter()
            .map(|e| e.label.chars().count() as f32 * LABEL_ESTIMATE + 24.0)
            .fold(MENU_WIDTH, f32::max);
        Some(Self {
            x,
            y,
            entries,
            hover: None,
            width,
        })
    }

    fn entry_height(e: &MenuEntry) -> f32 {
        if e.separator {
            SEPARATOR_HEIGHT
        } else {
            MENU_ITEM_HEIGHT
        }
    }

    /// Height derived from the entries: padding + rows + separators.
    pub fn height(&self) -> f32 {
        MENU_PAD * 2.0 + self.entries.iter().map(Self::entry_height).sum::<f32>()
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height())
    }

    pub fn clamped(mut self, window_width: f32, window_height: f32) -> Self {
        let r = self.rect();
        if self.x + r.width > window_width - WINDOW_MARGIN {
            self.x = window_width - r.width - WINDOW_MARGIN;
        }
        if self.y + r.height > window_height - WINDOW_MARGIN {
            self.y = window_height - r.height - WINDOW_MARGIN;
        }
        self.x = self.x.max(WINDOW_MARGIN);
        self.y = self.y.max(WINDOW_MARGIN);
        self
    }

    fn entry_rect(&self, index: usize) -> Option<Rect> {
        let e = self.entries.get(index)?;
        let y = self.y
            + MENU_PAD
            + self.entries[..index]
                .iter()
                .map(Self::entry_height)
                .sum::<f32>();
        Some(Rect::new(
            self.x + MENU_PAD,
            y,
            self.width - 2.0 * MENU_PAD,
            Self::entry_height(e),
        ))
    }

    /// Row rect; `None` for separators and out-of-range indices.
    pub fn item_rect(&self, index: usize) -> Option<Rect> {
        if self.entries.get(index)?.separator {
            return None;
        }
        self.entry_rect(index)
    }

    /// Full-row rect of a separator entry (the rule sits in its middle).
    pub fn separator_rect(&self, index: usize) -> Option<Rect> {
        if !self.entries.get(index)?.separator {
            return None;
        }
        self.entry_rect(index)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> MenuHit {
        if !self.rect().contains(x, y) {
            return MenuHit::Dismiss;
        }
        for i in 0..self.entries.len() {
            if self.entries[i].enabled()
                && self.item_rect(i).is_some_and(|r| r.contains(x, y))
            {
                return MenuHit::Item(i);
            }
        }
        MenuHit::Consume
    }

    /// Returns whether the hover changed.
    pub fn hover_at(&mut self, x: f32, y: f32) -> bool {
        let next = match self.hit_test(x, y) {
            MenuHit::Item(i) => Some(i),
            _ => None,
        };
        if self.hover == next {
            return false;
        }
        self.hover = next;
        true
    }

    /// Keyboard up/down: moves to the next enabled row, wrapping.
    pub fn move_hover(&mut self, delta: i32) {
        let n = self.entries.len() as i32;
        if n == 0 || delta == 0 {
            return;
        }
        let step = delta.signum();
        let mut i = match self.hover {
            Some(h) => h as i32,
            None if step > 0 => -1,
            None => n,
        };
        for _ in 0..n {
            i = (i + step).rem_euclid(n);
            if self.entries[i as usize].enabled() {
                self.hover = Some(i as usize);
                return;
            }
        }
    }

    /// Whether activating `index` should run its action.
    pub fn activate(&self, index: usize) -> bool {
        self.entries.get(index).is_some_and(MenuEntry::enabled)
    }

    pub fn visual(&self, index: usize) -> MenuVisual {
        let hovered = self.hover == Some(index);
        match (self.entries[index].state, hovered) {
            (EntryState::Disabled, _) => MenuVisual::Disabled,
            (EntryState::Danger, true) => MenuVisual::DangerHover,
            (EntryState::Danger, false) => MenuVisual::Danger,
            (EntryState::Default, true) => MenuVisual::Hover,
            (EntryState::Default, false) => MenuVisual::Default,
        }
    }
}
