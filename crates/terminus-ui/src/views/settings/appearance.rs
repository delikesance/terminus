//! Appearance tab: font, size stepper, cursor, theme and a live preview.
//!
//! Font, size, cursor and theme are written to the config file the app
//! hot-reloads (`[fonts] family/size`, `[cursor] shape`, `[appearance]
//! theme`). System follows the OS light/dark preference live.

use super::{column, Key, Measure, SettingsAction};
use crate::components::input::{field_layout, FieldKind, FieldLayout};
use crate::components::selection::{SegmentedLayout, SegmentedSize};
use crate::geom::Rect;
use crate::theme::ThemeMode;

pub const LEFT_WIDTH: f32 = 420.0;
pub const COLUMN_GAP: f32 = 28.0;
pub const SECTION_GAP: f32 = 22.0;
pub const LABEL_HEIGHT: f32 = 16.0;
pub const LABEL_GAP: f32 = 8.0;
pub const MIN_SIZE: f32 = 8.0;
pub const MAX_SIZE: f32 = 32.0;
pub const DEFAULT_SIZE: f32 = 16.0;
pub const STEPPER_BTN_W: f32 = 38.0;
pub const STEPPER_VALUE_W: f32 = 44.0;
pub const STEPPER_GAP: f32 = 4.0;
pub const STEPPER_PAD: f32 = 3.0;
pub const STEPPER_BTN_H: f32 = 34.0;
pub const MENU_ROW_H: f32 = 36.0;
pub const MENU_PAD: f32 = 4.0;
pub const MENU_MAX_ROWS: usize = 8;
pub const PREVIEW_HEIGHT: f32 = 150.0;
/// Shown under the theme control while System is selected.
pub const THEME_NOTE: &str = "Follows your system's light or dark setting.";
pub const DEFAULT_FONT_LABEL: &str = "Default";

/// Families offered first even if the system lists them among many others.
const KNOWN_MONO: &[&str] = &[
    "JetBrains Mono",
    "Fira Code",
    "Fira Mono",
    "Cascadia Code",
    "Cascadia Mono",
    "Source Code Pro",
    "Hack",
    "Iosevka",
    "Menlo",
    "Monaco",
    "Consolas",
    "Courier New",
    "SF Mono",
    "IBM Plex Mono",
    "Roboto Mono",
    "Inconsolata",
    "Martian Mono",
    "Geist Mono",
    "Victor Mono",
    "Ubuntu Mono",
    "Liberation Mono",
    "DejaVu Sans Mono",
    "Noto Sans Mono",
    "Terminus",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Beam,
}

