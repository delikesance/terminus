use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize)]
pub struct Host {
    pub id: Uuid,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub password: Option<String>,
    pub identity_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub notes: String,
    pub os_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Identity {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub public_key: Option<String>,
    pub private_key: Option<String>,
    pub passphrase: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Snippet {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub shortcut: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub command: String,
    pub cwd: Option<String>,
    pub host_id: Option<Uuid>,
    pub session_kind: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PortForward {
    pub id: Uuid,
    pub host_id: Uuid,
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

#[derive(Debug, Serialize, Deserialize)]
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
            font_family: "Cascadia Mono NF".into(),
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Credential {
    pub id: Uuid,
    pub kind: String,
    pub owner_kind: String,
    pub owner_id: Uuid,
    pub envelope: String,
    pub key_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncConfig {
    pub url: String,
    #[serde(default)]
    pub sync_secrets: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncStatus {
    pub configured: bool,
    pub url: Option<String>,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub state: String,
    #[serde(default)]
    pub sync_secrets: bool,
    #[serde(default)]
    pub vault_configured: bool,
    #[serde(default)]
    pub vault_unlocked: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    #[serde(default)]
    pub mtime: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    #[serde(default)]
    pub modified: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub host_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
    pub last_sync: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HostRuntime {
    pub host_id: String,
    pub connection: String,
    pub open_count: usize,
}