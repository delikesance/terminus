//! Navigation: server rows, section/group headers, drop target, view tabs
//! and session pills (violet-ink design board `CNavigation`).
//!
//! Pure state, geometry and hit-testing in logical pixels; no GPU.

use crate::geom::Rect;
use crate::theme::{text_color, ChromeTheme};

/// Visual state of a server row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Default,
    Hover,
    Selected,
    Focus,
    Dragging,
}

/// Colour role of the right-hand meta text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaTone {
    Muted,
    Success,
}

/// Content of a server row's right meta slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowMeta {
    /// WSL stopped, plain SSH host: nothing on the right.
    None,
    /// WSL distro currently running ("Running", success colour).
    Running,
    /// Number of open sessions on the machine.
    Sessions(u32),
}

impl RowMeta {
    pub fn label(&self) -> Option<String> {
        match self {
            RowMeta::None => None,
            RowMeta::Running => Some("Running".to_owned()),
            RowMeta::Sessions(n) => Some(n.to_string()),
        }
    }

    pub fn tone(&self) -> MetaTone {
        match self {
            RowMeta::Running => MetaTone::Success,
            _ => MetaTone::Muted,
        }
    }
}

/// Resolved paint instructions for a server row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowStyle {
    pub bg: Option<[f32; 4]>,
    /// Name in Medium weight (otherwise Regular).
    pub medium: bool,
    /// Identity tile turns accent with an `on_accent` mark.
    pub tile_active: bool,
    /// Drop shadow under the row (dragging).
    pub shadow: bool,
    /// Focus ring (2px canvas gap + 2px accent).
    pub ring: bool,
}

/// True when `(px, py)` is inside `row`.
pub fn row_hit(row: &Rect, px: f32, py: f32) -> bool {
    row.contains(px, py)
}

/// `count` rows of [`server_row::HEIGHT`] stacked from `(x, y)` with `gap` between.
pub fn stack_rows(x: f32, y: f32, width: f32, count: usize, gap: f32) -> Vec<Rect> {
    (0..count)
        .map(|i| {
            Rect::new(
                x,
                y + i as f32 * (server_row::HEIGHT + gap),
                width,
                server_row::HEIGHT,
            )
        })
        .collect()
}

/// Index of the row under `(px, py)`.
pub fn hit_row(rows: &[Rect], px: f32, py: f32) -> Option<usize> {
    rows.iter().position(|r| row_hit(r, px, py))
}

pub mod server_row {
    use super::*;
    use crate::tokens::{height, radius, tile};

    pub const HEIGHT: f32 = height::ROW;
    pub const RADIUS: f32 = radius::CONTROL;
    pub const PAD_X: f32 = 8.0;
    pub const TILE: f32 = tile::SM;
    pub const GAP: f32 = 12.0;
    pub const NAME_SIZE: f32 = 14.0;
    pub const META_SIZE: f32 = 12.0;
    /// Meta colour on a selected row (`#CFC4E6`).
    pub const META_ON_SELECTED: [u8; 4] = [0xCF, 0xC4, 0xE6, 255];
    /// Focus ring: canvas-coloured gap then accent band, 2px each.
    pub const RING_GAP: f32 = 2.0;
    pub const RING_WIDTH: f32 = 2.0;

    pub fn tile_rect(row: &Rect) -> Rect {
        Rect::new(row.x + PAD_X, row.y + (row.height - TILE) * 0.5, TILE, TILE)
    }

    pub fn name_x(row: &Rect) -> f32 {
        row.x + PAD_X + TILE + GAP
    }

    /// Right edge (text is right-aligned to it) of the meta slot.
    pub fn meta_right(row: &Rect) -> f32 {
        row.right() - PAD_X
    }

    /// `(outer, inner)` rects of the focus ring: fill `outer` with the
    /// accent, then `inner` with the canvas, then draw the row on top.
    pub fn focus_ring(row: &Rect) -> (Rect, Rect) {
        let grow = |by: f32| {
            Rect::new(
                row.x - by,
                row.y - by,
                row.width + 2.0 * by,
                row.height + 2.0 * by,
            )
        };
        (grow(RING_GAP + RING_WIDTH), grow(RING_GAP))
    }

