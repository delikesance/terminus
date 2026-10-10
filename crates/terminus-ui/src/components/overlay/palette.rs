use crate::geom::Rect;

pub const PALETTE_WIDTH: f32 = 600.0;
pub const PALETTE_RADIUS: f32 = 18.0;
pub const PALETTE_QUERY_HEIGHT: f32 = 62.0;
pub const PALETTE_QUERY_PAD_X: f32 = 20.0;
pub const PALETTE_QUERY_ICON: f32 = 19.0;
pub const PALETTE_LIST_PAD: f32 = 8.0;
pub const PALETTE_ITEM_HEIGHT: f32 = 46.0;
pub const PALETTE_ITEM_GAP: f32 = 2.0;
pub const PALETTE_ITEM_PAD_X: f32 = 12.0;
pub const PALETTE_ITEM_RADIUS: f32 = 10.0;
/// First group header: 8px above, 4px below a 16px line.
pub const PALETTE_HEADER_FIRST: f32 = 28.0;
/// Later group headers: 12px above, 4px below.
pub const PALETTE_HEADER: f32 = 32.0;
/// Empty-result line: 14px padding around a 21px line.
pub const PALETTE_EMPTY_HEIGHT: f32 = 49.0;
pub const PALETTE_FOOTER_HEIGHT: f32 = 42.0;
pub const PALETTE_FOOTER_GAP: f32 = 18.0;
pub const PALETTE_HINTS: [&str; 3] = ["Enter to run", "Arrows to move", "Esc to close"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteItem {
    pub label: String,
    pub hint: String,
}

impl PaletteItem {
    pub fn new(label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hint: hint.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteGroup {
    pub title: String,
    pub items: Vec<PaletteItem>,
}

impl PaletteGroup {
    pub fn new(title: impl Into<String>, items: Vec<PaletteItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteChoice {
    /// Flat index across all groups.
    Item(usize),
    AddServer(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKey {
    Escape,
    Enter,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    Close,
    Run,
    Move(i32),
}

pub fn palette_key(key: PaletteKey) -> PaletteAction {
    match key {
        PaletteKey::Escape => PaletteAction::Close,
        PaletteKey::Enter => PaletteAction::Run,
        PaletteKey::Up => PaletteAction::Move(-1),
        PaletteKey::Down => PaletteAction::Move(1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaletteRow {
    Header { rect: Rect, group: usize },
    Item { rect: Rect, index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteHit {
    Item(usize),
    /// The empty-result "Add server" line.
    AddServer,
    Inside,
    Outside,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaletteLayout {
    pub panel: Rect,
    pub query: Rect,
    pub rows: Vec<PaletteRow>,
    pub empty: Option<Rect>,
    pub footer: Rect,
}

impl PaletteLayout {
    pub fn hit_test(&self, x: f32, y: f32) -> PaletteHit {
        if !self.panel.contains(x, y) {
            return PaletteHit::Outside;
        }
        for r in &self.rows {
            if let PaletteRow::Item { rect, index } = r {
                if rect.contains(x, y) {
                    return PaletteHit::Item(*index);
                }
            }
        }
        if self.empty.is_some_and(|r| r.contains(x, y)) {
            return PaletteHit::AddServer;
        }
        PaletteHit::Inside
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub query: String,
    pub groups: Vec<PaletteGroup>,
    /// Flat index of the highlighted item.
    pub selected: usize,
    /// Query placeholder; `None` uses the default "Search servers and commands".
    pub placeholder: Option<String>,
}

impl Palette {
    pub fn new(query: impl Into<String>, groups: Vec<PaletteGroup>) -> Self {
        Self {
            query: query.into(),
            groups,
            selected: 0,
            placeholder: None,
        }
    }

    pub fn item_count(&self) -> usize {
        self.groups.iter().map(|g| g.items.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.item_count() == 0
    }

    pub fn add_server_label(&self) -> String {
        format!("Add server \u{201c}{}\u{201d}", self.query)
    }

    pub fn move_selection(&mut self, delta: i32) {
        let n = self.item_count() as i32;
        if n == 0 {
            return;
        }
        self.selected = (self.selected as i32 + delta).rem_euclid(n) as usize;
    }

    /// What Enter does now.
    pub fn choice(&self) -> Option<PaletteChoice> {
        if self.is_empty() {
            let q = self.query.trim();
            return (!q.is_empty()).then(|| PaletteChoice::AddServer(q.to_string()));
        }
        Some(PaletteChoice::Item(
            self.selected.min(self.item_count() - 1),
        ))
    }

    /// Panel near the top, horizontally centred in `window`.
    pub fn layout(&self, window: (f32, f32)) -> PaletteLayout {
        let x = ((window.0 - PALETTE_WIDTH) / 2.0).round();
        let y = (window.1 * 0.14).round().max(24.0);
        self.layout_at(x, y)
    }

    pub fn layout_at(&self, x: f32, y: f32) -> PaletteLayout {
        let query = Rect::new(x, y, PALETTE_WIDTH, PALETTE_QUERY_HEIGHT);
        let mut cy = query.bottom() + PALETTE_LIST_PAD;
        let inner_x = x + PALETTE_LIST_PAD;
        let inner_w = PALETTE_WIDTH - 2.0 * PALETTE_LIST_PAD;
        let mut rows = Vec::new();
        let mut empty = None;
        if self.is_empty() {
            empty = Some(Rect::new(inner_x, cy, inner_w, PALETTE_EMPTY_HEIGHT));
            cy += PALETTE_EMPTY_HEIGHT;
        } else {
            let mut index = 0;
            let mut first = true;
            for (g, group) in self.groups.iter().enumerate() {
                if group.items.is_empty() {
                    continue;
                }
                if !first {
                    cy += PALETTE_ITEM_GAP;
                }
                let h = if first {
                    PALETTE_HEADER_FIRST
                } else {
                    PALETTE_HEADER
                };
                rows.push(PaletteRow::Header {
                    rect: Rect::new(inner_x, cy, inner_w, h),
                    group: g,
                });
                cy += h;
                first = false;
                for _ in &group.items {
                    cy += PALETTE_ITEM_GAP;
                    rows.push(PaletteRow::Item {
                        rect: Rect::new(inner_x, cy, inner_w, PALETTE_ITEM_HEIGHT),
                        index,
                    });
                    cy += PALETTE_ITEM_HEIGHT;
                    index += 1;
                }
            }
        }
        cy += PALETTE_LIST_PAD;
        let footer = Rect::new(x, cy, PALETTE_WIDTH, PALETTE_FOOTER_HEIGHT);
        PaletteLayout {
            panel: Rect::new(x, y, PALETTE_WIDTH, footer.bottom() - y),
            query,
            rows,
            empty,
            footer,
        }
    }
}
