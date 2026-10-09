//! Overlays: dialog shell, wizard stepper, context menu and command palette.
//!
//! Pure geometry, state and key semantics in logical pixels; the painter in
//! `frontends/rioterm/src/renderer/components/overlay.rs` walks the same
//! rect functions the pointer hit-tests. Specs come from the design board
//! `COverlays` (violet ink).
//!
//! The older [`crate::context_menu::ContextMenu`] stays as is (host/SFTP
//! actions); [`Menu`] here is the design-system menu with disabled rows and
//! separators and keeps its semantics: a press returns Item / Consume /
//! Dismiss, height derives from the entries, `clamped` keeps it on screen.

use crate::geom::Rect;

// ---------------------------------------------------------------- dialog

pub const DIALOG_WIDTH: f32 = 440.0;
pub const DIALOG_PAD: f32 = 30.0;
/// Vertical gap between title block, option row and actions.
pub const DIALOG_GAP: f32 = 18.0;
pub const TITLE_LINE: f32 = 28.0;
pub const TITLE_BODY_GAP: f32 = 8.0;
/// 14px body at line-height 1.5 (rounded to a whole pixel run of 21).
pub const BODY_LINE: f32 = 21.0;
pub const OPTION_HEIGHT: f32 = 20.0;
pub const ACTION_HEIGHT: f32 = 44.0;
pub const ACTION_GAP: f32 = 10.0;
/// Horizontal padding inside an action button (text is measured by the painter).
pub const ACTION_PAD_X: f32 = 20.0;
/// Scrim colour, rgba(6,5,10,0.64).
pub const SCRIM: [f32; 4] = [6.0 / 255.0, 5.0 / 255.0, 10.0 / 255.0, 0.64];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Confirm,
    Destructive,
    WithOption,
}

impl DialogKind {
    /// Destructive dialogs open with Cancel focused so a stray Enter is safe.
    pub fn default_focus(self) -> DialogFocus {
        match self {
            DialogKind::Destructive => DialogFocus::Cancel,
            _ => DialogFocus::Confirm,
        }
    }

