//! Sidebar: the host list, its scroll viewport and the add-host row.
//!
//! Geometry lives here so the painter and the mouse agree by
//! construction: both walk the same `*_rect` methods, and the wheel
//! clamp used by the hit-tests is the one the painter clips with.

use crate::activity_bar;
use crate::geom::Rect;

/// Panel width, in logical pixels.
pub const WIDTH: f32 = 240.0;
/// Header band ("Hosts" + count).
pub const HEADER_HEIGHT: f32 = 36.0;
/// Height of one host row.
pub const ITEM_HEIGHT: f32 = 44.0;
/// Footer band holding the add-host row.
pub const FOOTER_HEIGHT: f32 = 38.0;
/// Height of the notice/error band, when one is showing.
pub const NOTICE_HEIGHT: f32 = 24.0;
/// Horizontal text inset.
pub const PAD_X: f32 = 14.0;
/// Rows advanced by one wheel notch.
pub const WHEEL_ROWS: f32 = 3.0;
/// Left inset of the panel, i.e. the width of the rail beside it.
pub const ORIGIN_X: f32 = activity_bar::WIDTH;

/// One host, as the list draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostItem {
    pub id: String,
    pub name: String,
    /// `user@host:port`, already formatted.
    pub endpoint: String,
}

/// What a press inside the panel landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelHit {
    /// The add-host row at the bottom.
    AddHost,
    /// A host row, by index into [`HostPanel::items`].
    Item(usize),
    /// The panel's own background.
    Background,
}

/// Sidebar state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostPanel {
    pub items: Vec<HostItem>,
    /// Scroll offset in logical pixels, always within
    /// `0..=max_scroll`.
    pub scroll: f32,
    pub hover: Option<usize>,
    /// Whether the pointer is over the add-host row. Kept apart from
    /// `hover` (which is an item index) so the button can highlight
    /// without pretending to be a host.
    pub add_hover: bool,
    pub selected: Option<usize>,
    pub loading: bool,
    /// Transient success message ("Added web-01"), cleared by the caller.
    pub notice: Option<String>,
    pub error: Option<String>,
}

