//! The "Add a server" dialog (add and edit): title, close, Overlays
//! stepper, the three steps (Address, Sign in, Organise) and the footer.
//!
//! Geometry and hit-testing live in `terminus_ui::add_host`; this file only
//! walks the same rects and paints them with the Input, Selection, Button
//! and Overlay components.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::add_host::{
    self as ah, AddHostForm, AddHostHit, AddHostLayout, AddHostStep, Field, Row,
    SelectMenu,
};
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonSpec, ButtonState};
use terminus_ui::components::input::{self as inp, FieldKind, FieldState};
use terminus_ui::components::overlay as ov;
use terminus_ui::components::selection::{self as sel, ControlState};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::Chrome;

use crate::renderer::chrome::{draw_icon, paint_flat, paint_surface_stroke, wrap_lines};
use crate::renderer::components::button::paint_button_on;
use crate::renderer::components::input::{paint_field, FieldContent};
use crate::renderer::components::overlay::paint_stepper;
use crate::renderer::components::selection::paint_choice;
use crate::renderer::components::Layer;
use crate::renderer::ui_text::{
    draw_ui_text, measure_mono_text, measure_ui_text, ui_opts, UiFamily, UiWeight,
};

/// Paint order inside the overlay pass: the scrim and shell sit under the
/// components (buttons 2, fields and cards 7); lists float above them.
const ORDER_SCRIM: u8 = 0;
const ORDER_SHELL: u8 = 1;
const ORDER_CLOSE: u8 = 3;
const ORDER_LIST: u8 = 30;
const DEPTH: f32 = 0.1;

const CHOICES: [(&str, &str); 3] = [
    ("SSH key", "Recommended"),
    ("Password", "Kept in your vault"),
    ("Kerberos", "GSSAPI"),
];

const VAULT_NOTE: &str = "Encrypted in your vault, never stored in plain text.";
const KEY_HINT_LEAD: &str = "No key yet? ";
const KEY_HINT_LINK: &str = "Generate one";
const KEY_HINT_TAIL: &str = " and Terminus will show you where to paste it.";
const KERBEROS_HINT: &str = "Uses your current Kerberos ticket. Nothing is stored here.";

fn rgba8(c: [u8; 4]) -> [f32; 4] {
    c.map(|v| v as f32 / 255.0)
}

/// Y that centres a text line of `size` in `rect` (same rule as the inputs).
fn text_top(rect: &Rect, size: f32) -> f32 {
    rect.y + (rect.height - size) / 2.0 - size * 0.12
}

fn covered(menu: Option<Rect>, r: &Rect) -> bool {
    menu.is_some_and(|m| terminus_ui::rects_overlap(m, *r))
}

/// Soft drop shadow under a rounded panel (layered translucent rects).
fn paint_shadow(sugarloaf: &mut Sugarloaf, r: &Rect, rad: f32, order: u8) {
    for (grow, dy, a) in [(14.0, 14.0, 0.07), (8.0, 12.0, 0.10), (3.0, 10.0, 0.14)] {
        sugarloaf.rounded_rect(
            None,
            r.x - grow,
            r.y + dy - grow,
            r.width + 2.0 * grow,
            r.height + 2.0 * grow,
            [0.0, 0.0, 0.0, a],
            DEPTH,
            rad + grow,
            order,
        );
    }
}

fn measure_text(sf: &mut Sugarloaf, mono: bool, s: &str, size: f32) -> f32 {
    if mono {
        measure_mono_text(sf, s, size, UiWeight::Regular)
    } else {
        measure_ui_text(sf, s, size, UiWeight::Regular)
    }
}

