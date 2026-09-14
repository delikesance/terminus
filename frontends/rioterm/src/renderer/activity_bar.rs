// Copyright (c) 2026-present, Terminus Contributors.
// Activity bar renderer — 48px icon rail on the left edge.

use rio_backend::sugarloaf::Sugarloaf;

/// Sections visible in the activity bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityBarSection {
    Hosts,
    Forwards,
    Sftp,
    Settings,
}

/// State for the activity bar.
pub struct ActivityBarState {
    pub selected: ActivityBarSection,
    pub collapsed: bool,
}

impl ActivityBarState {
    pub fn new() -> Self {
        Self {
            selected: ActivityBarSection::Hosts,
            collapsed: false,
        }
    }

    pub fn toggle_collapse(&mut self) {
        self.collapsed = !self.collapsed;
    }
}

/// Render the activity bar at the given position.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    state: &ActivityBarState,
    x: f32,
    y: f32,
    height: f32,
    _scale: f32,
) {
    if state.collapsed {
        return;
    }
    // Background
    sugarloaf.rect(None, x, y, 48.0, height, [0.08, 0.08, 0.08, 0.95], 0.05, 11);
    // Icons will be rendered here using sugarloaf primitives
    let sections = [
        ActivityBarSection::Hosts,
        ActivityBarSection::Forwards,
        ActivityBarSection::Sftp,
        ActivityBarSection::Settings,
    ];
    for (i, section) in sections.iter().enumerate() {
        let icon_y = y + 16.0 + (i as f32 * 48.0);
        let is_selected = *section == state.selected;
        let bg = if is_selected {
            [0.2, 0.2, 0.2, 1.0]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        };
        sugarloaf.rect(None, x, icon_y, 48.0, 40.0, bg, 0.06, 12);
    }
}

pub fn hit_test(
    state: &ActivityBarState,
    mouse_x: f32,
    mouse_y: f32,
    x: f32,
    y: f32,
) -> Option<ActivityBarSection> {
    if state.collapsed || mouse_x < x || mouse_x > x + 48.0 {
        return None;
    }
    let relative_y = mouse_y - y - 16.0;
    if relative_y < 0.0 {
        return None;
    }
    let index = (relative_y / 48.0) as usize;
    match index {
        0 => Some(ActivityBarSection::Hosts),
        1 => Some(ActivityBarSection::Forwards),
        2 => Some(ActivityBarSection::Sftp),
        3 => Some(ActivityBarSection::Settings),
        _ => None,
    }
}
