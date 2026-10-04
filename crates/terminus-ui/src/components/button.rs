//! Buttons: five kinds x three sizes x five states, geometry and hit-testing.
//!
//! No fonts here: the caller measures the label and passes the width in.
//! The painter walks the same rects this module returns for hit-testing.

use crate::geom::Rect;
use crate::theme::ChromeTheme;
use crate::tokens::{font_size, height, radius};

/// Gap between a leading icon and the label.
pub const ICON_GAP: f32 = 8.0;
/// Opacity of a disabled button.
pub const DISABLED_OPACITY: f32 = 0.4;
/// Gap (canvas colour) between a focused button and its ring.
pub const FOCUS_GAP: f32 = 2.0;
/// Accent ring thickness.
pub const FOCUS_RING: f32 = 2.0;

/// Text and icon colour on a danger fill (`#1A0B08`).
pub const ON_DANGER: [f32; 4] = [26.0 / 255.0, 11.0 / 255.0, 8.0 / 255.0, 1.0];
/// Danger fill while pressed (`#D2503F`).
pub const DANGER_PRESS: [f32; 4] = [210.0 / 255.0, 80.0 / 255.0, 63.0 / 255.0, 1.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Danger,
    /// Accent-coloured text button ("ghost" on the board).
    Text,
    Quiet,
}

impl ButtonKind {
    pub const ALL: [ButtonKind; 5] = [
        ButtonKind::Primary,
        ButtonKind::Secondary,
        ButtonKind::Danger,
        ButtonKind::Text,
        ButtonKind::Quiet,
    ];

    /// Primary and Danger labels are SemiBold, the rest Medium.
    pub fn semibold(self) -> bool {
        matches!(self, ButtonKind::Primary | ButtonKind::Danger)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    Large,
    Medium,
    Small,
}

impl ButtonSize {
    pub const ALL: [ButtonSize; 3] =
        [ButtonSize::Large, ButtonSize::Medium, ButtonSize::Small];

    pub fn height(self) -> f32 {
        match self {
            ButtonSize::Large => height::CONTROL_LG,
            ButtonSize::Medium => height::CONTROL_MD,
            ButtonSize::Small => height::CONTROL_SM,
        }
    }

    pub fn radius(self) -> f32 {
        match self {
            ButtonSize::Large => radius::CONTROL,
            ButtonSize::Medium => radius::SMALL,
            ButtonSize::Small => 7.0,
        }
    }

    pub fn padding_x(self) -> f32 {
        match self {
            ButtonSize::Large => 20.0,
            ButtonSize::Medium => 14.0,
            ButtonSize::Small => 10.0,
        }
    }

    pub fn font_size(self) -> f32 {
        match self {
            ButtonSize::Large => font_size::BODY_SM,
            ButtonSize::Medium => font_size::LABEL,
            ButtonSize::Small => font_size::CAPTION,
        }
    }

    /// Leading icon size next to a label.
    pub fn lead_icon(self) -> f32 {
        if self == ButtonSize::Small {
            13.0
        } else {
            15.0
        }
    }