/// The part of `text` that fits `max_w`, scrolled so the caret stays in
/// view, and the caret's x offset inside it. Without a caret, long text is
/// cut with an ellipsis.
fn fit_text(
    sf: &mut Sugarloaf,
    mono: bool,
    size: f32,
    text: &str,
    caret: Option<usize>,
    max_w: f32,
) -> (String, Option<f32>) {
    let chars: Vec<char> = text.chars().collect();
    let slice = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let Some(caret) = caret.map(|c| c.min(chars.len())) else {
        if measure_text(sf, mono, text, size) <= max_w {
            return (text.to_string(), None);
        }
        let mut end = chars.len();
        while end > 0 {
            let s = format!("{}\u{2026}", slice(0, end));
            if measure_text(sf, mono, &s, size) <= max_w {
                return (s, None);
            }
            end -= 1;
        }
        return (String::new(), None);
    };
    let mut start = 0;
    while start < caret
        && measure_text(sf, mono, &slice(start, caret), size) > max_w - 2.0
    {
        start += 1;
    }
    let mut end = chars.len();
    while end > caret && measure_text(sf, mono, &slice(start, end), size) > max_w {
        end -= 1;
    }
    let caret_x = measure_text(sf, mono, &slice(start, caret), size);
    (slice(start, end), Some(caret_x))
}

/// Paint the dialog. `paint_glyphs` is false while a higher modal covers
/// it: only the scrim and shell are drawn then.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    paint_glyphs: bool,
) {
    let form = &chrome.form;
    let layout = chrome.dialog_layout(window_width, window_height);
    let dialog = layout.rect(form.height());

    paint_flat(
        sugarloaf,
        &Rect::new(0.0, 0.0, window_width, window_height),
        ov::SCRIM,
        DEPTH - 0.02,
        ORDER_SCRIM,
    );
    paint_shadow(sugarloaf, &dialog, ah::DIALOG_RADIUS, ORDER_SHELL);
    paint_surface_stroke(
        sugarloaf,
        &dialog,
        theme.dialog,
        Some(theme.dialog_line),
        ah::DIALOG_RADIUS,
        1.0,
        DEPTH + 0.02,
        ORDER_SHELL,
        false,
    );
    if !paint_glyphs {
        return;
    }

    let menu = layout.menu_rect(form);
    paint_header(sugarloaf, form, &layout, theme, device_scale);
    paint_stepper(
        sugarloaf,
        theme,
        layout.stepper_rect(),
        form.step().index() + 1,
    );

    match form.step() {
        AddHostStep::Target | AddHostStep::Details => {
            for field in form.visible_fields() {
                paint_form_field(
                    sugarloaf,
                    form,
                    &layout,
                    theme,
                    field,
                    menu,
                    device_scale,
                );
            }
        }
        AddHostStep::Auth => {
            paint_sign_in(sugarloaf, form, &layout, theme, menu, device_scale)
        }
    }

    paint_footer(sugarloaf, form, &layout, theme, menu);
    if menu.is_some() {
        paint_menu(sugarloaf, form, &layout, theme, device_scale);
    }
}

fn paint_header(
    sugarloaf: &mut Sugarloaf,
    form: &AddHostForm,
    layout: &AddHostLayout,
    theme: &ChromeTheme,
    device_scale: f32,
) {
    let title = layout.title_rect();
    let text = if form.is_editing() {
        "Edit server"
    } else {
        "Add a server"
    };
    draw_ui_text(
        sugarloaf,
        title.x,
        title.y + (title.height - ah::TITLE_FONT * 1.25) / 2.0,
        text,
        ah::TITLE_FONT,
        theme.text,
        UiWeight::SemiBold,
    );
    let close = layout.close_button_rect();
    if form.hover() == Some(AddHostHit::Close) {
        sugarloaf.rounded_rect(
            None,
            close.x,
            close.y,
            close.width,
            close.height,
            theme.surface,
            DEPTH,
            9.0,
            ORDER_CLOSE,
        );
    }
    let icon = 15.0;
    draw_icon(
        sugarloaf,
        Icon::X,
        IconPlacement::new(
            close.x + (close.width - icon) / 2.0,
            close.y + (close.height - icon) / 2.0,
            icon,
        ),
        rgba8(theme.text_muted),
        device_scale,
    );
}

fn field_state(form: &AddHostForm, field: Field) -> FieldState {
    if form.error_field() == Some(field) {
        FieldState::Error
    } else if form.focused_field() == field {
        FieldState::Focus
    } else if form.hover() == Some(AddHostHit::Field(field)) {
        FieldState::Hover
    } else if form.value(field).is_empty() && field != Field::Password {
        FieldState::Default
    } else {
        FieldState::Filled
    }
}

