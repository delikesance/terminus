//! User-editable chrome themes: a TOML file overrides named slots of a
//! base palette (`extends`), with `$variables` from a `[palette]` table.
//!
//! ```toml
//! [meta]
//! name = "Gruvbox Dark"
//! extends = "violet-ink"
//!
//! [palette]
//! bg0 = "#282828"
//!
//! [chrome]
//! frame = "$bg0"
//! accent = "#d65d0e"
//! ```
//!
//! Unknown tables (the terminal `[colors]` table of the same file) are
//! ignored here. The legacy [`ChromeTheme`] aliases are re-derived from the
//! tokens after the overrides, exactly as the built-in palettes map them.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use crate::theme::{contrast, rgba, unit_color, ChromeTheme};

/// Longest `extends` chain accepted (also stops cycles).
pub const MAX_EXTENDS_DEPTH: usize = 8;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeFile {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default)]
    pub palette: BTreeMap<String, String>,
    #[serde(default)]
    pub chrome: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Meta {
    pub name: Option<String>,
    /// `violet-ink`, `violet-paper`, or the name of another theme.
    pub extends: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeError {
    Syntax(String),
    BadColor { slot: String, value: String },
    UnknownVariable(String),
    UnknownSlot(String),
    UnknownBase(String),
    ExtendsTooDeep,
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(e) => write!(f, "invalid theme file: {e}"),
            Self::BadColor { slot, value } => {
                write!(f, "`{slot}`: `{value}` is not #rrggbb or #rrggbbaa")
            }
            Self::UnknownVariable(v) => write!(f, "unknown palette variable `${v}`"),
            Self::UnknownSlot(s) => write!(f, "unknown chrome slot `{s}`"),
            Self::UnknownBase(b) => write!(f, "unknown base theme `{b}`"),
            Self::ExtendsTooDeep => {
                write!(
                    f,
                    "`extends` chain deeper than {MAX_EXTENDS_DEPTH} (cycle?)"
                )
            }
        }
    }
}

impl std::error::Error for ThemeError {}

impl ThemeFile {
    pub fn parse(src: &str) -> Result<Self, ThemeError> {
        toml::from_str(src).map_err(|e| ThemeError::Syntax(e.to_string()))
    }
}

/// A theme and the non-blocking readability problems found in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub theme: ChromeTheme,
    pub warnings: Vec<String>,
}