    pub fn has_option(self) -> bool {
        self == DialogKind::WithOption
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogFocus {
    Cancel,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKey {
    Escape,
    Enter,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogOutcome {
    Cancel,
    Confirm,
    Focus(DialogFocus),
}

/// Esc always cancels; Enter activates the focused button; Tab swaps focus.
pub fn dialog_key(key: DialogKey, focus: DialogFocus) -> DialogOutcome {
    match key {
        DialogKey::Escape => DialogOutcome::Cancel,
        DialogKey::Enter => match focus {
            DialogFocus::Cancel => DialogOutcome::Cancel,
            DialogFocus::Confirm => DialogOutcome::Confirm,
        },
        DialogKey::Tab => DialogOutcome::Focus(match focus {
            DialogFocus::Cancel => DialogFocus::Confirm,
            DialogFocus::Confirm => DialogFocus::Cancel,
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogHit {
    Confirm,
    Cancel,
    Option,
    Inside,
    Scrim,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogLayout {
    pub scrim: Rect,
    pub dialog: Rect,
    pub title: Rect,
    pub body: Rect,
    pub option: Option<Rect>,
    pub cancel: Rect,
    pub confirm: Rect,
}

impl DialogLayout {
    pub fn hit_test(&self, x: f32, y: f32) -> DialogHit {
        if self.confirm.contains(x, y) {
            DialogHit::Confirm
        } else if self.cancel.contains(x, y) {
            DialogHit::Cancel
        } else if self.option.is_some_and(|r| r.contains(x, y)) {
            DialogHit::Option
        } else if self.dialog.contains(x, y) {
            DialogHit::Inside
        } else {
            DialogHit::Scrim
        }
    }
}

/// Width of an action button for a label of `text_width` px.
pub fn action_width(text_width: f32) -> f32 {
    text_width + 2.0 * ACTION_PAD_X
}

/// Dialog centred in `window`. `cancel_w`/`confirm_w` are button widths
/// (see [`action_width`]); `body_lines` is the wrapped line count.
pub fn dialog_layout(
    window: (f32, f32),
    kind: DialogKind,
    body_lines: usize,
    cancel_w: f32,
    confirm_w: f32,
) -> DialogLayout {
    let body_h = body_lines.max(1) as f32 * BODY_LINE;
    let option_h = if kind.has_option() {
        DIALOG_GAP + OPTION_HEIGHT
    } else {
        0.0
    };
    let height = DIALOG_PAD * 2.0
        + TITLE_LINE
        + TITLE_BODY_GAP
        + body_h
        + option_h
        + DIALOG_GAP
        + ACTION_HEIGHT;
    let x = ((window.0 - DIALOG_WIDTH) / 2.0).round();
    let y = ((window.1 - height) / 2.0).round();
    dialog_layout_at(x, y, kind, body_lines, cancel_w, confirm_w, window)
}

/// Same as [`dialog_layout`] with an explicit top-left (gallery use).
pub fn dialog_layout_at(
    x: f32,
    y: f32,
    kind: DialogKind,
    body_lines: usize,
    cancel_w: f32,
    confirm_w: f32,
    window: (f32, f32),
) -> DialogLayout {
    let body_h = body_lines.max(1) as f32 * BODY_LINE;
    let option_h = if kind.has_option() {
        DIALOG_GAP + OPTION_HEIGHT
    } else {
        0.0
    };
    let height = DIALOG_PAD * 2.0
        + TITLE_LINE
        + TITLE_BODY_GAP
        + body_h
        + option_h
        + DIALOG_GAP
        + ACTION_HEIGHT;
    let dialog = Rect::new(x, y, DIALOG_WIDTH, height);
    let inner_x = x + DIALOG_PAD;
    let inner_w = DIALOG_WIDTH - 2.0 * DIALOG_PAD;
    let title = Rect::new(inner_x, y + DIALOG_PAD, inner_w, TITLE_LINE);
    let body = Rect::new(inner_x, title.bottom() + TITLE_BODY_GAP, inner_w, body_h);
    let option = kind
        .has_option()
        .then(|| Rect::new(inner_x, body.bottom() + DIALOG_GAP, inner_w, OPTION_HEIGHT));
    let (cancel, confirm) = action_row(
        &dialog,
        DIALOG_PAD,
        ACTION_HEIGHT,
        ACTION_GAP,
        cancel_w,
        confirm_w,
    );
    DialogLayout {
        scrim: Rect::new(0.0, 0.0, window.0, window.1),
        dialog,
        title,
        body,
        option,
        cancel,
        confirm,
    }
}

/// `(cancel, confirm)` buttons right-aligned on the bottom edge of `dialog`.
pub fn action_row(
    dialog: &Rect,
    pad: f32,
    height: f32,
    gap: f32,
    cancel_w: f32,
    confirm_w: f32,
) -> (Rect, Rect) {
    let y = dialog.bottom() - pad - height;
    let confirm = Rect::new(dialog.right() - pad - confirm_w, y, confirm_w, height);
    let cancel = Rect::new(confirm.x - gap - cancel_w, y, cancel_w, height);
    (cancel, confirm)
}

/// Greedy word wrap using a caller supplied width measure. A word wider
/// than `max_width` gets its own line.
pub fn wrap_text(
    text: &str,
    max_width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
            continue;
        }
        let candidate = format!("{cur} {word}");
        if measure(&candidate) <= max_width {
            cur = candidate;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    lines.push(cur);
    lines
}

// --------------------------------------------------------------- stepper

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

// ------------------------------------------------------------------ menu

pub const MENU_WIDTH: f32 = 236.0;
pub const MENU_RADIUS: f32 = 12.0;
pub const MENU_ITEM_RADIUS: f32 = 8.0;
pub const MENU_PAD: f32 = 6.0;
pub const MENU_ITEM_HEIGHT: f32 = 36.0;
pub const MENU_ITEM_PAD_X: f32 = 10.0;
/// 1px rule with 5px margins above and below.
pub const SEPARATOR_HEIGHT: f32 = 11.0;
const WINDOW_MARGIN: f32 = 8.0;

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
}

impl Menu {
    pub fn open(x: f32, y: f32, entries: Vec<MenuEntry>) -> Option<Self> {
        if entries.is_empty() {
            return None;
        }
        Some(Self {
            x,
            y,
            entries,
            hover: None,
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
        Rect::new(self.x, self.y, MENU_WIDTH, self.height())
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
            MENU_WIDTH - 2.0 * MENU_PAD,
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

// --------------------------------------------------------------- palette

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

#[cfg(test)]
mod tests {
    #[test]
    fn action_row_right_aligns_confirm_and_gaps_cancel() {
        let dialog = Rect::new(100.0, 50.0, 400.0, 300.0);
        let (cancel, confirm) = action_row(&dialog, 30.0, 44.0, 10.0, 70.0, 90.0);
        assert_eq!(confirm, Rect::new(380.0, 276.0, 90.0, 44.0));
        assert_eq!(cancel, Rect::new(300.0, 276.0, 70.0, 44.0));
    }
    use super::*;

    fn fake_measure(s: &str) -> f32 {
        s.chars().count() as f32 * 7.0
    }

    // ---- dialog ----
    #[test]
    fn dialog_is_centred_in_window() {
        let l = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 2, 90.0, 80.0);
        assert_eq!(l.dialog.width, DIALOG_WIDTH);
        let cx = l.dialog.x + l.dialog.width / 2.0;
        let cy = l.dialog.y + l.dialog.height / 2.0;
        assert!((cx - 500.0).abs() < 0.5 && (cy - 350.0).abs() < 0.5);
        assert_eq!(l.scrim, Rect::new(0.0, 0.0, 1000.0, 700.0));
    }

    #[test]
    fn dialog_actions_are_right_aligned_inside_padding() {
        let l = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 1, 90.0, 80.0);
        assert!((l.confirm.right() - (l.dialog.right() - DIALOG_PAD)).abs() < 0.01);
        assert!((l.cancel.right() + ACTION_GAP - l.confirm.x).abs() < 0.01);
        assert_eq!(l.confirm.height, ACTION_HEIGHT);
        assert!((l.confirm.bottom() - (l.dialog.bottom() - DIALOG_PAD)).abs() < 0.01);
        assert!(l.cancel.x < l.confirm.x);
    }

    #[test]
    fn dialog_grows_with_body_lines_and_option_row() {
        let a = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 1, 90.0, 80.0);
        let b = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 3, 90.0, 80.0);
        assert!((b.dialog.height - a.dialog.height - 2.0 * BODY_LINE).abs() < 0.01);
        let c = dialog_layout((1000.0, 700.0), DialogKind::WithOption, 1, 90.0, 80.0);
        assert!(c.option.is_some() && a.option.is_none());
        assert!(c.dialog.height > a.dialog.height);
        let o = c.option.unwrap();
        assert!(o.bottom() <= c.confirm.y && o.y >= c.body.bottom());
    }

    #[test]
    fn dialog_hit_distinguishes_buttons_option_inside_and_scrim() {
        let l = dialog_layout((1000.0, 700.0), DialogKind::WithOption, 1, 90.0, 80.0);
        let mid = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);
        let (x, y) = mid(l.confirm);
        assert_eq!(l.hit_test(x, y), DialogHit::Confirm);
        let (x, y) = mid(l.cancel);
        assert_eq!(l.hit_test(x, y), DialogHit::Cancel);
        let (x, y) = mid(l.option.unwrap());
        assert_eq!(l.hit_test(x, y), DialogHit::Option);
        assert_eq!(
            l.hit_test(l.dialog.x + 2.0, l.dialog.y + 2.0),
            DialogHit::Inside
        );
        assert_eq!(l.hit_test(1.0, 1.0), DialogHit::Scrim);
    }

    #[test]
    fn escape_always_cancels() {
        for kind in [
            DialogKind::Confirm,
            DialogKind::Destructive,
            DialogKind::WithOption,
        ] {
            for focus in [DialogFocus::Cancel, DialogFocus::Confirm] {
                assert_eq!(
                    dialog_key(DialogKey::Escape, focus),
                    DialogOutcome::Cancel,
                    "{kind:?}"
                );
            }
        }
    }

    #[test]
    fn enter_activates_the_focused_button_and_destructive_defaults_to_cancel() {
        assert_eq!(
            dialog_key(DialogKey::Enter, DialogFocus::Confirm),
            DialogOutcome::Confirm
        );
        assert_eq!(
            dialog_key(DialogKey::Enter, DialogFocus::Cancel),
            DialogOutcome::Cancel
        );
        assert_eq!(DialogKind::Confirm.default_focus(), DialogFocus::Confirm);
        assert_eq!(DialogKind::WithOption.default_focus(), DialogFocus::Confirm);
        assert_eq!(DialogKind::Destructive.default_focus(), DialogFocus::Cancel);
        assert_eq!(
            dialog_key(DialogKey::Tab, DialogFocus::Cancel),
            DialogOutcome::Focus(DialogFocus::Confirm)
        );
        assert_eq!(
            dialog_key(DialogKey::Tab, DialogFocus::Confirm),
            DialogOutcome::Focus(DialogFocus::Cancel)
        );
    }

    #[test]
    fn wrap_breaks_on_words_and_respects_width() {
        let lines = wrap_text("alpha beta gamma delta", 7.0 * 11.0, fake_measure);
        assert_eq!(lines, vec!["alpha beta", "gamma delta"]);
        assert_eq!(wrap_text("", 100.0, fake_measure), vec![String::new()]);
        // An overlong word stays on its own line rather than looping.
        assert_eq!(
            wrap_text("supercalifragilistic x", 20.0, fake_measure).len(),
            2
        );
    }

    // ---- stepper ----
    #[test]
    fn stepper_fills_up_to_current_step_and_marks_current_label() {
        let segs = stepper_segments(Rect::new(10.0, 20.0, 380.0, 30.0), 2);
        assert_eq!(segs.len(), 3);
        assert_eq!(
            segs.iter().map(|s| s.filled).collect::<Vec<_>>(),
            [true, true, false]
        );
        assert_eq!(
            segs.iter().map(|s| s.current).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(
            segs.iter().map(|s| s.label).collect::<Vec<_>>(),
            ["Address", "Sign in", "Organise"]
        );
    }

    #[test]
    fn stepper_bars_are_equal_3px_with_8px_gaps_spanning_width() {
        let r = Rect::new(10.0, 20.0, 380.0, 30.0);
        let segs = stepper_segments(r, 1);
        assert!(segs
            .iter()
            .all(|s| s.bar.height == STEPPER_BAR_HEIGHT && s.bar.y == 20.0));
        assert!((segs[1].bar.x - segs[0].bar.right() - STEPPER_GAP).abs() < 0.01);
        assert!((segs[2].bar.right() - r.right()).abs() < 0.01);
        assert!((segs[0].bar.width - segs[2].bar.width).abs() < 0.01);
        assert!(segs[0].label_rect.y > segs[0].bar.bottom());
        assert_eq!(
            STEPPER_HEIGHT,
            STEPPER_BAR_HEIGHT + STEPPER_LABEL_GAP + STEPPER_LABEL_HEIGHT
        );
    }

    #[test]
    fn stepper_step_is_clamped() {
        assert!(stepper_segments(Rect::new(0.0, 0.0, 300.0, 30.0), 0)
            .iter()
            .all(|s| !s.filled && !s.current));
        assert!(stepper_segments(Rect::new(0.0, 0.0, 300.0, 30.0), 9)
            .iter()
            .all(|s| s.filled));
    }

    // ---- menu ----
    fn sample() -> Menu {
        Menu::open(
            10.0,
            10.0,
            vec![
                MenuEntry::item("New session"),
                MenuEntry::item("Open files"),
                MenuEntry::item("Copy SSH command"),
                MenuEntry::item("Open in the other pane").disabled(),
                MenuEntry::separator(),
                MenuEntry::item("Edit server"),
                MenuEntry::item("Delete").danger(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn menu_height_is_derived_from_entries() {
        let m = sample();
        let expected = MENU_PAD * 2.0 + 6.0 * MENU_ITEM_HEIGHT + SEPARATOR_HEIGHT;
        assert!((m.height() - expected).abs() < 0.01);
        assert_eq!(m.rect().width, MENU_WIDTH);
        assert_eq!(Menu::open(0.0, 0.0, vec![]), None);
    }

    #[test]
    fn menu_rows_stack_inside_padding() {
        let m = sample();
        let first = m.item_rect(0).unwrap();
        assert_eq!((first.x, first.y), (10.0 + MENU_PAD, 10.0 + MENU_PAD));
        assert_eq!(first.height, MENU_ITEM_HEIGHT);
        let after_sep = m.item_rect(5).unwrap();
        let before = m.item_rect(3).unwrap();
        assert!((after_sep.y - before.bottom() - SEPARATOR_HEIGHT).abs() < 0.01);
        assert!(m.separator_rect(4).is_some() && m.separator_rect(0).is_none());
        assert!(m.item_rect(4).is_none(), "separators are not item rows");
    }

    #[test]
    fn menu_is_clamped_inside_window() {
        let m = Menu::open(
            990.0,
            690.0,
            vec![MenuEntry::item("A"), MenuEntry::item("B")],
        )
        .unwrap()
        .clamped(1000.0, 700.0);
        let r = m.rect();
        assert!(r.right() <= 1000.0 && r.bottom() <= 700.0 && r.x >= 0.0 && r.y >= 0.0);
    }

    #[test]
    fn menu_hit_test_and_hover_ignore_disabled_and_separators() {
        let mut m = sample();
        let c = |r: Rect| (r.x + 4.0, r.y + 4.0);
        let (x, y) = c(m.item_rect(1).unwrap());
        assert_eq!(m.hit_test(x, y), MenuHit::Item(1));
        let (x, y) = c(m.item_rect(3).unwrap());
        assert_eq!(
            m.hit_test(x, y),
            MenuHit::Consume,
            "disabled rows swallow the click"
        );
        let sep = m.separator_rect(4).unwrap();
        assert_eq!(m.hit_test(sep.x + 2.0, sep.y + 2.0), MenuHit::Consume);
        assert_eq!(m.hit_test(900.0, 900.0), MenuHit::Dismiss);
        assert!(!m.hover_at(x, y) && m.hover.is_none());
        let (x, y) = c(m.item_rect(1).unwrap());
        assert!(m.hover_at(x, y));
        assert_eq!(m.hover, Some(1));
    }

    #[test]
    fn menu_keyboard_skips_disabled_and_separators_and_wraps() {
        let mut m = sample();
        m.move_hover(1);
        assert_eq!(m.hover, Some(0));
        m.move_hover(1);
        m.move_hover(1);
        assert_eq!(m.hover, Some(2));
        m.move_hover(1);
        assert_eq!(m.hover, Some(5), "skips disabled item and separator");
        m.move_hover(1);
        assert_eq!(m.hover, Some(6));
        m.move_hover(1);
        assert_eq!(m.hover, Some(0), "wraps");
        m.move_hover(-1);
        assert_eq!(m.hover, Some(6), "wraps back");
        m.move_hover(-1);
        m.move_hover(-1);
        assert_eq!(m.hover, Some(2), "up skips too");
    }

    #[test]
    fn menu_activation_refuses_disabled() {
        let m = sample();
        assert!(m.activate(0));
        assert!(!m.activate(3));
        assert!(!m.activate(4));
        assert!(!m.activate(99));
    }

    #[test]
    fn menu_visual_state_follows_design() {
        let mut m = sample();
        m.hover = Some(0);
        assert_eq!(m.visual(0), MenuVisual::Hover);
        assert_eq!(m.visual(1), MenuVisual::Default);
        assert_eq!(m.visual(3), MenuVisual::Disabled);
        assert_eq!(m.visual(6), MenuVisual::Danger);
        m.hover = Some(6);
        assert_eq!(m.visual(6), MenuVisual::DangerHover);
    }

    // ---- palette ----
    fn palette() -> Palette {
        Palette::new(
            "spl",
            vec![PaletteGroup::new(
                "Commands",
                vec![
                    PaletteItem::new("Split Right", "Terminal"),
                    PaletteItem::new("Split Down", "Terminal"),
                ],
            )],
        )
    }

    #[test]
    fn palette_layout_is_centred_and_sized() {
        let p = palette();
        let l = p.layout((1200.0, 800.0));
        assert_eq!(l.panel.width, PALETTE_WIDTH);
        assert!((l.panel.x + l.panel.width / 2.0 - 600.0).abs() < 0.5);
        assert_eq!(l.query.height, PALETTE_QUERY_HEIGHT);
        assert_eq!(l.rows.len(), 3, "one header + two items");
        assert!(matches!(l.rows[0], PaletteRow::Header { .. }));
        let items: Vec<_> = l
            .rows
            .iter()
            .filter_map(|r| match r {
                PaletteRow::Item { rect, index } => Some((*rect, *index)),
                _ => None,
            })
            .collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0.height, PALETTE_ITEM_HEIGHT);
        assert_eq!(items[1].1, 1);
        assert!((items[1].0.y - items[0].0.bottom() - PALETTE_ITEM_GAP).abs() < 0.01);
        assert!(l.footer.y >= items[1].0.bottom());
        assert!((l.footer.bottom() - l.panel.bottom()).abs() < 0.5);
        assert!(l.empty.is_none());
    }

    #[test]
    fn palette_selection_wraps_and_resolves_choice() {
        let mut p = palette();
        assert_eq!(p.selected, 0);
        p.move_selection(1);
        assert_eq!(p.selected, 1);
        p.move_selection(1);
        assert_eq!(p.selected, 0);
        p.move_selection(-1);
        assert_eq!(p.selected, 1);
        assert_eq!(p.choice(), Some(PaletteChoice::Item(1)));
    }

    #[test]
    fn palette_empty_result_offers_add_server() {
        let p = Palette::new("splt", vec![]);
        assert!(p.is_empty());
        assert_eq!(p.add_server_label(), "Add server \u{201c}splt\u{201d}");
        assert_eq!(p.choice(), Some(PaletteChoice::AddServer("splt".into())));
        let l = p.layout((1200.0, 800.0));
        assert!(l.empty.is_some());
        assert!(l.rows.is_empty());
        let e = l.empty.unwrap();
        assert_eq!(l.hit_test(e.x + 3.0, e.y + 3.0), PaletteHit::AddServer);
        let blank = Palette::new("", vec![]);
        assert_eq!(blank.choice(), None, "nothing to add for an empty query");
    }

    #[test]
    fn palette_hit_test() {
        let p = palette();
        let l = p.layout((1200.0, 800.0));
        let item = l
            .rows
            .iter()
            .find_map(|r| match r {
                PaletteRow::Item { rect, index: 1 } => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert_eq!(l.hit_test(item.x + 5.0, item.y + 5.0), PaletteHit::Item(1));
        assert_eq!(
            l.hit_test(l.query.x + 5.0, l.query.y + 5.0),
            PaletteHit::Inside
        );
        assert_eq!(l.hit_test(1.0, 1.0), PaletteHit::Outside);
    }

    #[test]
    fn palette_keys() {
        assert_eq!(palette_key(PaletteKey::Escape), PaletteAction::Close);
        assert_eq!(palette_key(PaletteKey::Enter), PaletteAction::Run);
        assert_eq!(palette_key(PaletteKey::Down), PaletteAction::Move(1));
        assert_eq!(palette_key(PaletteKey::Up), PaletteAction::Move(-1));
    }

    #[test]
    fn palette_footer_hints() {
        assert_eq!(
            PALETTE_HINTS,
            ["Enter to run", "Arrows to move", "Esc to close"]
        );
    }
}
