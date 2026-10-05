//! Minimal TOML line editor for the Terminus config file.
//!
//! Appearance and Updates persist by rewriting one `key = value` line of the
//! file the app already hot-reloads. Nothing else in the file (comments,
//! other sections, ordering) is touched, so a hand-edited config survives.

/// Return `src` with `key = value` set inside `[section]`. `value` is a TOML
/// literal (`16`, `true`, `"block"`). A missing key is added to the section,
/// a missing section is appended.
pub fn set_value(src: &str, section: &str, key: &str, value: &str) -> String {
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = src.split_inclusive('\n').map(str::to_string).collect();

    let mut in_section = false;
    let mut header_idx = None;
    let mut last_entry = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if let Some(name) = header_name(t) {
            if in_section {
                break;
            }
            in_section = name == section;
            if in_section {
                header_idx = Some(i);
                last_entry = Some(i);
            }
            continue;
        }
        if !in_section || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if is_key_line(t, key) {
            lines[i] = replace_line(line, key, value, eol);
            return lines.concat();
        }
        last_entry = Some(i);
    }

    if header_idx.is_some() {
        let at = last_entry.unwrap_or(0);
        if !lines[at].ends_with('\n') {
            lines[at].push_str(eol);
        }
        lines.insert(at + 1, format!("{key} = {value}{eol}"));
        return lines.concat();
    }

    let mut out = lines.concat();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push_str(eol);
        }
        out.push_str(eol);
    }
    out.push_str(&format!("[{section}]{eol}{key} = {value}{eol}"));
    out
}

/// TOML basic-string literal for `s`.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `[name]` (not `[[array]]`, not a comment) -> `name`.
fn header_name(t: &str) -> Option<&str> {
    let inner = t.strip_prefix('[')?;
    if inner.starts_with('[') {
        return Some("");
    }
    let end = inner.find(']')?;
    Some(inner[..end].trim())
}

fn is_key_line(t: &str, key: &str) -> bool {
    t.strip_prefix(key)
        .map(|rest| rest.trim_start().starts_with('='))
        .unwrap_or(false)
}

/// Rewrite one `key = old # comment` line, keeping indent, comment and EOL.
fn replace_line(line: &str, key: &str, value: &str, eol: &str) -> String {
    let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let body = line.trim_end_matches(['\r', '\n']);
    let eq = body.find('=').unwrap_or(body.len());
    let rest = &body[(eq + 1).min(body.len())..];
    let mut in_str = false;
    let mut comment = None;
    for (i, c) in rest.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '#' if !in_str => {
                comment = Some(&rest[i..]);
                break;
            }
            _ => {}
        }
    }
    let ending = if line.ends_with('\n') { eol } else { "" };
    match comment {
        Some(c) => format!("{indent}{key} = {value} {c}{ending}"),
        None => format!("{indent}{key} = {value}{ending}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_key_in_section() {
        let src = "[fonts]\nsize = 16\n\n[colors]\nbackground = \"#000\"\n";
        let out = set_value(src, "fonts", "size", "18");
        assert_eq!(
            out,
            "[fonts]\nsize = 18\n\n[colors]\nbackground = \"#000\"\n"
        );
    }

    #[test]
    fn same_key_in_other_section_is_untouched() {
        let src = "[a]\nsize = 1\n[fonts]\nsize = 2\n";
        let out = set_value(src, "fonts", "size", "9");
        assert_eq!(out, "[a]\nsize = 1\n[fonts]\nsize = 9\n");
    }

    #[test]
    fn adds_missing_key_after_last_entry_of_section() {
        let src = "[fonts]\nsize = 16\n\n# colours\n[colors]\nx = 1\n";
        let out = set_value(src, "fonts", "family", "\"Fira Code\"");
        assert_eq!(
            out,
            "[fonts]\nsize = 16\nfamily = \"Fira Code\"\n\n# colours\n[colors]\nx = 1\n"
        );
    }

    #[test]
    fn appends_missing_section() {
        let out = set_value("margin = [1]\n", "cursor", "shape", "\"beam\"");
        assert_eq!(out, "margin = [1]\n\n[cursor]\nshape = \"beam\"\n");
    }

    #[test]
    fn empty_source_gets_section() {
        assert_eq!(
            set_value("", "updates", "check", "false"),
            "[updates]\ncheck = false\n"
        );
    }

    #[test]
    fn keeps_trailing_comment_and_indent_of_replaced_line() {
        let src = "[updates]\n  check = true # keep\n";
        let out = set_value(src, "updates", "check", "false");
        assert_eq!(out, "[updates]\n  check = false # keep\n");
    }

    #[test]
    fn commented_key_is_not_a_match() {
        let src = "[fonts]\n# size = 12\n";
        let out = set_value(src, "fonts", "size", "14");
        assert_eq!(out, "[fonts]\nsize = 14\n# size = 12\n");
    }

    #[test]
    fn subtable_header_is_not_the_section() {
        let src = "[fonts.regular]\nfamily = \"x\"\n";
        let out = set_value(src, "fonts", "size", "14");
        assert_eq!(
            out,
            "[fonts.regular]\nfamily = \"x\"\n\n[fonts]\nsize = 14\n"
        );
    }

    #[test]
    fn quote_escapes() {
        assert_eq!(quote("A \"B\" \\"), "\"A \\\"B\\\" \\\\\"");
    }

    #[test]
    fn crlf_is_preserved() {
        let src = "[fonts]\r\nsize = 16\r\n";
        assert_eq!(
            set_value(src, "fonts", "size", "17"),
            "[fonts]\r\nsize = 17\r\n"
        );
    }
}
