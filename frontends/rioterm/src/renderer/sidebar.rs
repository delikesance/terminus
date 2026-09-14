// Copyright (c) 2026-present, Terminus Contributors.
// Sidebar renderer — host tree with groups, connection status, port forwards.

use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

/// Connection state for a host, shown as a colored dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Local,
    Connecting,
    Connected,
    Disconnected,
    Error,
}

impl ConnectionState {
    pub fn dot_color(&self) -> [f32; 4] {
        match self {
            Self::Local => [0.3, 0.7, 1.0, 1.0],
            Self::Connecting => [1.0, 0.8, 0.0, 1.0],
            Self::Connected => [0.2, 0.8, 0.3, 1.0],
            Self::Disconnected => [0.4, 0.4, 0.4, 1.0],
            Self::Error => [1.0, 0.3, 0.3, 1.0],
        }
    }
}

/// A host entry in the sidebar tree.
#[derive(Debug, Clone)]
pub struct SidebarHostEntry {
    pub host_id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub os_id: Option<String>,
    pub group_id: Option<String>,
    pub connection: ConnectionState,
    pub open_count: usize,
}

/// A group entry in the sidebar tree.
#[derive(Debug, Clone)]
pub struct SidebarGroupEntry {
    pub group_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub expanded: bool,
    pub hosts: Vec<SidebarHostEntry>,
}

/// State for the sidebar panel.
pub struct SidebarState {
    pub visible: bool,
    pub width: f32,
    pub scroll_offset: f32,
    pub groups: Vec<SidebarGroupEntry>,
    pub ungrouped: Vec<SidebarHostEntry>,
}

impl SidebarState {
    pub fn new() -> Self {
        Self {
            visible: true,
            width: 260.0,
            scroll_offset: 0.0,
            groups: Vec::new(),
            ungrouped: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

const ITEM_HEIGHT: f32 = 36.0;
const GROUP_HEIGHT: f32 = 32.0;
const PADDING_X: f32 = 12.0;
const DOT_RADIUS: f32 = 4.0;

/// Render the sidebar at the given position.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    state: &SidebarState,
    x: f32,
    y: f32,
    height: f32,
    _scale: f32,
) {
    if !state.visible {
        return;
    }

    // Background
    sugarloaf.rect(
        None,
        x,
        y,
        state.width,
        height,
        [0.1, 0.1, 0.1, 0.95],
        0.05,
        11,
    );

    // Title
    sugarloaf.text_mut().draw(
        x + PADDING_X,
        y + 8.0,
        "Hosts",
        &DrawOpts {
            font_size: 14.0,
            color: [153, 153, 153, 255],
            ..DrawOpts::default()
        },
    );

    let mut current_y = y + 36.0;

    // Render groups
    for group in &state.groups {
        if current_y > y + height {
            break;
        }

        // Group header
        let arrow = if group.expanded {
            "\u{25BC}"
        } else {
            "\u{25B6}"
        };
        sugarloaf.text_mut().draw(
            x + PADDING_X,
            current_y + 8.0,
            &format!("{} {}", arrow, group.name),
            &DrawOpts {
                font_size: 12.0,
                color: [179, 179, 179, 255],
                ..DrawOpts::default()
            },
        );
        current_y += GROUP_HEIGHT;

        // Hosts in group
        if group.expanded {
            for host in &group.hosts {
                if current_y > y + height {
                    break;
                }
                render_host_item(sugarloaf, host, x, current_y, state.width);
                current_y += ITEM_HEIGHT;
            }
        }
    }

    // Ungrouped hosts
    if !state.ungrouped.is_empty() {
        sugarloaf.text_mut().draw(
            x + PADDING_X,
            current_y + 8.0,
            "--- Ungrouped ---",
            &DrawOpts {
                font_size: 11.0,
                color: [102, 102, 102, 255],
                ..DrawOpts::default()
            },
        );
        current_y += GROUP_HEIGHT;

        for host in &state.ungrouped {
            if current_y > y + height {
                break;
            }
            render_host_item(sugarloaf, host, x, current_y, state.width);
            current_y += ITEM_HEIGHT;
        }
    }
}

fn render_host_item(
    sugarloaf: &mut Sugarloaf,
    host: &SidebarHostEntry,
    x: f32,
    y: f32,
    width: f32,
) {
    // Hover/select background
    // sugarloaf.rect(None, x + 2.0, y, width - 4.0, ITEM_HEIGHT, [0.15, 0.15, 0.15, 0.8], 0.06, 13);

    // Connection status dot
    let dot_color = host.connection.dot_color();
    sugarloaf.rect(
        None,
        x + PADDING_X,
        y + ITEM_HEIGHT / 2.0 - DOT_RADIUS,
        DOT_RADIUS * 2.0,
        DOT_RADIUS * 2.0,
        dot_color,
        0.07,
        14,
    );

    // Host name
    sugarloaf.text_mut().draw(
        x + PADDING_X + 14.0,
        y + 6.0,
        &host.name,
        &DrawOpts {
            font_size: 12.0,
            color: [217, 217, 217, 255],
            ..DrawOpts::default()
        },
    );

    // Hostname:port subtitle
    sugarloaf.text_mut().draw(
        x + PADDING_X + 14.0,
        y + 22.0,
        &format!("{}@{}:{}", host.username, host.hostname, host.port),
        &DrawOpts {
            font_size: 10.0,
            color: [115, 115, 115, 255],
            ..DrawOpts::default()
        },
    );

    // Open session count badge
    if host.open_count > 0 {
        let badge_text = format!("{}", host.open_count);
        sugarloaf.text_mut().draw(
            x + width - 24.0,
            y + 8.0,
            &badge_text,
            &DrawOpts {
                font_size: 10.0,
                color: [77, 179, 255, 255],
                ..DrawOpts::default()
            },
        );
    }
}