    pub fn style(theme: &ChromeTheme, state: RowState) -> RowStyle {
        RowStyle {
            bg: match state {
                RowState::Default | RowState::Focus => None,
                RowState::Hover => Some(theme.surface),
                RowState::Selected => Some(theme.selected),
                RowState::Dragging => Some(theme.raised),
            },
            medium: state == RowState::Selected,
            tile_active: state == RowState::Selected,
            shadow: state == RowState::Dragging,
            ring: state == RowState::Focus,
        }
    }

    pub fn meta_color(theme: &ChromeTheme, tone: MetaTone, state: RowState) -> [u8; 4] {
        match tone {
            MetaTone::Success => text_color(theme.success),
            MetaTone::Muted if state == RowState::Selected => META_ON_SELECTED,
            MetaTone::Muted => theme.text_muted,
        }
    }
}

/// Section header ("This computer") and group header ("jeremy" + "New group").
pub mod section_header {
    use super::*;

    pub const HEIGHT: f32 = 20.0;
    pub const SIZE: f32 = 12.0;

    /// Hit rect of the right-aligned text action of width `text_w`.
    pub fn action_rect(header: &Rect, text_w: f32) -> Rect {
        Rect::new(header.right() - text_w, header.y, text_w, header.height)
    }
}

/// "Move to <group>" drop target: dashed accent outline, 8% accent fill.
pub mod drop_target {
    use super::*;
    use crate::tokens::{font_size, height, radius};

    pub const HEIGHT: f32 = height::ROW;
    pub const RADIUS: f32 = radius::CONTROL;
    pub const FILL_ALPHA: f32 = 0.08;
    pub const LABEL_SIZE: f32 = font_size::LABEL;

    pub fn rect(x: f32, y: f32, width: f32) -> Rect {
        Rect::new(x, y, width, HEIGHT)
    }

    pub fn fill(theme: &ChromeTheme) -> [f32; 4] {
        let a = theme.accent;
        [a[0], a[1], a[2], FILL_ALPHA]
    }
}

/// Visual state of a view tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    Default,
    Hover,
    Active,
}

/// Measured widths of one tab (text widths come from the painter).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabSize {
    pub label_w: f32,
    /// Badge text width, 0 when there is no badge.
    pub badge_w: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStyle {
    pub text: [u8; 4],
    pub medium: bool,
    /// 2px underline colour, if any.
    pub underline: Option<[f32; 4]>,
}

pub mod view_tabs {
    use super::*;

    /// 20px line + 12px padding + 2px underline.
    pub const HEIGHT: f32 = 34.0;
    pub const GAP: f32 = 24.0;
    pub const BADGE_GAP: f32 = 6.0;
    pub const UNDERLINE: f32 = 2.0;
    pub const SIZE: f32 = 14.0;

    pub fn width(size: TabSize) -> f32 {
        if size.badge_w > 0.0 {
            size.label_w + BADGE_GAP + size.badge_w
        } else {
            size.label_w
        }
    }

    /// Tab rects laid out left to right from `(x, y)`.
    pub fn layout(x: f32, y: f32, sizes: &[TabSize]) -> Vec<Rect> {
        let mut cx = x;
        sizes
            .iter()
            .map(|s| {
                let r = Rect::new(cx, y, width(*s), HEIGHT);
                cx += r.width + GAP;
                r
            })
            .collect()
    }

    pub fn hit(tabs: &[Rect], px: f32, py: f32) -> Option<usize> {
        tabs.iter().position(|r| r.contains(px, py))
    }

    pub fn underline_rect(tab: &Rect) -> Rect {
        Rect::new(tab.x, tab.bottom() - UNDERLINE, tab.width, UNDERLINE)
    }

