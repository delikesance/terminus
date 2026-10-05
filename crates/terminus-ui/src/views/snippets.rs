//! Snippets view: filter field + "New snippet", then one list card per
//! snippet (title, mono command, tag, Paste and Run).
//!
//! State, geometry, hit-testing and the filter semantics live here; the
//! painter (`rioterm::renderer::views::snippets`) walks the same rects.
//! All geometry is derived from the `content` rect passed in.

use crate::components::button::{ButtonKind, ButtonSize, ButtonSpec};
use crate::components::input::{search_layout, SearchKind, SearchLayout, SEARCH_HEIGHT};
use crate::components::list::{
    card_hit, CardHit, CardSpec, CARD_ACTION_GAP, CARD_HEIGHT,
};
use crate::geom::Rect;
use crate::snippets::SnippetItem;

/// Padding around the view (design: 28 px).
pub const PAD: f32 = 28.0;
/// Gap between the search field and the New snippet button, and between cards.
pub const GAP: f32 = 10.0;
/// Extra space under the toolbar row (design: margin-bottom 6).
pub const TOOLBAR_MARGIN: f32 = 6.0;
/// Toolbar row height (the Large button).
pub const TOOLBAR_HEIGHT: f32 = 44.0;

/// What the app must do after an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnippetsAction {
    /// Type the command into the active session without Enter.
    PasteToSession(String),
    /// Type the command into the active session and press Enter.
    RunInSession(String),
    /// Open the add-snippet dialog.
    OpenNewSnippet,
    Edit(String),
    Delete(String),
}

/// Measured label widths (set by the painter; sane defaults before that).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelWidths {
    pub new_snippet: f32,
    pub paste: f32,
    pub run: f32,
}

