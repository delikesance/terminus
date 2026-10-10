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

pub(super) fn rgba(c: [u8; 4]) -> [f32; 4] {
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
                theme.danger_press,
            )),
            fg: theme.on_danger,
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
