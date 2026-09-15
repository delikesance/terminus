//! Settings modal: SSH Keys + Remote SQL Sync.

use crate::geom::Rect;

pub const MAX_WIDTH: f32 = 768.0;
pub const SIDEBAR_WIDTH: f32 = 224.0;
pub const HEIGHT_RATIO: f32 = 0.78;
pub const RADIUS: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Keys,
    SqlSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyItem {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub created: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsModal {
    pub open: bool,
    pub tab: SettingsTab,
    pub keys: Vec<SshKeyItem>,
    pub sql_engine: usize,
    pub sql_uri: String,
    pub sql_passphrase: String,
    pub passphrase_visible: bool,
    pub sync_connected: bool,
    pub sync_status: String,
}

impl Default for SettingsModal {
    fn default() -> Self {
        Self {
            open: false,
            tab: SettingsTab::Keys,
            keys: vec![
                SshKeyItem {
                    id: "101".into(),
                    name: "Production RSA 4096".into(),
                    fingerprint: "SHA256:mKp9xQ2vL8...d1s9".into(),
                    created: "2026-01-15".into(),
                },
                SshKeyItem {
                    id: "102".into(),
                    name: "Deploy Ed25519".into(),
                    fingerprint: "SHA256:9vL2aB4xW1...8f3c".into(),
                    created: "2026-03-02".into(),
                },
            ],
            sql_engine: 0,
            sql_uri: "postgres://user:password@aws-cluster.db.internal:5432/terminus".into(),
            sql_passphrase: "supersecretpassphrase123".into(),
            passphrase_visible: false,
            sync_connected: true,
            sync_status: "Last synced just now".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    Consume,
    Close,
    Tab(SettingsTab),
    NewKey,
    DeleteKey(usize),
    TogglePassphrase,
    TestSync,
    Done,
}

impl SettingsModal {
    pub fn open_tab(&mut self, tab: SettingsTab) {
        self.tab = tab;
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn dialog_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let width = MAX_WIDTH.min(window_width - 32.0).max(320.0);
        let height = (window_height * HEIGHT_RATIO)
            .min(window_height - 32.0)
            .max(360.0);
        Rect::new(
            ((window_width - width) / 2.0).max(0.0),
            ((window_height - height) / 2.0).max(0.0),
            width,
            height,
        )
    }

    pub fn close_button_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let dialog = self.dialog_rect(window_width, window_height);
        Rect::new(dialog.right() - 40.0, dialog.y + 16.0, 28.0, 28.0)
    }

    pub fn tab_rect(
        &self,
        window_width: f32,
        window_height: f32,
        tab: SettingsTab,
    ) -> Rect {
        let dialog = self.dialog_rect(window_width, window_height);
        let y = dialog.y + 72.0 + match tab {
            SettingsTab::Keys => 0.0,
            SettingsTab::SqlSync => 40.0,
        };
        Rect::new(dialog.x + 12.0, y, SIDEBAR_WIDTH - 24.0, 36.0)
    }

    pub fn done_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let dialog = self.dialog_rect(window_width, window_height);
        Rect::new(dialog.right() - 88.0, dialog.bottom() - 40.0, 72.0, 28.0)
    }

    pub fn hit_test(
        &self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> SettingsHit {
        if !self.open {
            return SettingsHit::Consume;
        }
        let dialog = self.dialog_rect(window_width, window_height);
        if !dialog.contains(x, y) {
            return SettingsHit::Close;
        }
        if self.close_button_rect(window_width, window_height).contains(x, y)
            || self.done_rect(window_width, window_height).contains(x, y)
        {
            return SettingsHit::Close;
        }
        if self
            .tab_rect(window_width, window_height, SettingsTab::Keys)
            .contains(x, y)
        {
            return SettingsHit::Tab(SettingsTab::Keys);
        }
        if self
            .tab_rect(window_width, window_height, SettingsTab::SqlSync)
            .contains(x, y)
        {
            return SettingsHit::Tab(SettingsTab::SqlSync);
        }
        SettingsHit::Consume
    }
}
