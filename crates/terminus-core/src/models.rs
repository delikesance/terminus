use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub password: Option<String>,
    pub identity_id: Option<String>,
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Host {
    pub fn new(
        name: impl Into<String>,
        hostname: impl Into<String>,
        port: u16,
        username: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            hostname: hostname.into(),
            port,
            username: username.into(),
            auth_method: "password".into(),
            password: None,
            identity_id: None,
            group_id: None,
            tags: Vec::new(),
            notes: String::new(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Group {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            parent_id: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub public_key: Option<String>,
    pub private_key: Option<String>,
    pub passphrase: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Identity {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            public_key: None,
            private_key: None,
            passphrase: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub shortcut: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Snippet {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            content: content.into(),
            tags: Vec::new(),
            shortcut: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub host_id: Option<String>,
    pub session_kind: String,
    pub created_at: DateTime<Utc>,
}

impl HistoryEntry {
    pub fn new(command: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            command: command.into(),
            cwd: None,
            host_id: None,
            session_kind: kind.into(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    pub id: String,
    pub host_id: String,
    pub kind: String,
    pub name: String,
    pub bind_host: String,
    pub bind_port: u16,
    pub dest_host: Option<String>,
    pub dest_port: Option<u16>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalAppearance {
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: String,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub scrollback: u32,
    pub renderer: String,
    pub padding: u32,
    pub opacity: f32,
    pub custom_css: String,
    pub copy_on_select: bool,
    pub right_click_paste: bool,
    pub ligatures: bool,
    pub theme_id: String,
    pub scroll_sensitivity: f32,
    pub word_separator: String,
}

impl Default for TerminalAppearance {
    fn default() -> Self {
        Self {
            font_family: "IBM Plex Mono".into(),
            font_size: 14.0,
            font_weight: "400".into(),
            line_height: 1.0,
            letter_spacing: 0.0,
            cursor_style: "block".into(),
            cursor_blink: true,
            scrollback: 20000,
            renderer: "auto".into(),
            padding: 10,
            opacity: 1.0,
            custom_css: String::new(),
            copy_on_select: true,
            right_click_paste: true,
            ligatures: true,
            theme_id: "graphite".into(),
            scroll_sensitivity: 1.0,
            word_separator: " ()[]{}',\"`".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorTheme {
    pub id: String,
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection_background: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

impl ColorTheme {
    pub fn builtins() -> Vec<Self> {
        vec![
            theme(
                "graphite",
                "Graphite",
                "#2c2c2e",
                "#f5f5f7",
                "#0a84ff",
                "#0a84ff55",
                [
                    "#48484a", "#ff6961", "#32d74b", "#ffd426", "#64d2ff", "#bf5af2",
                    "#70d7ff", "#e5e5ea", "#8e8e93", "#ff453a", "#30d158", "#ffd60a",
                    "#0a84ff", "#da8fff", "#5ac8f5", "#ffffff",
                ],
            ),
            theme(
                "mocha",
                "Mocha",
                "#1e1e2e",
                "#cdd6f4",
                "#f5e0dc",
                "#45475a",
                [
                    "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7",
                    "#94e2d5", "#bac2de", "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af",
                    "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8",
                ],
            ),
            theme(
                "terminus",
                "Terminus",
                "#12141a",
                "#ece6d8",
                "#e0b15a",
                "#3d3424",
                [
                    "#2a2d36", "#e07a6a", "#8fbe8b", "#e0b15a", "#7ea1c4", "#c49bce",
                    "#7eb8b0", "#c8c2b4", "#5c5a55", "#f0a194", "#b4d4a8", "#f0cc7a",
                    "#9db8d4", "#d8b4e0", "#9ed4cc", "#f4efe4",
                ],
            ),
            theme(
                "obsidian",
                "Obsidian",
                "#0d1117",
                "#e6edf3",
                "#3fb950",
                "#264f78",
                [
                    "#484f58", "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff",
                    "#39d353", "#b1bac4", "#6e7681", "#ffa198", "#56d364", "#e3b341",
                    "#79c0ff", "#d2a8ff", "#7ee787", "#ffffff",
                ],
            ),
            theme(
                "phosphor",
                "Phosphor",
                "#03160d",
                "#b8ffce",
                "#39ff88",
                "#14532d",
                [
                    "#052e16", "#f87171", "#4ade80", "#facc15", "#38bdf8", "#c084fc",
                    "#2dd4bf", "#d1fae5", "#166534", "#fca5a5", "#86efac", "#fde047",
                    "#7dd3fc", "#d8b4fe", "#5eead4", "#ffffff",
                ],
            ),
            theme(
                "midnight",
                "Midnight",
                "#0b1020",
                "#d6e0ff",
                "#8b9cff",
                "#243056",
                [
                    "#1a2238", "#ff6b8a", "#7ee787", "#f0c674", "#82aaff", "#c792ea",
                    "#89ddff", "#c5d0ef", "#3d4a6b", "#ff8ba3", "#a3f5b0", "#ffe08a",
                    "#a8c4ff", "#ddb6ff", "#b3ecff", "#ffffff",
                ],
            ),
            theme(
                "paper",
                "Paper",
                "#f6f1e3",
                "#2b2118",
                "#b45309",
                "#d6c7a8",
                [
                    "#3f3a32", "#9f1239", "#166534", "#854d0e", "#1d4ed8", "#6b21a8",
                    "#0f766e", "#e7e0d0", "#6b6458", "#be123c", "#15803d", "#a16207",
                    "#1e40af", "#7e22ce", "#0d9488", "#1c1917",
                ],
            ),
        ]
    }
}

fn theme(
    id: &str,
    name: &str,
    background: &str,
    foreground: &str,
    cursor: &str,
    selection: &str,
    ansi: [&str; 16],
) -> ColorTheme {
    ColorTheme {
        id: id.into(),
        name: name.into(),
        background: background.into(),
        foreground: foreground.into(),
        cursor: cursor.into(),
        selection_background: selection.into(),
        black: ansi[0].into(),
        red: ansi[1].into(),
        green: ansi[2].into(),
        yellow: ansi[3].into(),
        blue: ansi[4].into(),
        magenta: ansi[5].into(),
        cyan: ansi[6].into(),
        white: ansi[7].into(),
        bright_black: ansi[8].into(),
        bright_red: ansi[9].into(),
        bright_green: ansi[10].into(),
        bright_yellow: ansi[11].into(),
        bright_blue: ansi[12].into(),
        bright_magenta: ansi[13].into(),
        bright_cyan: ansi[14].into(),
        bright_white: ansi[15].into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub url: String,
    pub sync_secrets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub configured: bool,
    pub url: Option<String>,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub host_id: Option<String>,
}
