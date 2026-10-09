use super::*;

/// Right margin of the tab strip: caption buttons (Windows) + action
/// strip (+ / search), or a small gap + actions elsewhere.
#[inline]
pub(super) fn island_margin_right() -> f32 {
    action_strip_width() + {
        #[cfg(target_os = "windows")]
        {
            crate::renderer::window_controls::MARGIN_RIGHT
        }
        #[cfg(not(target_os = "windows"))]
        {
            ISLAND_MARGIN_RIGHT
        }
    }
}

/// Left inset for the app logo (or macOS traffic lights).
#[inline]
pub(super) fn island_margin_left() -> f32 {
    #[cfg(target_os = "macos")]
    {
        ISLAND_MARGIN_LEFT_MACOS
    }
    #[cfg(not(target_os = "macos"))]
    {
        LOGO_SLOT
    }
}

/// Width of the + / search action strip before window controls.
/// Apple HIG mock has no search/+ strip — only traffic lights / captions + tabs.
pub const ACTION_SLOT: f32 = 0.0;
pub(super) const ACTION_STRIP_COUNT: f32 = 0.0;
/// App-logo hit/paint slot on the left of the title bar.
/// Non-macOS: small inset instead of a logo (mock has no logo).
pub const LOGO_SLOT: f32 = 12.0;
pub(super) const TITLEBAR_ICON: f32 = 14.0;

#[inline]
pub fn action_strip_width() -> f32 {
    0.0
}

/// Title-bar chrome hit (excluding tabs / window controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBarAction {
    Logo,
    NewTab,
    Search,
}

/// Hit-test logo / + / search in logical coordinates.
/// Apple HIG mock: no logo / search / + — always `None`.
pub fn title_bar_hit(
    _window_width_logical: f32,
    _x: f32,
    _y: f32,
) -> Option<TitleBarAction> {
    None
}

pub(super) fn fit_title_to_width<'a>(
    sugarloaf: &mut Sugarloaf,
    title: &'a str,
    max_width: f32,
) -> Cow<'a, str> {
    let attrs = Attributes::default();
    fit_title_with_widths(title, max_width, |c| {
        sugarloaf.char_advance(c, attrs, TITLE_FONT_SIZE)
    })
}

pub(super) fn fit_title_with_widths<'a>(
    title: &'a str,
    max_width: f32,
    mut char_width: impl FnMut(char) -> f32,
) -> Cow<'a, str> {
    let suffix_width = char_width(TITLE_ELLIPSIS);

    // `truncate_ix` tracks the last byte offset at which the prefix so
    // far still has room for the suffix. Updated before adding the next
    // char's width so the moment we detect overflow we already know
    // where to cut.
    let mut accumulated: f32 = 0.0;
    let mut truncate_ix: usize = 0;
    for (ix, c) in title.char_indices() {
        if accumulated + suffix_width <= max_width {
            truncate_ix = ix;
        }
        accumulated += char_width(c);
        if accumulated > max_width {
            let mut out = String::with_capacity(truncate_ix + TITLE_ELLIPSIS.len_utf8());
            out.push_str(&title[..truncate_ix]);
            out.push(TITLE_ELLIPSIS);
            return Cow::Owned(out);
        }
    }
    Cow::Borrowed(title)
}

#[derive(Clone, PartialEq)]
pub struct TabStripLayout {
    pub left_margin: f32,
    pub right_margin: f32,
    /// Per-tab slot widths (content-hug, clamped). Empty when there are no tabs.
    pub widths: SmallVec<[f32; 12]>,
}

impl TabStripLayout {
    #[inline]
    pub fn len(&self) -> usize {
        self.widths.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.widths.is_empty()
    }

    /// Slot width for `index`, or 0 when out of range.
    #[inline]
    pub fn width_at(&self, index: usize) -> f32 {
        self.widths.get(index).copied().unwrap_or(0.0)
    }

    /// Left edge of tab `index` (logical px).
    #[inline]
    pub fn slot_x(&self, index: usize) -> f32 {
        self.left_margin + self.widths.iter().take(index).sum::<f32>()
    }

    /// Total width of all tab slots.
    #[inline]
    pub fn tabs_width(&self) -> f32 {
        self.widths.iter().sum()
    }
}

/// Status-dot diameter inside a pill (logical px).
pub(super) const STATUS_DOT: f32 = 8.0;
pub(super) const STATUS_GAP: f32 = 8.0;
/// Floor so a tiny label still reads as a pill (not a chip).
pub(super) const MIN_TAB_WIDTH: f32 = 72.0;
/// Trailing room reserved for the hover × on closable tabs.
pub(super) const CLOSE_RESERVE: f32 = CLOSE_MARGIN_RIGHT + CLOSE_HIT_HALF_WIDTH;

/// Natural slot width for measured title text (+ optional OS icon).
///
/// Layout: `[gap/2 | pad | dot | gap | icon? | title | pad | close? | gap/2]`
/// — the pill hugs its content; closable tabs keep a trailing × slot so
/// the hover affordance does not reflow the strip.
pub fn tab_slot_width_for_content(
    text_width: f32,
    has_icon: bool,
    closable: bool,
) -> f32 {
    let icon = if has_icon { TITLEBAR_ICON + 4.0 } else { 0.0 };
    let close = if closable { CLOSE_RESERVE } else { 0.0 };
    TAB_GAP
        + TAB_PADDING_X
        + STATUS_DOT
        + STATUS_GAP
        + icon
        + text_width.max(0.0)
        + TAB_PADDING_X
        + close
}

/// Build a left-aligned content-hug strip from per-tab natural widths.
///
/// When the sum overflows the available band, every slot is scaled down
/// proportionally (still left-aligned — never stretched to fill).
pub fn tab_strip_layout_from_widths(
    window_width: f32,
    scale_factor: f32,
    max_tab_width: f32,
    natural_widths: &[f32],
) -> TabStripLayout {
    let left_margin = island_margin_left();
    let right_margin = island_margin_right();
    let available = ((window_width / scale_factor) - right_margin - left_margin).max(0.0);
    let cap = max_tab_width.max(0.0);

    let mut widths: SmallVec<[f32; 12]> = natural_widths
        .iter()
        .map(|&w| {
            if cap > 0.0 {
                w.clamp(MIN_TAB_WIDTH.min(cap), cap.max(MIN_TAB_WIDTH))
            } else {
                w.max(0.0)
            }
        })
        .collect();

    let total: f32 = widths.iter().sum();
    if total > available && total > 0.0 && available > 0.0 {
        let scale = available / total;
        for w in &mut widths {
            *w *= scale;
        }
    } else if available <= 0.0 {
        widths.fill(0.0);
    }

    TabStripLayout {
        left_margin,
        right_margin,
        widths,
    }
}

/// Uniform fallback used by unit tests and before the first paint measures titles.
pub fn tab_strip_layout(
    window_width: f32,
    scale_factor: f32,
    num_tabs: usize,
    max_tab_width: f32,
) -> TabStripLayout {
    let n = num_tabs;
    let natural = tab_slot_width_for_content(48.0, false, true)
        .min(max_tab_width.max(MIN_TAB_WIDTH))
        .max(MIN_TAB_WIDTH);
    let widths = vec![natural; n];
    tab_strip_layout_from_widths(window_width, scale_factor, max_tab_width, &widths)
}