    pub fn style(theme: &ChromeTheme, state: TabState) -> TabStyle {
        match state {
            TabState::Default => TabStyle {
                text: theme.text_muted,
                medium: false,
                underline: None,
            },
            TabState::Hover => TabStyle {
                text: theme.text,
                medium: false,
                underline: Some(theme.line),
            },
            TabState::Active => TabStyle {
                text: theme.text,
                medium: true,
                underline: Some(theme.accent),
            },
        }
    }
}

/// Visual state of a session pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillState {
    Default,
    Hover,
    Active,
    /// Output arrived in a session that is not being looked at.
    NewOutput,
}

/// Which part of a pill was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillHit {
    Body,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillStyle {
    pub bg: Option<[f32; 4]>,
    pub text: [u8; 4],
    /// Show the close × (hover and active).
    pub close: bool,
    /// Show the 6px accent activity dot.
    pub dot: bool,
}

pub mod session_pill {
    use super::*;
    use crate::tokens::{height, radius};

    pub const HEIGHT: f32 = height::CONTROL_SM;
    pub const RADIUS: f32 = radius::PILL;
    pub const PAD_LEFT: f32 = 12.0;
    pub const PAD_RIGHT: f32 = 12.0;
    pub const PAD_RIGHT_CLOSE: f32 = 8.0;
    pub const GAP: f32 = 8.0;
    pub const DOT: f32 = 6.0;
    pub const CLOSE_ICON: f32 = 12.0;
    /// Clickable square around the close icon.
    pub const CLOSE_HIT: f32 = 20.0;
    pub const SIZE: f32 = 13.0;

    /// Pill width for a label of `label_w`; `close` reserves the ×,
    /// `dot` the activity dot.
    pub fn width(label_w: f32, close: bool, dot: bool) -> f32 {
        let dot_w = if dot { DOT + GAP } else { 0.0 };
        let tail = if close {
            GAP + CLOSE_ICON + PAD_RIGHT_CLOSE
        } else {
            PAD_RIGHT
        };
        PAD_LEFT + dot_w + label_w + tail
    }

    pub fn dot_rect(pill: &Rect) -> Rect {
        Rect::new(
            pill.x + PAD_LEFT,
            pill.y + (pill.height - DOT) * 0.5,
            DOT,
            DOT,
        )
    }

    pub fn text_x(pill: &Rect, dot: bool) -> f32 {
        pill.x + PAD_LEFT + if dot { DOT + GAP } else { 0.0 }
    }

    /// Rect of the 12px × glyph.
    pub fn close_icon_rect(pill: &Rect) -> Rect {
        Rect::new(
            pill.right() - PAD_RIGHT_CLOSE - CLOSE_ICON,
            pill.y + (pill.height - CLOSE_ICON) * 0.5,
            CLOSE_ICON,
            CLOSE_ICON,
        )
    }

    /// Hit rect of the close ×, larger than the glyph.
    pub fn close_rect(pill: &Rect) -> Rect {
        let i = close_icon_rect(pill);
        let pad = (CLOSE_HIT - CLOSE_ICON) * 0.5;
        Rect::new(
            i.x - pad,
            pill.y + (pill.height - CLOSE_HIT) * 0.5,
            CLOSE_HIT,
            CLOSE_HIT,
        )
    }

    /// Hit-test; the close part only counts when `close` is shown.
    pub fn hit(pill: &Rect, close: bool, px: f32, py: f32) -> Option<PillHit> {
        if !pill.contains(px, py) {
            return None;
        }
        if close && close_rect(pill).contains(px, py) {
            Some(PillHit::Close)
        } else {
            Some(PillHit::Body)
        }
    }

