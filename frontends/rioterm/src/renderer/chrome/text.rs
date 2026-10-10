// Copyright (c) 2026-present, Terminus Contributors.

use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

/// Chrome text options: Sora (semibold when `bold`, else regular).
/// Weight comes from the face, never from synthetic bold.
pub(super) fn opts(size: f32, color: [u8; 4], bold: bool) -> DrawOpts {
    use crate::renderer::ui_text::{ui_opts, UiFamily, UiWeight};
    let weight = if bold {
        UiWeight::SemiBold
    } else {
        UiWeight::Regular
    };
    ui_opts(UiFamily::Sans, size, color, weight)
}

pub(super) fn draw_text(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: [u8; 4],
    bold: bool,
) {
    if text.is_empty() {
        return;
    }
    sugarloaf
        .text_mut()
        .draw(x, y, text, &opts(size, color, bold));
}

/// Truncate `text` to fit `max_width`, ending in an ellipsis.
///
/// Measured with the real shaper, so a proportional font cannot
/// overflow its panel the way a character count would allow.
pub(super) fn elide(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    max_width: f32,
    opts: &DrawOpts,
) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if sugarloaf.text_mut().measure(text, opts) <= max_width {
        return text.to_string();
    }
    let mut kept = String::new();
    for ch in text.chars() {
        let mut candidate = kept.clone();
        candidate.push(ch);
        candidate.push('\u{2026}');
        if sugarloaf.text_mut().measure(&candidate, opts) > max_width {
            break;
        }
        kept.push(ch);
    }
    kept.push('\u{2026}');
    kept
}

/// Word-wrap `text` into at most `max_lines` lines that fit `max_width`.
/// The final line is elided when the message still overflows.
pub(crate) fn wrap_lines(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    max_width: f32,
    opts: &DrawOpts,
    max_lines: usize,
) -> Vec<String> {
    if max_lines == 0 || max_width <= 0.0 {
        return Vec::new();
    }
    if sugarloaf.text_mut().measure(text, opts) <= max_width {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![elide(sugarloaf, text, max_width, opts)];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut idx = 0;
    while idx < words.len() && lines.len() < max_lines {
        let last_line = lines.len() + 1 == max_lines;
        if last_line {
            let rest = words[idx..].join(" ");
            lines.push(elide(sugarloaf, &rest, max_width, opts));
            break;
        }
        let mut current = String::new();
        while idx < words.len() {
            let candidate = if current.is_empty() {
                words[idx].to_string()
            } else {
                format!("{} {}", current, words[idx])
            };
            if sugarloaf.text_mut().measure(&candidate, opts) <= max_width {
                current = candidate;
                idx += 1;
            } else {
                break;
            }
        }
        if current.is_empty() {
            lines.push(elide(sugarloaf, words[idx], max_width, opts));
            idx += 1;
        } else {
            lines.push(current);
        }
    }
    lines
}