    /// Icon size inside an icon-only button.
    pub fn solo_icon(self) -> f32 {
        if self == ButtonSize::Small {
            14.0
        } else {
            16.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Default,
    Hover,
    Pressed,
    Focus,
    Disabled,
}

impl ButtonState {
    pub const ALL: [ButtonState; 5] = [
        ButtonState::Default,
        ButtonState::Hover,
        ButtonState::Pressed,
        ButtonState::Focus,
        ButtonState::Disabled,
    ];

    /// Disabled beats everything; then pressed, hover, focus.
    pub fn resolve(hovered: bool, pressed: bool, focused: bool, disabled: bool) -> Self {
        if disabled {
            ButtonState::Disabled
        } else if pressed {
            ButtonState::Pressed
        } else if hovered {
            ButtonState::Hover
        } else if focused {
            ButtonState::Focus
        } else {
            ButtonState::Default
        }
    }

    /// Opacity applied to the whole button.
    pub fn opacity(self) -> f32 {
        if self == ButtonState::Disabled {
            DISABLED_OPACITY
        } else {
            1.0
        }
    }

    /// Focus ring is drawn for [`ButtonState::Focus`] only.
    pub fn shows_focus_ring(self) -> bool {
        self == ButtonState::Focus
    }
}

/// Fill (None = transparent) and foreground (text and icon) of a button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonColors {
    pub fill: Option<[f32; 4]>,
    pub fg: [f32; 4],
}

fn rgba(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// Colours for `kind` in `state` (opacity not applied). Focus and
/// Disabled use the default colours.
pub fn colors(theme: &ChromeTheme, kind: ButtonKind, state: ButtonState) -> ButtonColors {
    let (h, p) = (state == ButtonState::Hover, state == ButtonState::Pressed);
    fn pick<T>(h: bool, p: bool, d: T, hv: T, pr: T) -> T {
        if p {
            pr
        } else if h {
            hv
        } else {
            d
        }
    }
    match kind {
        ButtonKind::Primary => ButtonColors {
            fill: Some(pick(
                h,
                p,
                theme.accent,
                theme.accent_hover,
                theme.accent_press,
            )),
            fg: theme.on_accent,
        },
        ButtonKind::Secondary => ButtonColors {
            fill: Some(pick(h, p, theme.raised, theme.selected, theme.line)),
            fg: rgba(theme.text),
        },
        ButtonKind::Danger => ButtonColors {
            fill: Some(pick(
                h,
                p,
                theme.danger_fill,
                theme.danger_hover,
                DANGER_PRESS,
            )),
            fg: ON_DANGER,
        },
        ButtonKind::Text => ButtonColors {
            fill: pick(h, p, None, Some(theme.surface), Some(theme.raised)),
            fg: pick(h, p, theme.accent, theme.accent_hover, theme.accent),
        },
        ButtonKind::Quiet => ButtonColors {
            fill: pick(h, p, None, Some(theme.surface), Some(theme.raised)),
            fg: pick(
                h,
                p,
                rgba(theme.text_muted),
                rgba(theme.text),
                rgba(theme.text),
            ),
        },
    }
}

/// What a button shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonContent {
    /// Label (measured width) with an optional leading icon.
    Label { label_width: f32, icon: bool },
    /// Square icon-only button; the accessible name is the tooltip.
    IconOnly,
}

/// Geometry of one button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonSpec {
    pub origin: (f32, f32),
    pub kind: ButtonKind,
    pub size: ButtonSize,
    pub content: ButtonContent,
}

impl ButtonSpec {
    pub fn label(
        origin: (f32, f32),
        kind: ButtonKind,
        size: ButtonSize,
        label_width: f32,
        icon: bool,
    ) -> Self {
        Self {
            origin,
            kind,
            size,
            content: ButtonContent::Label { label_width, icon },
        }
    }

    pub fn icon_only(origin: (f32, f32), kind: ButtonKind, size: ButtonSize) -> Self {
        Self {
            origin,
            kind,
            size,
            content: ButtonContent::IconOnly,
        }
    }

    pub fn width(&self) -> f32 {
        match self.content {
            ButtonContent::IconOnly => self.size.height(),
            ButtonContent::Label { label_width, icon } => {
                let lead = if icon {
                    self.size.lead_icon() + ICON_GAP
                } else {
                    0.0
                };
                2.0 * self.size.padding_x() + lead + label_width
            }
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(
            self.origin.0,
            self.origin.1,
            self.width(),
            self.size.height(),
        )
    }

    pub fn radius(&self) -> f32 {
        self.size.radius()
    }

    /// Icon box (leading icon, or the centred icon of an icon-only button).
    pub fn icon_rect(&self) -> Option<Rect> {
        let r = self.rect();
        match self.content {
            ButtonContent::IconOnly => {
                let s = self.size.solo_icon();
                Some(Rect::new(
                    r.x + (r.width - s) * 0.5,
                    r.y + (r.height - s) * 0.5,
                    s,
                    s,
                ))
            }
            ButtonContent::Label { icon: true, .. } => {
                let s = self.size.lead_icon();
                Some(Rect::new(
                    r.x + self.size.padding_x(),
                    r.y + (r.height - s) * 0.5,
                    s,
                    s,
                ))
            }
            ButtonContent::Label { icon: false, .. } => None,
        }
    }

    /// Top-left of the label text box (font-size tall, vertically centred).
    pub fn label_origin(&self) -> Option<(f32, f32)> {
        let ButtonContent::Label { icon, .. } = self.content else {
            return None;
        };
        let r = self.rect();
        let lead = if icon {
            self.size.lead_icon() + ICON_GAP
        } else {
            0.0
        };
        Some((
            r.x + self.size.padding_x() + lead,
            r.y + (r.height - self.size.font_size()) * 0.5,
        ))
    }

    /// Canvas-coloured gap ring drawn under the accent ring.
    pub fn focus_gap_rect(&self) -> Rect {
        grow(self.rect(), FOCUS_GAP)
    }

    /// Outer edge of the accent focus ring.
    pub fn focus_ring_rect(&self) -> Rect {
        grow(self.rect(), FOCUS_GAP + FOCUS_RING)
    }

    pub fn focus_gap_radius(&self) -> f32 {
        self.radius() + FOCUS_GAP
    }

    pub fn focus_ring_radius(&self) -> f32 {
        self.radius() + FOCUS_GAP + FOCUS_RING
    }

    /// Pointer hit test; a disabled button is never hit.
    pub fn hit_test(&self, x: f32, y: f32, disabled: bool) -> bool {
        !disabled && self.rect().contains(x, y)
    }

    /// Tooltip text: the name of an icon-only button, nothing otherwise.
    pub fn tooltip<'a>(&self, name: &'a str) -> Option<&'a str> {
        matches!(self.content, ButtonContent::IconOnly).then_some(name)
    }
}