/// One Input component for `field`, with the popover (if any) cut out of
/// its text so nothing floats over the list.
fn paint_form_field(
    sugarloaf: &mut Sugarloaf,
    form: &AddHostForm,
    layout: &AddHostLayout,
    theme: &ChromeTheme,
    field: Field,
    menu: Option<Rect>,
    device_scale: f32,
) {
    let Some(fl) = layout.field_layout(form, field) else {
        return;
    };
    let kind = field.kind();
    let focused = form.focused_field() == field;
    let mono = matches!(kind, FieldKind::Mono | FieldKind::Textarea);
    let font = kind.value_font();

    let value_hidden = covered(menu, &fl.text);
    let mut shown: String;
    let mut caret_w = None;
    let mut placeholder = field.placeholder();
    match field {
        Field::Identity => {
            shown = form.selected_identity_name().unwrap_or("").to_string();
        }
        Field::Group => {
            shown = form.selected_group_name().unwrap_or("").to_string();
            // "No group" reads as a value, not as a hint.
            if shown.is_empty() {
                shown = "No group".to_string();
            }
        }
        Field::Password => {
            if form.is_editing() {
                placeholder = "Leave blank to keep current";
            }
            let chars = form.password().chars().count();
            let full: String = if form.password_visible() {
                form.password().to_string()
            } else {
                inp::mask(chars)
            };
            let caret = focused.then(|| form.cursor(Field::Password));
            let (text, cx) =
                fit_text(sugarloaf, false, font, &full, caret, fl.text.width);
            shown = text;
            caret_w = cx;
        }
        _ => {
            let value = form.value(field);
            let caret = focused.then(|| form.cursor(field));
            let (text, cx) = fit_text(sugarloaf, mono, font, value, caret, fl.text.width);
            shown = text;
            caret_w = cx;
        }
    }
    if matches!(field, Field::Identity | Field::Group) {
        let (text, _) = fit_text(sugarloaf, false, font, &shown, None, fl.text.width);
        shown = text;
    }

    let placeholder_fit;
    let placeholder = if shown.is_empty() {
        placeholder_fit =
            fit_text(sugarloaf, mono, font, placeholder, None, fl.text.width).0;
        placeholder_fit.as_str()
    } else {
        placeholder
    };
    let label = (!covered(menu, &fl.label.unwrap_or(fl.box_rect)))
        .then(|| field.label())
        .filter(|_| fl.label.is_some());
    let helper = fl.helper.filter(|h| !covered(menu, h)).map(|_| VAULT_NOTE);
    let content = FieldContent {
        kind,
        state: field_state(form, field),
        label,
        value: if value_hidden { "" } else { &shown },
        placeholder: if value_hidden { "" } else { placeholder },
        helper,
        revealed: form.password_visible(),
        caret_prefix_width: caret_w.filter(|_| !value_hidden),
    };
    paint_field(sugarloaf, theme, &fl, &content, theme.dialog, device_scale);
}