impl CursorStyle {
    pub const ALL: [CursorStyle; 3] = [
        CursorStyle::Block,
        CursorStyle::Underline,
        CursorStyle::Beam,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CursorStyle::Block => "Block",
            CursorStyle::Underline => "Underline",
            CursorStyle::Beam => "Beam",
        }
    }

    /// Value of `[cursor] shape`.
    pub fn config_value(self) -> &'static str {
        match self {
            CursorStyle::Block => "block",
            CursorStyle::Underline => "underline",
            CursorStyle::Beam => "beam",
        }
    }

    pub fn from_config(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "underline" => CursorStyle::Underline,
            "beam" => CursorStyle::Beam,
            _ => CursorStyle::Block,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] =
        [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
            ThemeChoice::System => "System",
        }
    }

    /// Value of `[appearance] theme`.
    pub fn config_value(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "dark",
            ThemeChoice::Light => "light",
            ThemeChoice::System => "system",
        }
    }

    pub fn from_config(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "light" => ThemeChoice::Light,
            "system" => ThemeChoice::System,
            _ => ThemeChoice::Dark,
        }
    }

    /// The palette to show: System defers to the OS (`system`, dark when
    /// the platform does not say).
    pub fn resolve(self, system: Option<ThemeMode>) -> ThemeMode {
        match self {
            ThemeChoice::Dark => ThemeMode::Dark,
            ThemeChoice::Light => ThemeMode::Light,
            ThemeChoice::System => system.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceTarget {
    FontSelect,
    FontRow(usize),
    SizeMinus,
    SizePlus,
    Cursor(usize),
    Theme(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppearanceState {
    /// Offered families (see [`AppearanceState::set_fonts`]).
    pub fonts: Vec<String>,
    /// Configured family; empty is the app default.
    pub font: String,
    pub size: f32,
    pub cursor: CursorStyle,
    pub theme: ThemeChoice,
    pub font_menu_open: bool,
    /// First visible menu row.
    pub font_scroll: usize,
    pub hover: Option<AppearanceTarget>,
}

impl Default for AppearanceState {
    fn default() -> Self {
        Self {
            fonts: Vec::new(),
            font: String::new(),
            size: DEFAULT_SIZE,
            cursor: CursorStyle::Block,
            theme: ThemeChoice::Dark,
            font_menu_open: false,
            font_scroll: 0,
            hover: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppearanceLayout {
    pub font_label: Rect,
    pub font_select: FieldLayout,
    pub size_label: Rect,
    pub size_track: Rect,
    pub size_minus: Rect,
    pub size_value: Rect,
    pub size_plus: Rect,
    pub cursor_label: Rect,
    pub cursor: SegmentedLayout,
    pub theme_label: Rect,
    pub theme: SegmentedLayout,
    pub theme_note: Rect,
    pub preview_label: Rect,
    pub preview: Rect,
    /// Open menu panel and its visible rows (`index into fonts`, rect).
    pub menu: Option<Rect>,
    pub menu_rows: Vec<(usize, Rect)>,
}

impl AppearanceState {
    pub fn new(family: Option<&str>, size: f32, cursor: CursorStyle) -> Self {
        Self {
            font: family.unwrap_or("").to_string(),
            size: size.clamp(MIN_SIZE, MAX_SIZE),
            cursor,
            ..Self::default()
        }
    }

    /// Offer the monospace-looking subset of `installed` (all of them when
    /// none look monospace), always including the configured family.
    pub fn set_fonts(&mut self, installed: Vec<String>) {
        let mono = |name: &str| {
            let l = name.to_ascii_lowercase();
            KNOWN_MONO.iter().any(|k| k.eq_ignore_ascii_case(name))
                || l.contains("mono")
                || l.contains("code")
                || l.contains("courier")
                || l.contains("consol")
                || l.contains("terminal")
        };
        let mut list: Vec<String> =
            installed.iter().filter(|n| mono(n)).cloned().collect();
        if list.is_empty() {
            list = installed;
        }
        if !self.font.is_empty() && !list.contains(&self.font) {
            list.push(self.font.clone());
        }
        list.sort_by_key(|n| n.to_ascii_lowercase());
        list.dedup();
        self.fonts = list;
    }

    pub fn font_label(&self) -> &str {
        if self.font.is_empty() {
            DEFAULT_FONT_LABEL
        } else {
            &self.font
        }
    }

    pub fn layout(&self, content: Rect, m: Measure) -> AppearanceLayout {
        let col = column(content);
        let w = col.width.min(LEFT_WIDTH);
        let x = col.x;
        let mut y = col.y;

        let font_label = Rect::new(x, y, w, LABEL_HEIGHT);
        y += LABEL_HEIGHT + LABEL_GAP;
        let font_select = field_layout((x, y), w, FieldKind::Select, false, false);
        y = font_select.total.bottom() + SECTION_GAP;

        let size_label = Rect::new(x, y, w, LABEL_HEIGHT);
        y += LABEL_HEIGHT + LABEL_GAP;
        let track_w =
            2.0 * STEPPER_PAD + 2.0 * STEPPER_BTN_W + STEPPER_VALUE_W + 2.0 * STEPPER_GAP;
        let track = Rect::new(x, y, track_w, STEPPER_BTN_H + 2.0 * STEPPER_PAD);
        let by = track.y + STEPPER_PAD;
        let size_minus =
            Rect::new(track.x + STEPPER_PAD, by, STEPPER_BTN_W, STEPPER_BTN_H);
        let size_value = Rect::new(
            size_minus.right() + STEPPER_GAP,
            by,
            STEPPER_VALUE_W,
            STEPPER_BTN_H,
        );
        let size_plus = Rect::new(
            size_value.right() + STEPPER_GAP,
            by,
            STEPPER_BTN_W,
            STEPPER_BTN_H,
        );
        y = track.bottom() + SECTION_GAP;

        let cursor_label = Rect::new(x, y, w, LABEL_HEIGHT);
        y += LABEL_HEIGHT + LABEL_GAP;
        let cw: Vec<f32> = CursorStyle::ALL
            .iter()
            .map(|c| m(c.label(), SegmentedSize::Medium.font_size(), false))
            .collect();
        let cursor = SegmentedLayout::new(x, y, &cw, SegmentedSize::Medium);
        y = cursor.track.bottom() + SECTION_GAP;

        let theme_label = Rect::new(x, y, w, LABEL_HEIGHT);
        y += LABEL_HEIGHT + LABEL_GAP;
        let tw: Vec<f32> = ThemeChoice::ALL
            .iter()
            .map(|c| m(c.label(), SegmentedSize::Medium.font_size(), false))
            .collect();
        let theme = SegmentedLayout::new(x, y, &tw, SegmentedSize::Medium);
        let theme_note = Rect::new(x, theme.track.bottom() + 10.0, w, 16.0);

        let px = col.x + LEFT_WIDTH + COLUMN_GAP;
        let pw = (col.right() - px).max(0.0);
        let preview_label = Rect::new(px, col.y, pw, LABEL_HEIGHT);
        let preview = Rect::new(px, col.y + LABEL_HEIGHT + LABEL_GAP, pw, PREVIEW_HEIGHT);

        let (menu, menu_rows) = if self.font_menu_open && !self.fonts.is_empty() {
            let rows = self.fonts.len().min(MENU_MAX_ROWS);
            let first = self.font_scroll.min(self.fonts.len() - rows);
            let b = font_select.box_rect;
            let panel = Rect::new(
                b.x,
                b.bottom() + 4.0,
                b.width,
                rows as f32 * MENU_ROW_H + 2.0 * MENU_PAD,
            );
            let rects = (0..rows)
                .map(|i| {
                    (
                        first + i,
                        Rect::new(
                            panel.x + MENU_PAD,
                            panel.y + MENU_PAD + i as f32 * MENU_ROW_H,
                            panel.width - 2.0 * MENU_PAD,
                            MENU_ROW_H,
                        ),
                    )
                })
                .collect();
            (Some(panel), rects)
        } else {
            (None, Vec::new())
        };

        AppearanceLayout {
            font_label,
            font_select,
            size_label,
            size_track: track,
            size_minus,
            size_value,
            size_plus,
            cursor_label,
            cursor,
            theme_label,
            theme,
            theme_note,
            preview_label,
            preview,
            menu,
            menu_rows,
        }
    }

    pub fn hit(&self, l: &AppearanceLayout, x: f32, y: f32) -> Option<AppearanceTarget> {
        if let Some(panel) = l.menu {
            // The open menu is modal-ish: it owns the pointer over its panel.
            if panel.contains(x, y) {
                return l
                    .menu_rows
                    .iter()
                    .find(|(_, r)| r.contains(x, y))
                    .map(|(i, _)| AppearanceTarget::FontRow(*i));
            }
        }
        if l.font_select.box_rect.contains(x, y) {
            return Some(AppearanceTarget::FontSelect);
        }
        if l.size_minus.contains(x, y) {
            return Some(AppearanceTarget::SizeMinus);
        }
        if l.size_plus.contains(x, y) {
            return Some(AppearanceTarget::SizePlus);
        }
        if let Some(i) = l.cursor.hit_test(x, y) {
            return Some(AppearanceTarget::Cursor(i));
        }
        l.theme.hit_test(x, y).map(AppearanceTarget::Theme)
    }

    pub fn hover(&mut self, content: Rect, m: Measure, x: f32, y: f32) -> bool {
        let l = self.layout(content, m);
        let t = self.hit(&l, x, y);
        let changed = t != self.hover;
        self.hover = t;
        changed
    }

    pub fn press(
        &mut self,
        content: Rect,
        m: Measure,
        x: f32,
        y: f32,
    ) -> Option<SettingsAction> {
        let l = self.layout(content, m);
        let target = self.hit(&l, x, y);
        let was_open = self.font_menu_open;
        // Any press outside the menu closes it.
        self.font_menu_open = false;
        match target {
            Some(AppearanceTarget::FontSelect) => {
                self.font_menu_open = !was_open;
                if self.font_menu_open {
                    self.scroll_to_current();
                }
                None
            }
            Some(AppearanceTarget::FontRow(i)) => self.pick_font(i),
            Some(AppearanceTarget::SizeMinus) => self.bump_size(-1.0),
            Some(AppearanceTarget::SizePlus) => self.bump_size(1.0),
            Some(AppearanceTarget::Cursor(i)) => {
                let c = CursorStyle::ALL[i];
                if c == self.cursor {
                    return None;
                }
                self.cursor = c;
                Some(SettingsAction::SetCursor(c))
            }
            Some(AppearanceTarget::Theme(i)) => {
                let t = ThemeChoice::ALL[i];
                if t == self.theme {
                    return None;
                }
                self.theme = t;
                Some(SettingsAction::SetTheme(t))
            }
            None => None,
        }
    }

    fn scroll_to_current(&mut self) {
        let at = self.fonts.iter().position(|f| *f == self.font).unwrap_or(0);
        let rows = self.fonts.len().min(MENU_MAX_ROWS);
        self.font_scroll = at
            .saturating_sub(rows / 2)
            .min(self.fonts.len().saturating_sub(rows));
    }

    fn pick_font(&mut self, i: usize) -> Option<SettingsAction> {
        let name = self.fonts.get(i)?.clone();
        if name == self.font {
            return None;
        }
        self.font = name.clone();
        Some(SettingsAction::SetFont(name))
    }

    fn bump_size(&mut self, delta: f32) -> Option<SettingsAction> {
        let next = (self.size + delta).clamp(MIN_SIZE, MAX_SIZE);
        if (next - self.size).abs() < f32::EPSILON {
            return None;
        }
        self.size = next;
        Some(SettingsAction::SetFontSize(next))
    }

    pub fn wheel(&mut self, content: Rect, m: Measure, x: f32, y: f32, dy: f32) -> bool {
        let l = self.layout(content, m);
        let Some(panel) = l.menu else {
            return false;
        };
        if !panel.contains(x, y) || self.fonts.len() <= MENU_MAX_ROWS {
            return false;
        }
        let max = self.fonts.len() - MENU_MAX_ROWS;
        let step = if dy > 0.0 { 1 } else { usize::MAX };
        let next = if step == 1 {
            (self.font_scroll + 1).min(max)
        } else {
            self.font_scroll.saturating_sub(1)
        };
        let changed = next != self.font_scroll;
        self.font_scroll = next;
        changed
    }

    /// Keyboard for the open font menu; returns the pick, if any.
    pub fn key(&mut self, key: Key) -> Option<SettingsAction> {
        if !self.font_menu_open {
            return None;
        }
        let current = self.fonts.iter().position(|f| *f == self.font);
        match key {
            Key::Escape => {
                self.font_menu_open = false;
                None
            }
            Key::Up | Key::Down => {
                let n = self.fonts.len();
                if n == 0 {
                    return None;
                }
                let i = match (key, current) {
                    (Key::Down, Some(i)) => (i + 1).min(n - 1),
                    (Key::Up, Some(i)) => i.saturating_sub(1),
                    _ => 0,
                };
                self.scroll_row_into_view(i);
                self.pick_font(i)
            }
            Key::Enter => {
                self.font_menu_open = false;
                None
            }
            _ => None,
        }
    }

    fn scroll_row_into_view(&mut self, i: usize) {
        if i < self.font_scroll {
            self.font_scroll = i;
        } else if i >= self.font_scroll + MENU_MAX_ROWS {
            self.font_scroll = i + 1 - MENU_MAX_ROWS;
        }
    }
}

/// The preview's coloured segments cut to `max_chars` columns: what does
/// not fit ends in an ellipsis and later segments are dropped, so a narrow
/// card never has text running past it. Segment `i` of the result is a
/// prefix of segment `i` of the input.
pub fn fit_segments(segments: &[&str], max_chars: usize) -> Vec<String> {
    let total: usize = segments.iter().map(|s| s.chars().count()).sum();
    if total <= max_chars {
        return segments.iter().map(|s| s.to_string()).collect();
    }
    let mut left = max_chars.saturating_sub(1);
    let mut out = Vec::new();
    if max_chars == 0 {
        return out;
    }
    for seg in segments {
        let n = seg.chars().count();
        if n <= left {
            out.push(seg.to_string());
            left -= n;
        } else {
            let head: String = seg.chars().take(left).collect();
            out.push(format!("{head}\u{2026}"));
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::test_measure;
    use super::*;

    const CONTENT: Rect = Rect::new(260.0, 96.0, 1180.0, 804.0);

    fn center(r: Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    fn state() -> AppearanceState {
        let mut s =
            AppearanceState::new(Some("JetBrains Mono"), 14.0, CursorStyle::Block);
        s.set_fonts(
            ["Arial", "JetBrains Mono", "Fira Code", "Noto Serif", "Hack"]
                .map(String::from)
                .to_vec(),
        );
        s
    }

    #[test]
    fn font_list_keeps_monospace_and_the_configured_family() {
        let s = state();
        assert_eq!(s.fonts, vec!["Fira Code", "Hack", "JetBrains Mono"]);
        let mut t = AppearanceState::new(Some("Weird Face"), 14.0, CursorStyle::Block);
        t.set_fonts(vec!["Arial".into(), "Hack".into()]);
        assert!(t.fonts.contains(&"Weird Face".to_string()));
        let mut u = AppearanceState::default();
        u.set_fonts(vec!["Arial".into(), "Georgia".into()]);
        assert_eq!(u.fonts.len(), 2, "no monospace found: offer everything");
    }

    #[test]
    fn size_stepper_clamps_and_reports_the_new_size() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (px, py) = center(l.size_plus);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, px, py),
            Some(SettingsAction::SetFontSize(15.0))
        );
        let (mx, my) = center(l.size_minus);
        s.size = MIN_SIZE;
        assert_eq!(s.press(CONTENT, &mut test_measure, mx, my), None);
        assert_eq!(s.size, MIN_SIZE);
        s.size = MAX_SIZE;
        assert_eq!(s.press(CONTENT, &mut test_measure, px, py), None);
    }

    #[test]
    fn cursor_segments_persist_a_change_only() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.cursor.segments[2]);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SetCursor(CursorStyle::Beam))
        );
        assert_eq!(s.press(CONTENT, &mut test_measure, x, y), None);
        assert_eq!(s.cursor, CursorStyle::Beam);
    }

    #[test]
    fn theme_segments_persist_a_change_only() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        for (i, t) in [(1, ThemeChoice::Light), (2, ThemeChoice::System)] {
            let (x, y) = center(l.theme.segments[i]);
            assert_eq!(
                s.press(CONTENT, &mut test_measure, x, y),
                Some(SettingsAction::SetTheme(t))
            );
            assert_eq!(s.theme, t);
            assert_eq!(s.press(CONTENT, &mut test_measure, x, y), None);
        }
    }

    #[test]
    fn system_follows_the_os_and_defaults_to_dark() {
        assert_eq!(
            ThemeChoice::Dark.resolve(Some(ThemeMode::Light)),
            ThemeMode::Dark
        );
        assert_eq!(
            ThemeChoice::Light.resolve(Some(ThemeMode::Dark)),
            ThemeMode::Light
        );
        assert_eq!(
            ThemeChoice::System.resolve(Some(ThemeMode::Light)),
            ThemeMode::Light
        );
        assert_eq!(
            ThemeChoice::System.resolve(Some(ThemeMode::Dark)),
            ThemeMode::Dark
        );
        assert_eq!(ThemeChoice::System.resolve(None), ThemeMode::Dark);
    }

    #[test]
    fn theme_config_values_round_trip() {
        for t in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::from_config(t.config_value()), t);
        }
        assert_eq!(ThemeChoice::from_config("Solarized"), ThemeChoice::Dark);
    }

    #[test]
    fn font_menu_opens_picks_and_closes() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.font_select.box_rect);
        assert_eq!(s.press(CONTENT, &mut test_measure, x, y), None);
        assert!(s.font_menu_open);
        let l = s.layout(CONTENT, &mut test_measure);
        assert_eq!(l.menu_rows.len(), 3);
        let (x, y) = center(l.menu_rows[1].1);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SetFont("Hack".into()))
        );
        assert!(!s.font_menu_open);
        assert_eq!(s.font, "Hack");
    }

    #[test]
    fn press_outside_closes_the_menu_without_acting() {
        let mut s = state();
        s.font_menu_open = true;
        assert_eq!(
            s.press(
                CONTENT,
                &mut test_measure,
                CONTENT.x + 5.0,
                CONTENT.bottom() - 5.0
            ),
            None
        );
        assert!(!s.font_menu_open);
    }

    #[test]
    fn menu_scrolls_when_there_are_many_fonts() {
        let mut s = AppearanceState::default();
        s.set_fonts((0..20).map(|i| format!("Test Mono {i:02}")).collect());
        s.font_menu_open = true;
        let l = s.layout(CONTENT, &mut test_measure);
        assert_eq!(l.menu_rows.len(), MENU_MAX_ROWS);
        let p = l.menu.unwrap();
        assert!(s.wheel(CONTENT, &mut test_measure, p.x + 5.0, p.y + 5.0, 1.0));
        assert_eq!(s.font_scroll, 1);
        for _ in 0..30 {
            s.wheel(CONTENT, &mut test_measure, p.x + 5.0, p.y + 5.0, 1.0);
        }
        assert_eq!(s.font_scroll, 12);
        let l = s.layout(CONTENT, &mut test_measure);
        assert_eq!(l.menu_rows[0].0, 12);
    }

    #[test]
    fn arrow_keys_pick_in_the_open_menu_and_escape_closes() {
        let mut s = state();
        s.font_menu_open = true;
        assert_eq!(s.key(Key::Up), Some(SettingsAction::SetFont("Hack".into())));
        assert_eq!(s.key(Key::Escape), None);
        assert!(!s.font_menu_open);
        assert_eq!(s.key(Key::Down), None, "closed menu ignores keys");
    }

    #[test]
    fn preview_lines_are_cut_to_the_card_not_past_it() {
        let segs = ["development", "  ", "notes.md", "  ", "build.tar.gz"];
        // Fits: untouched.
        assert_eq!(fit_segments(&segs, 40), segs.map(String::from).to_vec());
        // 31 columns: the last name is cut with an ellipsis, nothing after.
        let cut = fit_segments(&segs, 31);
        assert_eq!(cut.concat(), "development  notes.md  build.t\u{2026}");
        assert_eq!(cut.concat().chars().count(), 31);
        // Cut inside an earlier segment: the rest disappears.
        assert_eq!(fit_segments(&segs, 5).concat(), "deve\u{2026}");
        assert_eq!(fit_segments(&segs, 0).concat(), "");
    }

    #[test]
    fn preview_sits_in_the_right_column() {
        let s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        assert!(l.preview.x >= l.font_select.box_rect.right() + COLUMN_GAP - 0.01);
        assert!(l.preview.right() <= CONTENT.right() - 28.0 + 0.01);
    }

    #[test]
    fn cursor_config_values_round_trip() {
        for c in CursorStyle::ALL {
            assert_eq!(CursorStyle::from_config(c.config_value()), c);
        }
        assert_eq!(CursorStyle::from_config("Hidden"), CursorStyle::Block);
    }
}
