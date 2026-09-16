//! Dev-time overlap detection for chrome geometry.
//!
//! Painters and hit-tests share [`crate::geom::Rect`]. In debug builds we
//! assert that interactive boxes never share area, so a layout regression
//! fails tests instead of silently stacking controls.

use crate::geom::Rect;

/// Two axis-aligned rects with positive area that share interior pixels.
///
/// Touching edges (half-open convention) do **not** count as overlap —
/// stacked rows that meet at `a.bottom() == b.y` are fine.
pub fn rects_overlap(a: Rect, b: Rect) -> bool {
    if a.width <= 0.0 || a.height <= 0.0 || b.width <= 0.0 || b.height <= 0.0 {
        return false;
    }
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// One overlapping pair found in a labeled set of rects.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlap {
    pub a: &'static str,
    pub b: &'static str,
    pub ra: Rect,
    pub rb: Rect,
}

/// Return every unordered pair of labeled rects that overlap.
pub fn find_overlaps(labeled: &[(&'static str, Rect)]) -> Vec<Overlap> {
    let mut out = Vec::new();
    for i in 0..labeled.len() {
        for j in (i + 1)..labeled.len() {
            let (na, ra) = labeled[i];
            let (nb, rb) = labeled[j];
            if rects_overlap(ra, rb) {
                out.push(Overlap {
                    a: na,
                    b: nb,
                    ra,
                    rb,
                });
            }
        }
    }
    out
}

/// Panic in debug builds when any pair overlaps.
#[cfg(debug_assertions)]
pub fn assert_no_overlaps(labeled: &[(&'static str, Rect)], context: &str) {
    let hits = find_overlaps(labeled);
    if hits.is_empty() {
        return;
    }
    let mut msg = format!("overlap detected ({context}):\n");
    for hit in &hits {
        msg.push_str(&format!(
            "  {} {:?} ∩ {} {:?}\n",
            hit.a, hit.ra, hit.b, hit.rb
        ));
    }
    panic!("{msg}");
}

#[cfg(not(debug_assertions))]
#[inline]
pub fn assert_no_overlaps(_labeled: &[(&'static str, Rect)], _context: &str) {}

/// Assert host-panel interactive boxes do not overlap (debug only).
///
/// Checks:
/// 1. Same-row controls (chevron / badge / add / close)
/// 2. Search vs CTA
/// 3. Vertical stack: CTA then each visible card (no area intersection)
/// 4. Inline new-group form vs Hosts section action when drafting
pub fn assert_panel_no_overlaps(
    panel: &crate::sidebar::HostPanel,
    origin_y: f32,
    height: f32,
) {
    let cta = panel.add_button_rect(origin_y, height);
    assert_no_overlaps(
        &[("search", panel.search_rect(origin_y)), ("cta", cta)],
        "search vs cta",
    );

    let mut stack: Vec<(&'static str, Rect)> = Vec::new();
    if cta.width > 0.0 && cta.height > 0.0 {
        stack.push(("cta", cta));
    }

    for index in panel.visible_row_indices() {
        let mut row_controls: Vec<(&'static str, Rect)> = Vec::new();
        if let Some(ch) = panel.host_chevron_rect(origin_y, index) {
            row_controls.push(("chevron", ch));
        }
        if let Some(add) = panel.host_add_session_rect(origin_y, index) {
            row_controls.push(("add", add));
        }
        if matches!(panel.rows.get(index), Some(crate::sidebar::Row::Session(_))) {
            if let Some(close) = panel.session_close_rect(origin_y, index) {
                row_controls.push(("close", close));
            }
        }
        if panel.rows.get(index).and_then(|r| r.host()).is_some() {
            let card = panel.card_rect(origin_y, index);
            let inset = panel.host_leading_inset(index);
            let badge = Rect::new(
                card.x + inset,
                card.y + (card.height - crate::sidebar::HOST_BADGE_TILE) * 0.5,
                crate::sidebar::HOST_BADGE_TILE,
                crate::sidebar::HOST_BADGE_TILE,
            );
            row_controls.push(("badge", badge));
        }
        assert_no_overlaps(&row_controls, &format!("row {index} controls"));

        // Cards / section slots participate in the Y stack (not section
        // labels alone — the Hosts action is checked separately).
        match panel.rows.get(index) {
            Some(crate::sidebar::Row::Host(_))
            | Some(crate::sidebar::Row::Session(_))
            | Some(crate::sidebar::Row::Group { .. }) => {
                stack.push(("card", panel.card_rect(origin_y, index)));
            }
            Some(crate::sidebar::Row::Section(label))
                if label.eq_ignore_ascii_case("Hosts") =>
            {
                let action = panel.new_group_button_rect(origin_y, height);
                if action.width > 0.0 {
                    // Action must stay inside the section row band.
                    let section = panel.item_rect(origin_y, index);
                    assert!(
                        action.y >= section.y - 0.5
                            && action.bottom()
                                <= section.y + crate::sidebar::SECTION_HEIGHT + 0.5,
                        "new group action escapes Hosts header"
                    );
                }
                if let Some(form) = panel.new_group_form_rect(origin_y) {
                    stack.push(("new_group_form", form));
                }
            }
            _ => {}
        }
    }

    // Consecutive stack entries must not share area (gaps / touching OK).
    for pair in stack.windows(2) {
        assert_no_overlaps(&[pair[0], pair[1]], "vertical stack");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os_icons::HostStatus;
    use crate::sidebar::{Badge, HostItem, HostPanel, Row, SessionItem};

    #[test]
    fn touching_edges_are_not_overlaps() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 10.0, 10.0);
        let c = Rect::new(0.0, 10.0, 10.0, 10.0);
        assert!(!rects_overlap(a, b));
        assert!(!rects_overlap(a, c));
    }

    #[test]
    fn interior_intersection_is_overlap() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!(rects_overlap(a, b));
        let hits = find_overlaps(&[("a", a), ("b", b)]);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn empty_rects_never_overlap() {
        let a = Rect::new(0.0, 0.0, 0.0, 10.0);
        let b = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(!rects_overlap(a, b));
    }

    #[test]
    fn host_chevron_clears_badge_when_sessions_exist() {
        let mut panel = HostPanel::default();
        panel.set_rows(vec![
            Row::Host(HostItem {
                id: "local".into(),
                name: "This computer".into(),
                endpoint: "nixos".into(),
                badge: Badge::Local,
                stored: false,
                os_id: None,
                status: HostStatus::Active,
                nested: false,
                session_count: 1,
            }),
            Row::Session(SessionItem {
                tab_index: 0,
                host_id: "local".into(),
                title: "shell".into(),
                active: true,
                closable: false,
            }),
        ]);
        assert_panel_no_overlaps(&panel, 0.0, 1000.0);
    }

    #[test]
    fn cta_does_not_overlap_first_host_card() {
        let mut panel = HostPanel::default();
        panel.set_rows(vec![
            Row::Section("Local".into()),
            Row::Host(HostItem {
                id: "local".into(),
                name: "This computer".into(),
                endpoint: "nixos".into(),
                badge: Badge::Local,
                stored: false,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
        ]);
        let oy = 0.0;
        let h = 1000.0;
        let cta = panel.add_button_rect(oy, h);
        let card = panel.card_rect(oy, 1);
        assert!(!rects_overlap(cta, card), "cta={cta:?} card={card:?}");
        assert_panel_no_overlaps(&panel, oy, h);
    }
}