fn paint_sign_in(
    sugarloaf: &mut Sugarloaf,
    form: &AddHostForm,
    layout: &AddHostLayout,
    theme: &ChromeTheme,
    menu: Option<Rect>,
    device_scale: f32,
) {
    if let (Some(label), Some(cards)) =
        (layout.choices_label_rect(form), layout.choice_rects(form))
    {
        draw_ui_text(
            sugarloaf,
            label.x,
            text_top(&label, inp::LABEL_FONT),
            Field::AuthMethod.label(),
            inp::LABEL_FONT,
            theme.text,
            UiWeight::Medium,
        );
        let focused = form.focused_field() == Field::AuthMethod;
        for (i, rect) in cards.iter().enumerate() {
            let selected = form.auth_method() == ah::AUTH_METHODS[i];
            let state = if selected {
                ControlState::Selected
            } else if form.hover() == Some(AddHostHit::SelectAuth(i)) {
                ControlState::Hover
            } else {
                ControlState::Default
            };
            if selected && focused {
                let (_, outer) = sel::focus_ring_rects(rect);
                let ring = [theme.accent[0], theme.accent[1], theme.accent[2], 0.45];
                sugarloaf.rounded_rect(
                    None,
                    outer.x,
                    outer.y,
                    outer.width,
                    outer.height,
                    ring,
                    DEPTH,
                    sel::CHOICE_RADIUS + 4.0,
                    6,
                );
            }
            let (title, sub) = CHOICES[i];
            paint_choice(sugarloaf, theme, rect, title, sub, state);
        }
    }
    for field in form.visible_fields() {
        if field != Field::AuthMethod {
            paint_form_field(sugarloaf, form, layout, theme, field, menu, device_scale);
        }
    }
    if let Some(hint) = layout.hint_rect(form) {
        if covered(menu, &hint) {
            return;
        }
        let y = text_top(&hint, inp::LABEL_FONT);
        match form
            .rows()
            .iter()
            .find(|r| matches!(r, Row::KeyHint | Row::KerberosHint))
        {
            Some(Row::KeyHint) => {
                let mut x = hint.x;
                x += draw_ui_text(
                    sugarloaf,
                    x,
                    y,
                    KEY_HINT_LEAD,
                    inp::LABEL_FONT,
                    theme.text_muted,
                    UiWeight::Regular,
                );
                let hovered = form.hover() == Some(AddHostHit::GenerateKey);
                let link = draw_ui_text(
                    sugarloaf,
                    x,
                    y,
                    KEY_HINT_LINK,
                    inp::LABEL_FONT,
                    if hovered {
                        theme.accent_hover_u8()
                    } else {
                        color_u8(theme.accent)
                    },
                    UiWeight::Medium,
                );
                paint_flat(
                    sugarloaf,
                    &Rect::new(x, hint.y + hint.height - 2.0, link, 1.0),
                    theme.accent,
                    DEPTH + 0.05,
                    ORDER_LIST - 20,
                );
                draw_ui_text(
                    sugarloaf,
                    x + link,
                    y,
                    KEY_HINT_TAIL,
                    inp::LABEL_FONT,
                    theme.text_muted,
                    UiWeight::Regular,
                );
            }
            _ => {
                draw_ui_text(
                    sugarloaf,
                    hint.x,
                    y,
                    KERBEROS_HINT,
                    inp::LABEL_FONT,
                    theme.text_muted,
                    UiWeight::Regular,
                );
            }
        }
    }
}

fn color_u8(c: [f32; 4]) -> [u8; 4] {
    c.map(|v| (v * 255.0).round() as u8)
}

trait AccentHover {
    fn accent_hover_u8(&self) -> [u8; 4];
}

impl AccentHover for ChromeTheme {
    fn accent_hover_u8(&self) -> [u8; 4] {
        color_u8(self.accent_hover)
    }
}