impl HostPanel {
    /// The panel's own box.
    pub fn rect(&self, origin_y: f32, height: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y, WIDTH, height)
    }

    pub fn header_rect(&self, origin_y: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y, WIDTH, HEADER_HEIGHT)
    }

    /// The notice/error band, when there is one to show.
    pub fn notice_rect(&self, origin_y: f32) -> Option<Rect> {
        let message = self.error.as_ref().or(self.notice.as_ref())?;
        debug_assert!(!message.is_empty());
        Some(Rect::new(
            ORIGIN_X,
            origin_y + HEADER_HEIGHT,
            WIDTH,
            NOTICE_HEIGHT,
        ))
    }

    /// The scroll viewport: everything between the header (and its
    /// notice band) and the footer.
    pub fn body_rect(&self, origin_y: f32, height: f32) -> Rect {
        let top = match self.notice_rect(origin_y) {
            Some(notice) => notice.bottom(),
            None => origin_y + HEADER_HEIGHT,
        };
        let bottom = (origin_y + height - FOOTER_HEIGHT).max(top);
        Rect::new(ORIGIN_X, top, WIDTH, bottom - top)
    }

    pub fn footer_rect(&self, origin_y: f32, height: f32) -> Rect {
        let top = (origin_y + height - FOOTER_HEIGHT).max(origin_y);
        Rect::new(ORIGIN_X, top, WIDTH, origin_y + height - top)
    }

    /// The clickable add-host row inside the footer.
    ///
    /// It is inset by the padding so the row reads as a button rather
    /// than a full-bleed band, and the hit-test uses this same rect.
    pub fn add_button_rect(&self, origin_y: f32, height: f32) -> Rect {
        let footer = self.footer_rect(origin_y, height);
        Rect::new(
            footer.x + 8.0,
            footer.y + 5.0,
            footer.width - 16.0,
            (footer.height - 10.0).max(0.0),
        )
    }

    /// A host row, in viewport coordinates (already scrolled).
    pub fn item_rect(&self, origin_y: f32, index: usize) -> Rect {
        Rect::new(
            ORIGIN_X,
            self.rows_top(origin_y) + index as f32 * ITEM_HEIGHT - self.scroll,
            WIDTH,
            ITEM_HEIGHT,
        )
    }

    /// Row 0's unscrolled top edge: below the header and its notice band.
    fn rows_top(&self, origin_y: f32) -> f32 {
        match self.notice_rect(origin_y) {
            Some(notice) => notice.bottom(),
            None => origin_y + HEADER_HEIGHT,
        }
    }

    /// Total height of the rows, ignoring the viewport.
    pub fn content_height(&self) -> f32 {
        self.items.len() as f32 * ITEM_HEIGHT
    }

    /// Largest legal scroll offset for this viewport.
    pub fn max_scroll(&self, origin_y: f32, height: f32) -> f32 {
        (self.content_height() - self.body_rect(origin_y, height).height).max(0.0)
    }

    /// Pull `scroll` back inside the viewport.
    ///
    /// Called after every list or window change: without it a shrink
    /// would leave the list scrolled past its own content, showing an
    /// empty panel.
    pub fn clamp_scroll(&mut self, origin_y: f32, height: f32) {
        let max = self.max_scroll(origin_y, height);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Scroll by `delta` logical pixels, clamped.
    pub fn scroll_by(&mut self, delta: f32, origin_y: f32, height: f32) {
        self.scroll += delta;
        self.clamp_scroll(origin_y, height);
    }

    /// Scroll by whole wheel notches.
    pub fn scroll_rows(&mut self, rows: f32, origin_y: f32, height: f32) {
        self.scroll_by(rows * ITEM_HEIGHT, origin_y, height);
    }

    /// Replace the list, keeping the selected host selected.
    ///
    /// Selection is an index, so a refresh that reorders or inserts
    /// rows would silently move the highlight onto a different host
    /// unless it is re-resolved by id.
    pub fn set_items(&mut self, items: Vec<HostItem>) {
        let keep = self.selected_id().map(str::to_string);
        self.items = items;
        self.selected = keep
            .as_deref()
            .and_then(|id| self.items.iter().position(|item| item.id == id));
        self.hover = None;
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected
            .and_then(|index| self.items.get(index))
            .map(|item| item.id.as_str())
    }

    pub fn selected_item(&self) -> Option<&HostItem> {
        self.selected.and_then(|index| self.items.get(index))
    }

    /// Select a host by id. Returns whether it was found.
    pub fn select_id(&mut self, id: &str) -> bool {
        match self.items.iter().position(|item| item.id == id) {
            Some(index) => {
                self.selected = Some(index);
                true
            }
            None => false,
        }
    }

    /// Which host row is under `(x, y)`, if any.
    fn item_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        let body = self.body_rect(origin_y, height);
        if !body.contains(x, y) {
            return None;
        }
        // Rows are laid out from `top - scroll`; invert that instead of
        // scanning, so the cost is O(1) for a long host list.
        let top = (match self.notice_rect(origin_y) {
            Some(notice) => notice.bottom(),
            None => origin_y + HEADER_HEIGHT,
        }) - self.scroll;
        let offset = y - top;
        if offset < 0.0 {
            return None;
        }
        let index = (offset / ITEM_HEIGHT).floor() as usize;
        (index < self.items.len()).then_some(index)
    }

    /// What is under `(x, y)`.
    pub fn hit_test(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<PanelHit> {
        if !self.rect(origin_y, height).contains(x, y) {
            return None;
        }
        if self.add_button_rect(origin_y, height).contains(x, y) {
            return Some(PanelHit::AddHost);
        }
        if let Some(index) = self.item_at(origin_y, height, x, y) {
            return Some(PanelHit::Item(index));
        }
        Some(PanelHit::Background)
    }

    pub fn set_hover(&mut self, hit: Option<PanelHit>) -> bool {
        let (item, add) = match hit {
            Some(PanelHit::Item(index)) => (Some(index), false),
            Some(PanelHit::AddHost) => (None, true),
            _ => (None, false),
        };
        if item == self.hover && add == self.add_hover {
            return false;
        }
        self.hover = item;
        self.add_hover = add;
        true
    }

    /// Which row the pointer is over, if any.
    ///
    /// Returns the hit rather than an index so the add-host row can be
    /// highlighted too — it is not an item, and reporting `None` for it
    /// would leave the button with no hover feedback.
    pub fn hover_at(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<PanelHit> {
        match self.hit_test(origin_y, height, x, y) {
            Some(PanelHit::Background) | None => None,
            hit => hit,
        }
    }

    /// The header's count label ("3 hosts").
    pub fn count_label(&self) -> String {
        match self.items.len() {
            0 => "empty".to_string(),
            1 => "1 host".to_string(),
            n => format!("{n} hosts"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize) -> Vec<HostItem> {
        (0..n)
            .map(|i| HostItem {
                id: format!("id-{i}"),
                name: format!("host-{i}"),
                endpoint: format!("deploy@host-{i}:2222"),
            })
            .collect()
    }

    fn panel(n: usize) -> HostPanel {
        let mut panel = HostPanel::default();
        panel.set_items(items(n));
        panel
    }

    /// A viewport tall enough that N rows fit without scrolling.
    fn tall() -> (f32, f32) {
        (0.0, 1000.0)
    }

    #[test]
    fn rows_are_hit_at_their_painted_position() {
        let panel = panel(3);
        let (oy, h) = tall();
        for index in 0..3 {
            let rect = panel.item_rect(oy, index);
            let (x, y) = (rect.x + 10.0, rect.y + rect.height / 2.0);
            assert_eq!(panel.hit_test(oy, h, x, y), Some(PanelHit::Item(index)));
        }
    }

    #[test]
    fn the_add_button_is_its_own_target() {
        let panel = panel(2);
        let (oy, h) = tall();
        let button = panel.add_button_rect(oy, h);
        let (x, y) = (
            button.x + button.width / 2.0,
            button.y + button.height / 2.0,
        );
        assert_eq!(panel.hit_test(oy, h, x, y), Some(PanelHit::AddHost));

        // The footer strip beside the button is not the button.
        let beside = Rect::new(panel.footer_rect(oy, h).x, y, 4.0, 1.0);
        assert_eq!(
            panel.hit_test(oy, h, beside.x + 1.0, y),
            Some(PanelHit::Background)
        );
    }

    #[test]
    fn the_panel_claims_nothing_left_of_itself() {
        let panel = panel(3);
        let (oy, h) = tall();
        let row = panel.item_rect(oy, 0);
        assert_eq!(panel.hit_test(oy, h, ORIGIN_X - 1.0, row.y + 1.0), None);
        assert_eq!(panel.hit_test(oy, h, ORIGIN_X + WIDTH, row.y + 1.0), None);
    }

    #[test]
    fn scrolling_moves_the_hit_targets_with_the_pixels() {
        let mut panel = panel(20);
        let (oy, h) = (0.0, 400.0);
        let body = panel.body_rect(oy, h);
        assert_eq!(
            panel.max_scroll(oy, h),
            panel.content_height() - body.height
        );

        panel.scroll_rows(WHEEL_ROWS, oy, h);
        assert_eq!(panel.scroll, WHEEL_ROWS * ITEM_HEIGHT);

        let row = panel.item_rect(oy, 0);
        // Row 0 has scrolled up out of the viewport, so the pixel that
        // used to be row 0 is now row 3.
        let probe = body.y + 2.0;
        assert!(row.bottom() < probe);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + 10.0, probe),
            Some(PanelHit::Item(3))
        );
    }

    #[test]
    fn scroll_is_clamped_to_the_content() {
        let mut panel = panel(3);
        let (oy, h) = tall();
        panel.scroll_rows(100.0, oy, h);
        assert_eq!(panel.scroll, 0.0);

        let (oy, h) = (0.0, 200.0);
        panel.scroll_rows(100.0, oy, h);
        assert_eq!(panel.scroll, panel.max_scroll(oy, h));
    }

    #[test]
    fn losing_hosts_pulls_the_list_back_into_view() {
        let mut panel = panel(20);
        let (oy, h) = (0.0, 400.0);
        panel.scroll_rows(20.0, oy, h);
        let scrolled = panel.scroll;
        assert!(scrolled > 0.0);

        // Every host but two is gone, so there is no longer anything to
        // scroll to — without the clamp the panel would sit empty.
        panel.set_items(items(2));
        panel.clamp_scroll(oy, h);
        assert_eq!(panel.scroll, 0.0);
        assert_eq!(panel.max_scroll(oy, h), 0.0);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + 10.0, panel.item_rect(oy, 0).y + 2.0),
            Some(PanelHit::Item(0))
        );
    }

    #[test]
    fn a_shrinking_window_keeps_the_scroll_legal() {
        let mut panel = panel(20);
        let (oy, h) = (0.0, 400.0);
        panel.scroll_rows(20.0, oy, h);
        assert_eq!(panel.scroll, panel.max_scroll(oy, h));

        // A shorter window means a smaller viewport and therefore a
        // larger maximum, so the offset stays legal — but it must stay
        // inside the new bounds, which is what the clamp guarantees
        // before every paint.
        let short = 200.0;
        panel.clamp_scroll(oy, short);
        let max = panel.max_scroll(oy, short);
        assert!(panel.scroll <= max, "{} > {max}", panel.scroll);
        assert!(panel.scroll > 0.0);

        // And a viewport taller than the content pins the list back to
        // the top instead of leaving it scrolled into empty space.
        panel.clamp_scroll(oy, 4000.0);
        assert_eq!(panel.scroll, 0.0);
    }

    #[test]
    fn refreshing_keeps_the_selected_host_selected() {
        let mut panel = panel(3);
        panel.selected = Some(1);
        assert_eq!(panel.selected_id(), Some("id-1"));

        // A new host is prepended: index 1 is no longer the same host.
        let mut next = items(3);
        next.insert(
            0,
            HostItem {
                id: "new".to_string(),
                name: "new".to_string(),
                endpoint: "root@new".to_string(),
            },
        );
        panel.set_items(next);
        assert_eq!(panel.selected_id(), Some("id-1"));
        assert_eq!(panel.selected, Some(2));
        assert_eq!(panel.hover, None);
    }

    #[test]
    fn a_deleted_selection_is_dropped_rather_than_moved() {
        let mut panel = panel(3);
        panel.selected = Some(2);
        panel.set_items(items(1));
        assert_eq!(panel.selected, None);
        assert_eq!(panel.selected_id(), None);
    }

    #[test]
    fn the_notice_band_pushes_the_rows_down() {
        let mut panel = panel(2);
        let (oy, h) = tall();
        let without = panel.item_rect(oy, 0).y;

        panel.error = Some("'x' is not a valid port".to_string());
        let with = panel.item_rect(oy, 0).y;
        assert_eq!(with - without, NOTICE_HEIGHT);

        let row = panel.item_rect(oy, 0);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + 4.0, row.y + 2.0),
            Some(PanelHit::Item(0))
        );
    }

    #[test]
    fn the_notice_never_overlaps_the_add_row() {
        // A wide range of window heights: the footer is anchored to the
        // bottom and the body must never extend under it.
        for height in [120.0, 200.0, 400.0, 1000.0] {
            let mut panel = panel(30);
            panel.notice = Some("Added web-01".to_string());
            let body = panel.body_rect(0.0, height);
            let footer = panel.footer_rect(0.0, height);
            assert!(body.bottom() <= footer.y, "height {height}");
            assert!(body.height >= 0.0, "height {height}");
        }
    }

    #[test]
    fn count_label_is_pluralised() {
        let mut panel = panel(0);
        assert_eq!(panel.count_label(), "empty");
        panel.set_items(items(1));
        assert_eq!(panel.count_label(), "1 host");
        panel.set_items(items(7));
        assert_eq!(panel.count_label(), "7 hosts");
    }
}
