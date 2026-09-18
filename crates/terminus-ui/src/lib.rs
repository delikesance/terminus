//! Terminus UI: the chrome's state, layout and hit-testing.
//!
//! This crate is deliberately paint-free. It answers three questions
//! and nothing else:
//!
//! * what does the chrome look like — every rectangle, in logical
//!   pixels, from [`geom::Rect`] and the per-section modules;
//! * what is under the pointer — [`chrome::Chrome::handle_press`],
//!   `handle_hover` and `handle_wheel`, which walk the same rectangles;
//! * what does the keyboard do — [`add_host::AddHostForm`], a plain
//!   text sink with a caret and no I/O.
//!
//! The painters live in the frontend (`rioterm`) and read this state;
//! the host database lives there too. Keeping the geometry and the
//! focus model here means they can be tested without a window, a GPU or
//! a database.

pub mod activity_bar;
pub mod add_host;
pub mod add_snippet;
pub mod anim;
pub mod button;
pub mod chrome;
pub mod connection;
pub mod context_menu;
pub mod dialog_form;
pub mod geom;
pub mod icons;
pub mod loading;
pub mod os_icons;
pub mod overlap;
pub mod settings;
pub mod sftp_pane;
pub mod sidebar;
pub mod snippets;
pub mod text_field;
pub mod theme;
pub mod vault_unlock;

pub use activity_bar::{
    ActivityBarState, RailAction, RailHit, Section, SECTIONS, TOP_SECTIONS,
};
pub use add_host::{
    auth_method_label, AddHostForm, AddHostHit, AddHostLayout, Field, FormInput,
    FormOutcome, HostFormValues, AUTH_METHODS, BASE_FIELDS,
};
pub use anim::{lerp, lerp_rect, Ease, RectTween, Tween, SNAP_DURATION};
pub use button::{centered_label_origin, dashed_cta_badge, ButtonKind, ButtonSpec};
pub use chrome::{Chrome, ChromeAction, ChromeCursor, ModalPaintLayer};
pub use connection::{
    ConnectKind, ConnectionHit, ConnectionSequence, NodeVisual, STEP_COUNT,
};
pub use context_menu::{
    ContextAction, ContextItem, ContextMenu, ContextMenuHit,
    ITEM_HEIGHT as CONTEXT_ITEM_HEIGHT, MENU_PAD_X, MENU_RADIUS,
};
pub use geom::Rect;
pub use icons::{Cmd, Icon, IconPlacement};
pub use loading::{
    breath_ring, orbit_dots, phase as loading_phase, shimmer_bar, OrbitDot,
};
pub use os_icons::{HostStatus, OsGlyph};
pub use overlap::{
    assert_no_overlaps, assert_panel_no_overlaps, find_overlaps, rects_overlap,
};
pub use settings::{
    SettingsHit, SettingsModal, SettingsTab, SqlSyncFocus, SshKeyItem, SyncUiStatus,
    SQL_ENGINES,
};
pub use sftp_pane::{
    hit_is_clickable, hit_is_text, join_remote, parent_path, SftpBackend,
    SftpClickResult, SftpConflictKind, SftpConflictPrompt, SftpDrag, SftpFocus, SftpHit,
    SftpNameEdit, SftpNameKind, SftpPaneLayout, SftpPaneState, SftpRow, SftpSideState,
    BTN_GAP, BTN_SIZE, FOOTER_HEIGHT, HEADER_HEIGHT, PANE_GAP, PANE_PAD, ROW_HEIGHT,
    SFTP_DRAG_THRESHOLD, TOOLBAR_HEIGHT,
};
pub use sidebar::{
    Badge, HostDrag, HostDragKind, HostDragPhase, HostDropTarget, HostItem, HostPanel,
    PanelHit, RenameDraft, RenameMoveKind, Row, HOST_DRAG_THRESHOLD,
};
pub use snippets::{SnippetHit, SnippetItem, SnippetsPanel};
pub use text_field::{FieldPaint, TextDraft, TextMoveKind};
pub use theme::ChromeTheme;
pub use vault_unlock::{
    PendingVaultAction, VaultUnlockHit, VaultUnlockLayout, VaultUnlockPrompt,
};

pub use terminus_bridge;
pub use terminus_core;

/// Placeholder version marker.
pub const UI_VERSION: &str = env!("CARGO_PKG_VERSION");