fn paint_footer(
    sugarloaf: &mut Sugarloaf,
    form: &AddHostForm,
    layout: &AddHostLayout,
    theme: &ChromeTheme,
    menu: Option<Rect>,
) {
    let text = layout.footer_text_rect(form);
    if !covered(menu, &text) {
        let (message, color) = match form.error() {
            Some(error) => (error.to_string(), theme.danger_text),
            None => (form.step_hint(), theme.text_faint),
        };
        let size = inp::LABEL_FONT;
        let opts = ui_opts(UiFamily::Sans, size, color, UiWeight::Regular);
        let lines = wrap_lines(sugarloaf, &message, text.width, &opts, 2);
        let line_h = size * 1.35;
        let top = text.y + (text.height - line_h * lines.len() as f32) / 2.0;
        for (i, line) in lines.iter().enumerate() {
            draw_ui_text(
                sugarloaf,
                text.x,
                top + i as f32 * line_h,
                line,
                size,
                color,
                UiWeight::Regular,
            );
        }
    }

    let secondary_hit = if form.step() == AddHostStep::Target {
        AddHostHit::Cancel
    } else {
        AddHostHit::Back
    };
    let primary_hit = if form.step() == AddHostStep::Details {
        AddHostHit::Connect
    } else {
        AddHostHit::Next
    };
    for (rect, kind, label, hit) in [
        (
            layout.secondary_button_rect(form),
            ButtonKind::Secondary,
            form.secondary_label(),
            secondary_hit,
        ),
        (
            layout.primary_button_rect(form),
            ButtonKind::Primary,
            form.primary_label(),
            primary_hit,
        ),
    ] {
        let pad = ButtonSize::Large.padding_x();
        let spec = ButtonSpec::label(
            (rect.x, rect.y),
            kind,
            ButtonSize::Large,
            rect.width - 2.0 * pad,
            false,
        );
        let state = ButtonState::resolve(form.hover() == Some(hit), false, false, false);
        // The component draws the fill; the label is centred here because
        // the width comes from the layout, not from the measured label.
        paint_button_on(
            sugarloaf,
            theme,
            &spec,
            state,
            "",
            None,
            Layer {
                order: 2,
                depth: DEPTH,
                backdrop: theme.dialog,
            },
        );
        if covered(menu, &rect) {
            continue;
        }
        let weight = if kind.semibold() {
            UiWeight::SemiBold
        } else {
            UiWeight::Medium
        };
        let size = ButtonSize::Large.font_size();
        let w = measure_ui_text(sugarloaf, label, size, weight);
        let c = terminus_ui::components::button::colors(theme, kind, state);
        draw_ui_text(
            sugarloaf,
            rect.x + (rect.width - w) / 2.0,
            text_top(&rect, size),
            label,
            size,
            terminus_ui::theme::text_color(c.fg),
            weight,
        );
    }
}

/// The open select's list: floats above the dialog.
fn paint_menu(
    sugarloaf: &mut Sugarloaf,
    form: &AddHostForm,
    layout: &AddHostLayout,
    theme: &ChromeTheme,
    device_scale: f32,
) {
    let Some(rect) = layout.menu_rect(form) else {
        return;
    };
    paint_shadow(sugarloaf, &rect, 12.0, ORDER_LIST);
    paint_surface_stroke(
        sugarloaf,
        &rect,
        theme.surface,
        Some(theme.line),
        12.0,
        1.0,
        DEPTH + 0.02,
        ORDER_LIST,
        false,
    );
    let menu = form.menu();
    if menu == Some(SelectMenu::Identity) && form.identities().is_empty() {
        let row = Rect::new(rect.x + 4.0, rect.y + 4.0, rect.width - 8.0, ah::MENU_ROW);
        draw_ui_text(
            sugarloaf,
            row.x + 10.0,
            text_top(&row, 14.0),
            "No saved keys",
            14.0,
            theme.text_muted,
            UiWeight::Regular,
        );
        return;
    }
    for i in 0..form.menu_len() {
        let Some(row) = layout.menu_option_rect(form, i) else {
            continue;
        };
        let (label, selected): (&str, bool) = match menu {
            Some(SelectMenu::Group) => {
                if i == 0 {
                    ("No group", form.group_id().is_none())
                } else {
                    let (id, name) = &form.groups()[i - 1];
                    (name.as_str(), form.group_id() == Some(id.as_str()))
                }
            }
            _ => {
                let (id, name) = &form.identities()[i];
                (name.as_str(), form.identity_id() == Some(id.as_str()))
            }
        };
        if form.menu_hover() == Some(i) {
            sugarloaf.rounded_rect(
                None,
                row.x,
                row.y,
                row.width,
                row.height,
                theme.selected,
                DEPTH + 0.05,
                ov::MENU_ITEM_RADIUS,
                ORDER_LIST,
            );
        }
        draw_ui_text(
            sugarloaf,
            row.x + 10.0,
            text_top(&row, 14.0),
            label,
            14.0,
            if selected {
                color_u8(theme.accent)
            } else {
                theme.text
            },
            if selected {
                UiWeight::Medium
            } else {
                UiWeight::Regular
            },
        );
        if selected {
            let icon = 15.0;
            draw_icon(
                sugarloaf,
                Icon::Check,
                IconPlacement::new(
                    row.right() - 10.0 - icon,
                    row.y + (row.height - icon) / 2.0,
                    icon,
                ),
                theme.accent,
                device_scale,
            );
        }
    }
}