impl Default for LabelWidths {
    fn default() -> Self {
        Self {
            new_snippet: 88.0,
            paste: 34.0,
            run: 24.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetsHit {
    Search,
    NewButton,
    /// Indices are into [`SnippetsView::visible`].
    Paste(usize),
    Run(usize),
    Delete(usize),
    Card(usize),
    Background,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnippetsView {
    pub items: Vec<SnippetItem>,
    pub filter: String,
    pub filter_focused: bool,
    pub scroll: f32,
    pub hover: Option<SnippetsHit>,
    pub labels: LabelWidths,
}

/// Tag shown on a card: the snippet's description.
pub fn tag_of(item: &SnippetItem) -> &str {
    item.desc.trim()
}

fn matches(item: &SnippetItem, needle: &str) -> bool {
    let n = needle.trim().to_lowercase();
    if n.is_empty() {
        return true;
    }
    [item.name.as_str(), item.cmd.as_str(), item.desc.as_str()]
        .iter()
        .any(|f| f.to_lowercase().contains(&n))
}

impl SnippetsView {
    pub fn new(items: Vec<SnippetItem>) -> Self {
        Self {
            items,
            ..Self::default()
        }
    }

    /// Items passing the filter (title, command, tag; case-insensitive).
    pub fn visible(&self) -> Vec<&SnippetItem> {
        self.items
            .iter()
            .filter(|i| matches(i, &self.filter))
            .collect()
    }

    pub fn new_button(&self, content: Rect) -> ButtonSpec {
        let w = ButtonSpec::label(
            (0.0, 0.0),
            ButtonKind::Primary,
            ButtonSize::Large,
            self.labels.new_snippet,
            false,
        )
        .width();
        ButtonSpec::label(
            (content.right() - PAD - w, content.y + PAD),
            ButtonKind::Primary,
            ButtonSize::Large,
            self.labels.new_snippet,
            false,
        )
    }

    pub fn search(&self, content: Rect) -> SearchLayout {
        let btn = self.new_button(content);
        let width = (btn.rect().x - GAP - (content.x + PAD)).max(0.0);
        let y = content.y + PAD + (TOOLBAR_HEIGHT - SEARCH_HEIGHT) / 2.0;
        search_layout((content.x + PAD, y), width, SearchKind::Search, 0.0)
    }

    /// The scrolling list area (below the toolbar).
    pub fn list_area(&self, content: Rect) -> Rect {
        let top = content.y + PAD + TOOLBAR_HEIGHT + TOOLBAR_MARGIN + GAP;
        Rect::new(
            content.x + PAD,
            top,
            (content.width - 2.0 * PAD).max(0.0),
            (content.bottom() - top).max(0.0),
        )
    }

    pub fn content_height(&self) -> f32 {
        let n = self.visible().len() as f32;
        if n == 0.0 {
            0.0
        } else {
            n * CARD_HEIGHT + (n - 1.0) * GAP + PAD
        }
    }

    pub fn max_scroll(&self, content: Rect) -> f32 {
        (self.content_height() - self.list_area(content).height).max(0.0)
    }

    pub fn wheel(&mut self, content: Rect, delta_y: f32) -> bool {
        let next = (self.scroll - delta_y).clamp(0.0, self.max_scroll(content));
        let changed = (next - self.scroll).abs() > f32::EPSILON;
        self.scroll = next;
        changed
    }

    /// Delete button column width (icon-only Medium quiet button + gap).
    fn trash_slot() -> f32 {
        ButtonSize::Medium.height() + GAP
    }

    /// Card rect of the `i`-th visible snippet (scrolled).
    pub fn card_rect(&self, content: Rect, i: usize) -> Rect {
        let area = self.list_area(content);
        Rect::new(
            area.x,
            area.y + i as f32 * (CARD_HEIGHT + GAP) - self.scroll,
            (area.width - Self::trash_slot()).max(0.0),
            CARD_HEIGHT,
        )
    }

    pub fn delete_rect(&self, content: Rect, i: usize) -> Rect {
        let card = self.card_rect(content, i);
        let s = ButtonSize::Medium.height();
        Rect::new(card.right() + GAP, card.y + (card.height - s) / 2.0, s, s)
    }

    pub fn card_spec<'a>(&self, widths: &'a [f32; 2]) -> CardSpec<'a> {
        CardSpec {
            has_dot: false,
            meta_width: 0.0,
            action_widths: widths,
        }
    }

    /// Button widths of the Paste and Run slots.
    pub fn action_widths(&self) -> [f32; 2] {
        let w = |label_w: f32| {
            ButtonSpec::label(
                (0.0, 0.0),
                ButtonKind::Secondary,
                ButtonSize::Medium,
                label_w,
                false,
            )
            .width()
        };
        [w(self.labels.paste), w(self.labels.run)]
    }

    pub fn hit_test(&self, content: Rect, x: f32, y: f32) -> Option<SnippetsHit> {
        if !content.contains(x, y) {
            return None;
        }
        if self.new_button(content).rect().contains(x, y) {
            return Some(SnippetsHit::NewButton);
        }
        if self.search(content).box_rect.contains(x, y) {
            return Some(SnippetsHit::Search);
        }
        let area = self.list_area(content);
        if !area.contains(x, y) {
            return Some(SnippetsHit::Background);
        }
        let widths = self.action_widths();
        let spec = self.card_spec(&widths);
        for i in 0..self.visible().len() {
            if self.delete_rect(content, i).contains(x, y) {
                return Some(SnippetsHit::Delete(i));
            }
            match card_hit(self.card_rect(content, i), &spec, x, y) {
                Some(CardHit::Action(0)) => return Some(SnippetsHit::Paste(i)),
                Some(CardHit::Action(_)) => return Some(SnippetsHit::Run(i)),
                Some(CardHit::Body) => return Some(SnippetsHit::Card(i)),
                None => {}
            }
        }
        Some(SnippetsHit::Background)
    }

    /// Returns true when the hover changed (repaint needed).
    pub fn hover_at(&mut self, content: Rect, x: f32, y: f32) -> bool {
        let next = self.hit_test(content, x, y);
        let changed = next != self.hover;
        self.hover = next;
        changed
    }

    /// Primary-button press.
    pub fn press(&mut self, content: Rect, x: f32, y: f32) -> Option<SnippetsAction> {
        let hit = self.hit_test(content, x, y);
        self.filter_focused = hit == Some(SnippetsHit::Search);
        let visible = self.visible();
        let id_cmd = |i: usize| visible.get(i).map(|s| (s.id.clone(), s.cmd.clone()));
        match hit? {
            SnippetsHit::NewButton => Some(SnippetsAction::OpenNewSnippet),
            SnippetsHit::Paste(i) => {
                id_cmd(i).map(|(_, c)| SnippetsAction::PasteToSession(c))
            }
            SnippetsHit::Run(i) => {
                id_cmd(i).map(|(_, c)| SnippetsAction::RunInSession(c))
            }
            SnippetsHit::Delete(i) => id_cmd(i).map(|(id, _)| SnippetsAction::Delete(id)),
            _ => None,
        }
    }

    /// Secondary-button press: edit the card under the pointer.
    pub fn secondary_press(
        &mut self,
        content: Rect,
        x: f32,
        y: f32,
    ) -> Option<SnippetsAction> {
        match self.hit_test(content, x, y)? {
            SnippetsHit::Card(i) | SnippetsHit::Paste(i) | SnippetsHit::Run(i) => self
                .visible()
                .get(i)
                .map(|s| SnippetsAction::Edit(s.id.clone())),
            _ => None,
        }
    }

    /// Typed text goes to the filter when focused. Returns true if consumed.
    pub fn type_text(&mut self, text: &str) -> bool {
        if !self.filter_focused {
            return false;
        }
        self.filter.extend(text.chars().filter(|c| !c.is_control()));
        self.scroll = 0.0;
        true
    }

    pub fn backspace(&mut self) -> bool {
        if !self.filter_focused {
            return false;
        }
        self.filter.pop();
        self.scroll = 0.0;
        true
    }

    /// Escape clears the filter, then unfocuses. Returns true if consumed.
    pub fn escape(&mut self) -> bool {
        if !self.filter.is_empty() {
            self.filter.clear();
            return true;
        }
        std::mem::take(&mut self.filter_focused)
    }

    /// Gap between the Paste and Run buttons (re-exported for painters).
    pub const ACTION_GAP: f32 = CARD_ACTION_GAP;
}

/// Seeded data for the preview harness (mirrors the design mock).
pub fn seed() -> Vec<SnippetItem> {
    let mk = |id: &str, name: &str, cmd: &str, tag: &str| SnippetItem {
        id: id.into(),
        name: name.into(),
        cmd: cmd.into(),
        desc: tag.into(),
    };
    vec![
        mk(
            "1",
            "Restart nginx",
            "sudo systemctl restart nginx",
            "nginx",
        ),
        mk(
            "2",
            "Follow compose logs",
            "docker compose logs -f --tail 100",
            "docker",
        ),
        mk("3", "Disk usage", "df -h", ""),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Rect {
        Rect::new(260.0, 96.0, 1180.0, 804.0)
    }

    fn view() -> SnippetsView {
        SnippetsView::new(seed())
    }

    #[test]
    fn filter_matches_title_command_and_tag() {
        let mut v = view();
        v.filter = "NGINX".into();
        assert_eq!(v.visible().len(), 1);
        v.filter = "tail 100".into();
        assert_eq!(v.visible()[0].id, "2");
        v.filter = "disk".into();
        assert_eq!(v.visible()[0].id, "3");
        v.filter = "zzz".into();
        assert!(v.visible().is_empty());
        v.filter = "  ".into();
        assert_eq!(v.visible().len(), 3);
    }

    #[test]
    fn toolbar_fills_width_and_button_is_right_aligned() {
        let v = view();
        let c = content();
        let b = v.new_button(c).rect();
        assert!((b.right() - (c.right() - PAD)).abs() < 0.01);
        let s = v.search(c).box_rect;
        assert!((s.right() + GAP - b.x).abs() < 0.01);
        assert_eq!(s.x, c.x + PAD);
    }

    #[test]
    fn press_actions() {
        let mut v = view();
        let c = content();
        let nb = v.new_button(c).rect();
        assert_eq!(
            v.press(c, nb.x + 5.0, nb.y + 5.0),
            Some(SnippetsAction::OpenNewSnippet)
        );
        let w = v.action_widths();
        let card = v.card_rect(c, 1);
        let spec = v.card_spec(&w);
        let l = crate::components::list::card_layout(card, &spec);
        let p = l.actions[0].unwrap();
        let r = l.actions[1].unwrap();
        assert_eq!(
            v.press(c, p.x + 2.0, p.y + 2.0),
            Some(SnippetsAction::PasteToSession(
                "docker compose logs -f --tail 100".into()
            ))
        );
        assert_eq!(
            v.press(c, r.x + 2.0, r.y + 2.0),
            Some(SnippetsAction::RunInSession(
                "docker compose logs -f --tail 100".into()
            ))
        );
        let d = v.delete_rect(c, 1);
        assert_eq!(
            v.press(c, d.x + 2.0, d.y + 2.0),
            Some(SnippetsAction::Delete("2".into()))
        );
        assert_eq!(
            v.secondary_press(c, card.x + 4.0, card.y + 4.0),
            Some(SnippetsAction::Edit("2".into()))
        );
    }

    #[test]
    fn hit_indices_follow_the_filter() {
        let mut v = view();
        v.filter = "df".into();
        let c = content();
        let card = v.card_rect(c, 0);
        let w = v.action_widths();
        let l = crate::components::list::card_layout(card, &v.card_spec(&w));
        let r = l.actions[1].unwrap();
        assert_eq!(
            v.press(c, r.x + 2.0, r.y + 2.0),
            Some(SnippetsAction::RunInSession("df -h".into()))
        );
    }

    #[test]
    fn search_focus_and_typing() {
        let mut v = view();
        let c = content();
        assert!(!v.type_text("x"));
        let s = v.search(c).box_rect;
        assert_eq!(v.press(c, s.x + 20.0, s.y + 10.0), None);
        assert!(v.filter_focused);
        assert!(v.type_text("ng"));
        assert!(v.type_text("\n"));
        assert_eq!(v.filter, "ng");
        assert_eq!(v.visible().len(), 1);
        assert!(v.backspace());
        assert_eq!(v.filter, "n");
        assert!(v.escape());
        assert!(v.filter.is_empty() && v.filter_focused);
        assert!(v.escape());
        assert!(!v.filter_focused);
    }

    #[test]
    fn scroll_is_clamped() {
        let mut v = SnippetsView::new(
            (0..30)
                .map(|i| SnippetItem {
                    id: i.to_string(),
                    name: format!("s{i}"),
                    cmd: "x".into(),
                    desc: String::new(),
                })
                .collect(),
        );
        let c = content();
        assert!(!v.wheel(c, 50.0));
        assert!(v.wheel(c, -100.0));
        assert_eq!(v.scroll, 100.0);
        v.wheel(c, -1.0e6);
        assert_eq!(v.scroll, v.max_scroll(c));
        // Rows scrolled above the list area are not clickable.
        let none = v.hit_test(c, c.x + PAD + 5.0, c.y + 20.0);
        assert_ne!(none, Some(SnippetsHit::Card(0)));
    }

    #[test]
    fn outside_content_is_not_hit() {
        let v = view();
        assert_eq!(v.hit_test(content(), 10.0, 10.0), None);
    }
}