impl ChromeTheme {
    /// Built-in base by name.
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "violet-ink" => Some(Self::violet_ink()),
            "violet-paper" => Some(Self::violet_paper()),
            _ => None,
        }
    }

    /// Apply `file` on top of its `extends` chain. `lookup` returns other
    /// themes by name (user files); built-ins need no lookup.
    pub fn from_file(
        file: &ThemeFile,
        lookup: &dyn Fn(&str) -> Option<ThemeFile>,
    ) -> Result<Resolved, ThemeError> {
        let mut chain = vec![file.clone()];
        let base = loop {
            let next = chain.last().and_then(|f| f.meta.extends.clone());
            let Some(name) = next else {
                break Self::violet_ink();
            };
            if let Some(b) = Self::builtin(&name) {
                break b;
            }
            if chain.len() >= MAX_EXTENDS_DEPTH {
                return Err(ThemeError::ExtendsTooDeep);
            }
            chain.push(lookup(&name).ok_or(ThemeError::UnknownBase(name))?);
        };
        // Root first, so the child wins.
        let mut theme = base;
        let mut palette = BTreeMap::new();
        for f in chain.iter().rev() {
            palette.extend(f.palette.clone());
            for (slot, value) in &f.chrome {
                theme.set_slot(slot, &resolve_value(value, &palette, slot)?)?;
            }
        }
        theme.derive_legacy();
        let warnings = theme.readability_warnings();
        Ok(Resolved { theme, warnings })
    }

    fn set_slot(&mut self, slot: &str, color: &[f32; 4]) -> Result<(), ThemeError> {
        let bytes = crate::theme::text_color(*color);
        macro_rules! f32_slots {
            ($($name:ident),*) => {
                match slot {
                    $(stringify!($name) => { self.$name = *color; return Ok(()); })*
                    _ => {}
                }
            };
        }
        macro_rules! u8_slots {
            ($($name:ident),*) => {
                match slot {
                    $(stringify!($name) => { self.$name = bytes; return Ok(()); })*
                    _ => {}
                }
            };
        }
        f32_slots!(
            frame,
            canvas,
            field,
            surface,
            raised,
            selected,
            line,
            divider,
            dialog,
            dialog_line,
            accent_hover,
            accent_press,
            on_accent,
            danger_fill,
            danger_hover,
            on_danger,
            danger_press,
            hover_border,
            choice_selected_bg,
            idle_ring,
            step_done_bg,
            step_failed_bg,
            info,
            warning,
            accent,
            success,
            scrim
        );
        u8_slots!(
            danger_text,
            selected_subtle_text,
            text,
            text_muted,
            text_faint,
            text_placeholder
        );
        Err(ThemeError::UnknownSlot(slot.to_string()))
    }

    /// Re-map the legacy aliases onto the tokens (same mapping as the
    /// built-in palettes; the light branch follows `is_light`).
    fn derive_legacy(&mut self) {
        let light = self.is_light();
        let tint = if light { 0.12 } else { 0.15 };
        let [r, g, b, _] = self.accent;
        let soft = [r, g, b, tint];
        self.rail_bg = self.canvas;
        self.rail_marker = self.accent;
        self.rail_active_bg = soft;
        self.panel_bg = self.frame;
        self.panel_border = self.divider;
        self.item_hover = if light { self.raised } else { self.surface };
        self.item_selected = self.selected;
        self.button_bg = self.surface;
        self.notice_bg = self.field;
        self.field_bg = self.field;
        self.field_border = self.line;
        self.field_border_focus = self.accent;
        self.dialog_bg = self.dialog;
        self.dialog_border = self.dialog_line;
        self.dialog_header = if light { self.frame } else { self.surface };
        self.shell_bg = self.frame;
        self.accent_soft = soft;
        self.terminal_bg = self.canvas;
        self.danger = self.danger_text;
    }

    /// Same thresholds as the built-in palettes' readability test.
    fn readability_warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        let surfaces = [
            ("frame", self.frame),
            ("canvas", self.canvas),
            ("surface", self.surface),
            ("raised", self.raised),
            ("selected", self.selected),
            ("dialog", self.dialog),
        ];
        let texts = [
            ("text", self.text, 7.0),
            ("text_muted", self.text_muted, 4.5),
            ("text_faint", self.text_faint, 4.5),
        ];
        for (tn, t, min) in texts {
            for (sn, s) in surfaces {
                let ratio = contrast(unit_color(t), s);
                if ratio < min {
                    out.push(format!("{tn} on {sn}: contrast {ratio:.1} < {min}"));
                }
            }
        }
        let pairs = [
            ("on_accent on accent", self.on_accent, self.accent, 4.5),
            (
                "on_danger on danger_fill",
                self.on_danger,
                self.danger_fill,
                4.5,
            ),
            (
                "danger_text on surface",
                unit_color(self.danger_text),
                self.surface,
                4.5,
            ),
            ("accent on surface", self.accent, self.surface, 3.0),
        ];
        for (name, fg, bg, min) in pairs {
            let ratio = contrast(fg, bg);
            if ratio < min {
                out.push(format!("{name}: contrast {ratio:.1} < {min}"));
            }
        }
        out
    }
}

fn resolve_value(
    value: &str,
    palette: &BTreeMap<String, String>,
    slot: &str,
) -> Result<[f32; 4], ThemeError> {
    let mut raw = value.trim();
    // Variables may point at other variables; bounded to avoid loops.
    for _ in 0..MAX_EXTENDS_DEPTH {
        let Some(name) = raw.strip_prefix('$') else {
            break;
        };
        raw = palette
            .get(name)
            .ok_or_else(|| ThemeError::UnknownVariable(name.to_string()))?
            .trim();
    }
    parse_hex(raw).ok_or_else(|| ThemeError::BadColor {
        slot: slot.to_string(),
        value: value.to_string(),
    })
}

fn parse_hex(s: &str) -> Option<[f32; 4]> {
    let h = s.strip_prefix('#')?;
    if !h.is_ascii() || !matches!(h.len(), 6 | 8) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    let alpha = if h.len() == 8 {
        byte(6)? as f32 / 255.0
    } else {
        1.0
    };
    Some(rgba([byte(0)?, byte(2)?, byte(4)?], alpha))
}

#[cfg(test)]
#[path = "theme_file_tests.rs"]
mod tests;
