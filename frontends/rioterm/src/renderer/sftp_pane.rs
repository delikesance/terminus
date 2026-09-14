// Copyright (c) 2026-present, Terminus Contributors.
// SFTP dual-pane browser overlay renderer.

use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

/// A file/directory entry in the SFTP browser.
#[derive(Debug, Clone)]
pub struct SftpFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
}

/// Transfer state for an in-progress file transfer.
#[derive(Debug, Clone)]
pub struct SftpTransfer {
    pub filename: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub direction: TransferDirection,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Upload,
    Download,
}

/// State for the SFTP dual-pane overlay.
pub struct SftpPaneState {
    pub visible: bool,
    pub local_path: String,
    pub remote_path: String,
    pub local_entries: Vec<SftpFileEntry>,
    pub remote_entries: Vec<SftpFileEntry>,
    pub local_selected: Option<usize>,
    pub remote_selected: Option<usize>,
    pub active_pane: ActivePane,
    pub transfers: Vec<SftpTransfer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Local,
    Remote,
}

impl SftpPaneState {
    pub fn new() -> Self {
        Self {
            visible: false,
            local_path: String::from("."),
            remote_path: String::from("/"),
            local_entries: Vec::new(),
            remote_entries: Vec::new(),
            local_selected: None,
            remote_selected: None,
            active_pane: ActivePane::Local,
            transfers: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

const PANE_PADDING: f32 = 8.0;
const ROW_HEIGHT: f32 = 24.0;
const BREADCRUMB_HEIGHT: f32 = 32.0;
const TRANSFER_BAR_HEIGHT: f32 = 28.0;

/// Render the SFTP dual-pane overlay.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    state: &SftpPaneState,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    _scale: f32,
) {
    if !state.visible {
        return;
    }

    let half_width = width / 2.0;

    // Background
    sugarloaf.rect(
        None,
        x,
        y,
        width,
        height,
        [0.07, 0.07, 0.07, 0.97],
        0.04,
        18,
    );

    // Local pane breadcrumb
    sugarloaf.text_mut().draw(
        x + PANE_PADDING,
        y + 8.0,
        &format!("Local: {}", state.local_path),
        &DrawOpts {
            font_size: 11.0,
            color: [153, 153, 153, 255],
            ..DrawOpts::default()
        },
    );

    // Separator between panes
    sugarloaf.rect(
        None,
        x + half_width - 0.5,
        y,
        1.0,
        height,
        [0.2, 0.2, 0.2, 1.0],
        0.05,
        19,
    );

    // Remote pane breadcrumb
    sugarloaf.text_mut().draw(
        x + half_width + PANE_PADDING,
        y + 8.0,
        &format!("Remote: {}", state.remote_path),
        &DrawOpts {
            font_size: 11.0,
            color: [153, 153, 153, 255],
            ..DrawOpts::default()
        },
    );

    // Transfer queue at bottom
    if !state.transfers.is_empty() {
        let transfer_y = y + height - TRANSFER_BAR_HEIGHT - 8.0;
        sugarloaf.rect(
            None,
            x,
            transfer_y,
            width,
            TRANSFER_BAR_HEIGHT,
            [0.12, 0.12, 0.12, 0.9],
            0.05,
            19,
        );

        for (i, transfer) in state.transfers.iter().enumerate() {
            let ty = transfer_y + 4.0 + (i as f32 * 22.0);
            if ty > transfer_y + TRANSFER_BAR_HEIGHT {
                break;
            }

            let pct = if transfer.total_bytes > 0 {
                transfer.bytes_transferred as f32 / transfer.total_bytes as f32
            } else {
                0.0
            };

            let dir_icon = match transfer.direction {
                TransferDirection::Upload => "\u{2191}",
                TransferDirection::Download => "\u{2193}",
            };

            sugarloaf.text_mut().draw(
                x + PANE_PADDING,
                ty,
                &format!("{} {} {:.0}%", dir_icon, transfer.filename, pct * 100.0),
                &DrawOpts {
                    font_size: 10.0,
                    color: [179, 179, 179, 255],
                    ..DrawOpts::default()
                },
            );

            // Progress bar
            let bar_width = width - PANE_PADDING * 2.0;
            sugarloaf.rect(
                None,
                x + PANE_PADDING,
                ty + 14.0,
                bar_width,
                3.0,
                [0.15, 0.15, 0.15, 1.0],
                0.06,
                20,
            );
            sugarloaf.rect(
                None,
                x + PANE_PADDING,
                ty + 14.0,
                bar_width * pct,
                3.0,
                [0.3, 0.7, 1.0, 1.0],
                0.06,
                21,
            );
        }
    }
}
