//! Windowed view of a long, grouped result list for the overlay palette.
//!
//! The command palette can hold dozens of rows but shows a handful; the
//! overlay [`Palette`] lays out whatever groups it is given. This turns a
//! flat list of rows (each tagged with its group) plus the scroll offset
//! into the groups of the visible window, with the selection made relative.

use crate::components::overlay::{Palette, PaletteGroup, PaletteItem};

/// One result row before windowing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSpec {
    pub group: &'static str,
    pub label: String,
    pub hint: String,
}

impl RowSpec {
    pub fn new(
        group: &'static str,
        label: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            group,
            label: label.into(),
            hint: hint.into(),
        }
    }
}

/// Palette for rows `scroll..scroll + max`; `selected` is the absolute index
/// into `rows`. Consecutive rows of one group share a header.
pub fn visible_palette(
    query: &str,
    placeholder: &str,
    rows: &[RowSpec],
    scroll: usize,
    selected: usize,
    max: usize,
) -> Palette {
    let mut groups: Vec<PaletteGroup> = Vec::new();
    for row in rows.iter().skip(scroll).take(max) {
        let item = PaletteItem::new(row.label.clone(), row.hint.clone());
        match groups.last_mut() {
            Some(g) if g.title == row.group => g.items.push(item),
            _ => groups.push(PaletteGroup::new(row.group, vec![item])),
        }
    }
    let mut palette = Palette::new(query, groups);
    palette.selected = selected.saturating_sub(scroll);
    palette.placeholder = Some(placeholder.to_string());
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<RowSpec> {
        vec![
            RowSpec::new("Commands", "New Tab", "Ctrl+T"),
            RowSpec::new("Commands", "Close Tab", ""),
            RowSpec::new("Commands", "Quit", ""),
            RowSpec::new("Servers", "jerem prod", "ubuntu@1.2.3.4"),
        ]
    }

    #[test]
    fn rows_group_under_shared_headers() {
        let p = visible_palette("", "Search", &rows(), 0, 0, 8);
        assert_eq!(p.groups.len(), 2);
        assert_eq!(p.groups[0].title, "Commands");
        assert_eq!(p.groups[0].items.len(), 3);
        assert_eq!(p.groups[1].items[0].label, "jerem prod");
        assert_eq!(p.item_count(), 4);
    }

    #[test]
    fn scrolling_windows_the_list_and_keeps_selection_absolute() {
        let p = visible_palette("", "Search", &rows(), 2, 3, 2);
        assert_eq!(p.item_count(), 2);
        assert_eq!(p.groups[0].title, "Commands");
        assert_eq!(p.groups[0].items[0].label, "Quit");
        assert_eq!(p.selected, 1, "selected 3 with scroll 2 is the 2nd visible");
    }

    #[test]
    fn query_and_placeholder_carry_over() {
        let p = visible_palette("spl", "Search servers", &rows(), 0, 0, 8);
        assert_eq!(p.query, "spl");
        assert_eq!(p.placeholder.as_deref(), Some("Search servers"));
    }

    #[test]
    fn no_rows_is_the_empty_state() {
        let p = visible_palette("zzz", "Search", &[], 0, 0, 8);
        assert!(p.is_empty());
    }
}