fn grow(r: Rect, by: f32) -> Rect {
    Rect::new(r.x - by, r.y - by, r.width + 2.0 * by, r.height + 2.0 * by)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lbl(size: ButtonSize, icon: bool) -> ButtonSpec {
        ButtonSpec::label((10.0, 20.0), ButtonKind::Primary, size, 60.0, icon)
    }

    #[test]
    fn size_metrics_match_board() {
        let m = |s: ButtonSize| (s.height(), s.radius(), s.padding_x(), s.font_size());
        assert_eq!(m(ButtonSize::Large), (44.0, 10.0, 20.0, 14.0));
        assert_eq!(m(ButtonSize::Medium), (36.0, 8.0, 14.0, 13.0));
        assert_eq!(m(ButtonSize::Small), (30.0, 7.0, 10.0, 12.0));
    }

    #[test]
    fn weights_semibold_for_primary_and_danger_only() {
        let semi: Vec<_> = ButtonKind::ALL.iter().map(|k| k.semibold()).collect();
        assert_eq!(semi, [true, false, true, false, false]);
    }

    #[test]
    fn state_resolution_priority() {
        use ButtonState::*;
        assert_eq!(ButtonState::resolve(false, false, false, false), Default);
        assert_eq!(ButtonState::resolve(true, false, false, false), Hover);
        assert_eq!(ButtonState::resolve(true, true, false, false), Pressed);
        assert_eq!(ButtonState::resolve(false, false, true, false), Focus);
        assert_eq!(ButtonState::resolve(true, false, true, false), Hover);
        assert_eq!(ButtonState::resolve(true, true, true, true), Disabled);
        assert_eq!(Disabled.opacity(), 0.4);
        assert_eq!(Default.opacity(), 1.0);
        assert!(Focus.shows_focus_ring() && !Hover.shows_focus_ring());
    }

    #[test]
    fn width_is_padding_plus_label_plus_icon() {
        assert_eq!(lbl(ButtonSize::Large, false).width(), 40.0 + 60.0);
        assert_eq!(
            lbl(ButtonSize::Medium, true).width(),
            28.0 + 15.0 + 8.0 + 60.0
        );
        assert_eq!(
            lbl(ButtonSize::Small, true).width(),
            20.0 + 13.0 + 8.0 + 60.0
        );
        let r = lbl(ButtonSize::Large, false).rect();
        assert_eq!((r.x, r.y, r.height), (10.0, 20.0, 44.0));
    }

    #[test]
    fn label_origin_follows_icon() {
        let s = lbl(ButtonSize::Large, true);
        let ic = s.icon_rect().unwrap();
        assert_eq!(ic.x, 30.0);
        assert_eq!(ic.y, 20.0 + (44.0 - 15.0) / 2.0);
        let (lx, ly) = s.label_origin().unwrap();
        assert_eq!(lx, 30.0 + 15.0 + 8.0);
        assert_eq!(ly, 20.0 + (44.0 - 14.0) / 2.0);
        let plain = lbl(ButtonSize::Large, false);
        assert!(plain.icon_rect().is_none());
        assert_eq!(plain.label_origin().unwrap().0, 30.0);
    }

    #[test]
    fn icon_only_is_square_and_centred() {
        for (size, solo) in [
            (ButtonSize::Medium, 16.0),
            (ButtonSize::Small, 14.0),
            (ButtonSize::Large, 16.0),
        ] {
            let s = ButtonSpec::icon_only((0.0, 0.0), ButtonKind::Quiet, size);
            let r = s.rect();
            assert_eq!(r.width, r.height);
            assert_eq!(r.height, size.height());
            let ic = s.icon_rect().unwrap();
            assert_eq!(ic.width, solo);
            assert_eq!(ic.x - r.x, r.right() - ic.right());
            assert_eq!(ic.y - r.y, r.bottom() - ic.bottom());
            assert!(s.label_origin().is_none());
            assert_eq!(s.tooltip("Close"), Some("Close"));
        }
        assert_eq!(lbl(ButtonSize::Large, false).tooltip("x"), None);
    }

    #[test]
    fn focus_ring_is_two_gap_plus_two_ring() {
        let s = lbl(ButtonSize::Medium, false);
        let r = s.rect();
        let gap = s.focus_gap_rect();
        let ring = s.focus_ring_rect();
        assert_eq!((gap.x, gap.width), (r.x - 2.0, r.width + 4.0));
        assert_eq!((ring.x, ring.height), (r.x - 4.0, r.height + 8.0));
        assert_eq!(s.focus_gap_radius(), 10.0);
        assert_eq!(s.focus_ring_radius(), 12.0);
    }

    #[test]
    fn hit_test_half_open_and_disabled() {
        let s = lbl(ButtonSize::Large, false);
        let r = s.rect();
        assert!(s.hit_test(r.x, r.y, false));
        assert!(s.hit_test(r.right() - 0.1, r.bottom() - 0.1, false));
        assert!(!s.hit_test(r.right(), r.y, false));
        assert!(!s.hit_test(r.x, r.y - 0.1, false));
        assert!(!s.hit_test(r.x + 5.0, r.y + 5.0, true));
    }

    #[test]
    fn colors_match_board() {
        let t = ChromeTheme::violet_ink();
        let c = |k, s| colors(&t, k, s);
        use ButtonKind::*;
        use ButtonState::*;
        assert_eq!(c(Primary, Default).fill, Some(t.accent));
        assert_eq!(c(Primary, Hover).fill, Some(t.accent_hover));
        assert_eq!(c(Primary, Pressed).fill, Some(t.accent_press));
        assert_eq!(c(Primary, Focus), c(Primary, Default));
        assert_eq!(c(Primary, Disabled), c(Primary, Default));
        assert_eq!(c(Primary, Default).fg, t.on_accent);
        assert_eq!(c(Secondary, Default).fill, Some(t.raised));
        assert_eq!(c(Secondary, Hover).fill, Some(t.selected));
        assert_eq!(c(Secondary, Pressed).fill, Some(t.line));
        assert_eq!(c(Danger, Default).fill, Some(t.danger_fill));
        assert_eq!(c(Danger, Hover).fill, Some(t.danger_hover));
        assert_eq!(c(Danger, Pressed).fill, Some(DANGER_PRESS));
        assert_eq!(c(Danger, Default).fg, ON_DANGER);
        assert_eq!(c(Text, Default).fill, None);
        assert_eq!(c(Text, Default).fg, t.accent);
        assert_eq!(c(Text, Hover).fill, Some(t.surface));
        assert_eq!(c(Text, Hover).fg, t.accent_hover);
        assert_eq!(c(Text, Pressed).fill, Some(t.raised));
        assert_eq!(c(Text, Pressed).fg, t.accent);
        assert_eq!(c(Quiet, Default).fill, None);
        assert_eq!(c(Quiet, Default).fg, rgba(t.text_muted));
        assert_eq!(c(Quiet, Hover).fill, Some(t.surface));
        assert_eq!(c(Quiet, Hover).fg, rgba(t.text));
        assert_eq!(c(Quiet, Pressed).fill, Some(t.raised));
    }
}
