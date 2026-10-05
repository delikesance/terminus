//! Workspace header geometry: Identity header (title + mono address) with
//! Navigation view tabs on its bottom border.
//!
//! The header has three faces (`App.dc.html`):
//! * machine — name + address, tabs Terminal · Files · Tunnels · Snippets ·
//!   History (Tunnels carries the running-tunnel badge);
//! * settings — "Settings", tabs SSH keys · Sync · Appearance · Updates;
//! * home — "Where to?", no tabs.

use super::workspace::{SettingsPage, WorkspaceView};
use crate::components::identity::{self, HeaderRects, HEADER_PAD_LEFT};
use crate::components::navigation::{view_tabs, TabSize};
use crate::geom::Rect;

/// Header `padding-left` (28px) — where the title text starts.
pub const PAD_LEFT: f32 = 28.0;
pub const HOME_TITLE: &str = "Where to?";
pub const SETTINGS_TITLE: &str = "Settings";

/// One header tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderTab {
    pub view: WorkspaceView,
    pub label: &'static str,
    /// Badge text ("1"), empty when none.
    pub badge: String,
}

/// The tabs the header shows for `view`.
pub fn tabs_for(view: WorkspaceView, tunnel_badge: u32) -> Vec<HeaderTab> {
    if view.is_settings() {
        return SettingsPage::ALL
            .iter()
            .map(|p| HeaderTab {
                view: WorkspaceView::Settings(*p),
                label: p.label(),
                badge: String::new(),
            })
            .collect();
    }
    if view == WorkspaceView::Home {
        return Vec::new();
    }
    WorkspaceView::MACHINE_TABS
        .iter()
        .map(|v| HeaderTab {
            view: *v,
            label: v.label(),
            badge: if *v == WorkspaceView::Tunnels && tunnel_badge > 0 {
                tunnel_badge.to_string()
            } else {
                String::new()
            },
        })
        .collect()
}

/// Header boxes: the Identity rects plus one rect per tab.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderGeom {
    pub ident: HeaderRects,
    pub tabs: Vec<Rect>,
}

/// Lay the header out inside `header` (the 96px band). `block_w` is the
/// measured width of the widest of title and address; `sizes` the measured
/// tabs.
pub fn layout(
    header: &Rect,
    has_address: bool,
    block_w: f32,
    sizes: &[TabSize],
) -> HeaderGeom {
    let x = header.x + PAD_LEFT - HEADER_PAD_LEFT;
    let y = header.bottom() - identity::header_height(has_address);
    let tabs_w: f32 = sizes.iter().map(|s| view_tabs::width(*s)).sum::<f32>()
        + view_tabs::GAP * sizes.len().saturating_sub(1) as f32;
    let mut ident =
        identity::header_rects(x, y, header.right() - x, has_address, block_w, tabs_w);
    // The bottom border runs the whole card width.
    ident.border = Rect::new(header.x, ident.border.y, header.width, ident.border.height);
    let tabs = view_tabs::layout(ident.tabs.x, ident.tabs.y, sizes);
    HeaderGeom { ident, tabs }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(n: usize) -> Vec<TabSize> {
        (0..n)
            .map(|_| TabSize {
                label_w: 60.0,
                badge_w: 0.0,
            })
            .collect()
    }

    #[test]
    fn machine_views_get_the_five_tabs_with_a_tunnel_badge() {
        let tabs = tabs_for(WorkspaceView::Files, 2);
        let labels: Vec<_> = tabs.iter().map(|t| t.label).collect();
        assert_eq!(
            labels,
            ["Terminal", "Files", "Tunnels", "Snippets", "History"]
        );
        assert_eq!(tabs[2].badge, "2");
        assert!(tabs_for(WorkspaceView::Terminal, 0)[2].badge.is_empty());
    }

    #[test]
    fn settings_has_its_own_tabs_and_home_none() {
        let tabs = tabs_for(WorkspaceView::Settings(SettingsPage::Sync), 3);
        let labels: Vec<_> = tabs.iter().map(|t| t.label).collect();
        assert_eq!(labels, ["SSH keys", "Sync", "Appearance", "Updates"]);
        assert!(tabs.iter().all(|t| t.badge.is_empty()));
        assert!(tabs_for(WorkspaceView::Home, 1).is_empty());
    }

    #[test]
    fn title_sits_28px_in_and_tabs_rest_on_the_border() {
        let header = Rect::new(260.0, 8.0, 1172.0, 96.0);
        let g = layout(&header, true, 150.0, &sizes(5));
        assert_eq!(g.ident.title.x, 288.0);
        assert_eq!(g.ident.border.bottom(), header.bottom());
        assert_eq!(g.ident.border.x, header.x);
        assert_eq!(g.ident.border.width, header.width);
        assert_eq!(g.tabs.len(), 5);
        assert_eq!(g.tabs[0].x, 288.0 + 150.0 + identity::HEADER_GAP);
        for t in &g.tabs {
            assert_eq!(t.bottom(), g.ident.border.y);
        }
        // Address line under the title, 14px above the border.
        let a = g.ident.address.expect("machine header has an address");
        assert!(a.y > g.ident.title.y);
        assert!(a.bottom() <= g.ident.border.y - 14.0 + 0.01);
    }

    #[test]
    fn a_title_only_header_is_shorter_but_still_bottom_aligned() {
        let header = Rect::new(260.0, 8.0, 900.0, 96.0);
        let g = layout(&header, false, 120.0, &sizes(4));
        assert!(g.ident.address.is_none());
        assert_eq!(g.ident.border.bottom(), header.bottom());
        assert!(g.ident.title.y > header.y);
    }
}