    pub fn style(theme: &ChromeTheme, state: PillState) -> PillStyle {
        match state {
            PillState::Default => PillStyle {
                bg: None,
                text: theme.text_muted,
                close: false,
                dot: false,
            },
            PillState::Hover => PillStyle {
                bg: Some(theme.surface),
                text: theme.text,
                close: true,
                dot: false,
            },
            PillState::Active => PillStyle {
                bg: Some(theme.divider),
                text: theme.text,
                close: true,
                dot: false,
            },
            PillState::NewOutput => PillStyle {
                bg: None,
                text: theme.text_muted,
                close: false,
                dot: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::theme::ChromeTheme;

    #[test]
    fn server_row_geometry_matches_board() {
        let row = Rect::new(10.0, 20.0, 248.0, server_row::HEIGHT);
        assert_eq!(server_row::HEIGHT, 44.0);
        let tile = server_row::tile_rect(&row);
        assert_eq!((tile.x, tile.width, tile.height), (18.0, 28.0, 28.0));
        assert_eq!(tile.y, 20.0 + 8.0);
        assert_eq!(server_row::name_x(&row), 18.0 + 28.0 + 12.0);
        assert_eq!(server_row::meta_right(&row), 258.0 - 8.0);
        let (outer, inner) = server_row::focus_ring(&row);
        assert_eq!(inner.x, row.x - 2.0);
        assert_eq!(outer.x, row.x - 4.0);
        assert_eq!(outer.height, row.height + 8.0);
    }

    #[test]
    fn server_row_styles_per_state() {
        let t = ChromeTheme::default();
        let d = server_row::style(&t, RowState::Default);
        assert_eq!(d.bg, None);
        assert!(!d.medium && !d.tile_active && !d.shadow && !d.ring);
        assert_eq!(server_row::style(&t, RowState::Hover).bg, Some(t.surface));
        let s = server_row::style(&t, RowState::Selected);
        assert_eq!(s.bg, Some(t.selected));
        assert!(s.medium && s.tile_active);
        assert!(server_row::style(&t, RowState::Focus).ring);
        let g = server_row::style(&t, RowState::Dragging);
        assert_eq!(g.bg, Some(t.raised));
        assert!(g.shadow);
    }

    #[test]
    fn meta_colour_by_tone_and_state() {
        let t = ChromeTheme::default();
        assert_eq!(
            server_row::meta_color(&t, MetaTone::Success, RowState::Default),
            crate::theme::text_color(t.success)
        );
        assert_eq!(
            server_row::meta_color(&t, MetaTone::Muted, RowState::Default),
            t.text_muted
        );
        assert_eq!(
            server_row::meta_color(&t, MetaTone::Muted, RowState::Selected),
            server_row::META_ON_SELECTED
        );
    }

    #[test]
    fn row_meta_labels() {
        assert_eq!(RowMeta::None.label(), None);
        assert_eq!(RowMeta::Running.label().as_deref(), Some("Running"));
        assert_eq!(RowMeta::Sessions(2).label().as_deref(), Some("2"));
        assert_eq!(RowMeta::Running.tone(), MetaTone::Success);
        assert_eq!(RowMeta::Sessions(2).tone(), MetaTone::Muted);
    }

    #[test]
    fn row_hit_test() {
        let row = Rect::new(0.0, 0.0, 248.0, 44.0);
        assert!(row_hit(&row, 100.0, 22.0));
        assert!(!row_hit(&row, 100.0, 44.0));
        let rows = stack_rows(0.0, 0.0, 248.0, 3, 2.0);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].y, 46.0);
        assert_eq!(hit_row(&rows, 5.0, 50.0), Some(1));
        assert_eq!(hit_row(&rows, 5.0, 45.0), None); // in the gap
    }

    #[test]
    fn headers_and_drop_target() {
        let h = Rect::new(0.0, 0.0, 248.0, section_header::HEIGHT);
        let a = section_header::action_rect(&h, 60.0);
        assert_eq!(a.right(), h.right());
        assert_eq!(a.width, 60.0);
        assert_eq!(a.height, h.height);
        let t = drop_target::rect(0.0, 0.0, 248.0);
        assert_eq!((t.width, t.height), (248.0, 44.0));
        assert_eq!(drop_target::RADIUS, 10.0);
        assert!((drop_target::FILL_ALPHA - 0.08).abs() < 1e-6);
        let f = drop_target::fill(&ChromeTheme::default());
        assert!((f[3] - 0.08).abs() < 1e-6);
    }

    #[test]
    fn tabs_layout_and_style() {
        let t = ChromeTheme::default();
        let tabs = view_tabs::layout(
            0.0,
            0.0,
            &[
                TabSize {
                    label_w: 40.0,
                    badge_w: 0.0,
                },
                TabSize {
                    label_w: 50.0,
                    badge_w: 8.0,
                },
            ],
        );
        assert_eq!(tabs[0].width, 40.0);
        assert_eq!(tabs[0].height, view_tabs::HEIGHT);
        assert_eq!(tabs[1].x, 40.0 + view_tabs::GAP);
        assert_eq!(tabs[1].width, 50.0 + view_tabs::BADGE_GAP + 8.0);
        assert_eq!(view_tabs::hit(&tabs, 45.0, 5.0), None);
        assert_eq!(view_tabs::hit(&tabs, 70.0, 5.0), Some(1));
        let u = view_tabs::underline_rect(&tabs[0]);
        assert_eq!((u.height, u.bottom()), (2.0, tabs[0].bottom()));
        let a = view_tabs::style(&t, TabState::Active);
        assert!(a.medium);
        assert_eq!(a.underline, Some(t.accent));
        assert_eq!(a.text, t.text);
        let h = view_tabs::style(&t, TabState::Hover);
        assert_eq!(
            (h.text, h.underline, h.medium),
            (t.text, Some(t.line), false)
        );
        let d = view_tabs::style(&t, TabState::Default);
        assert_eq!((d.text, d.underline), (t.text_muted, None));
    }

    #[test]
    fn pill_geometry_and_hits() {
        assert_eq!(session_pill::HEIGHT, 30.0);
        assert_eq!(session_pill::width(40.0, false, false), 12.0 + 40.0 + 12.0);
        assert_eq!(
            session_pill::width(40.0, true, false),
            12.0 + 40.0 + 8.0 + 12.0 + 8.0
        );
        assert_eq!(
            session_pill::width(40.0, false, true),
            12.0 + 6.0 + 8.0 + 40.0 + 12.0
        );
        let r = Rect::new(100.0, 0.0, session_pill::width(40.0, true, false), 30.0);
        let c = session_pill::close_rect(&r);
        assert_eq!(
            c.right(),
            r.right() - 8.0 + (session_pill::CLOSE_HIT - session_pill::CLOSE_ICON) / 2.0
        );
        assert_eq!(c.y + c.height / 2.0, 15.0);
        assert_eq!(
            session_pill::hit(&r, true, c.x + 1.0, 15.0),
            Some(PillHit::Close)
        );
        assert_eq!(
            session_pill::hit(&r, true, 105.0, 15.0),
            Some(PillHit::Body)
        );
        assert_eq!(
            session_pill::hit(&r, false, r.right() - 10.0, 15.0),
            Some(PillHit::Body)
        );
        assert_eq!(session_pill::hit(&r, true, 0.0, 15.0), None);
        let dot = session_pill::dot_rect(&r);
        assert_eq!((dot.x, dot.width), (112.0, 6.0));
        assert_eq!(dot.y + 3.0, 15.0);
        assert_eq!(session_pill::text_x(&r, true), 100.0 + 12.0 + 6.0 + 8.0);
        assert_eq!(session_pill::text_x(&r, false), 112.0);
    }

    #[test]
    fn pill_styles() {
        let t = ChromeTheme::default();
        let d = session_pill::style(&t, PillState::Default);
        assert_eq!((d.bg, d.close, d.dot), (None, false, false));
        assert_eq!(d.text, t.text_muted);
        let h = session_pill::style(&t, PillState::Hover);
        assert_eq!((h.bg, h.close, h.text), (Some(t.surface), true, t.text));
        let a = session_pill::style(&t, PillState::Active);
        assert_eq!((a.bg, a.close, a.text), (Some(t.divider), true, t.text));
        let n = session_pill::style(&t, PillState::NewOutput);
        assert_eq!((n.bg, n.close, n.dot), (None, false, true));
        assert_eq!(n.text, t.text_muted);
    }
}
