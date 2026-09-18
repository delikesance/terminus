//! SSH / WSL connection sequence: four nodes on a progress line.
//!
//! Paint-free geometry and state for the Termius-style connecting modal.
//! The frontend advances [`ConnectionSequence::step`] on a timer and
//! retires the modal once the session is ready.

use crate::geom::Rect;
use crate::icons::Icon;
use std::cell::Cell;

/// How many nodes sit on the progress line.
pub const STEP_COUNT: usize = 4;

/// Fallback / minimum dialog width when labels are short.
pub const DIALOG_WIDTH: f32 = 480.0;
pub const DIALOG_PAD: f32 = 24.0;
pub const HEADER_HEIGHT: f32 = 64.0;
pub const TRACK_HEIGHT: f32 = 88.0;
pub const NODE_SIZE: f32 = 40.0;
pub const STATUS_HEIGHT: f32 = 36.0;
pub const LOGS_MAX_HEIGHT: f32 = 120.0;
pub const BUTTON_HEIGHT: f32 = 36.0;
pub const BUTTON_GAP: f32 = 12.0;
pub const TRACK_LINE_HEIGHT: f32 = 4.0;
/// Horizontal padding inside each step column around the measured label.
pub const LABEL_COLUMN_PAD: f32 = 6.0;
/// Gap between adjacent step columns (edge to edge).
pub const STEP_COLUMN_GAP: f32 = 12.0;
/// Rough advance used before the painter reports real glyph widths.
const LABEL_ESTIMATE_ADVANCE: f32 = 6.0;

/// Width of one step column: hugs the label (or the node if wider).
pub fn step_column_width(label_width: f32) -> f32 {
    label_width.max(NODE_SIZE) + 2.0 * LABEL_COLUMN_PAD
}

/// Dialog width and evenly spaced node centres (relative to `dialog.x`).
///
/// Every column uses the **widest** label's hug width so node-to-node
/// gaps stay equal; the dialog grows when that uniform track exceeds
/// [`DIALOG_WIDTH`].
pub fn track_layout_from_labels(
    label_widths: &[f32; STEP_COUNT],
) -> (f32, [f32; STEP_COUNT]) {
    let col_w = label_widths
        .iter()
        .map(|&w| step_column_width(w))
        .fold(NODE_SIZE + 2.0 * LABEL_COLUMN_PAD, f32::max);
    let content = col_w * STEP_COUNT as f32 + STEP_COLUMN_GAP * (STEP_COUNT - 1) as f32;
    let dialog_width = (content + 2.0 * DIALOG_PAD).max(DIALOG_WIDTH);
    let start = DIALOG_PAD + (dialog_width - 2.0 * DIALOG_PAD - content).max(0.0) * 0.5;
    let mut node_cx = [0.0; STEP_COUNT];
    let mut x = start;
    for i in 0..STEP_COUNT {
        node_cx[i] = x + col_w * 0.5;
        x += col_w;
        if i + 1 < STEP_COUNT {
            x += STEP_COLUMN_GAP;
        }
    }
    (dialog_width, node_cx)
}

fn estimate_label_width(label: &str) -> f32 {
    (label.chars().count() as f32 * LABEL_ESTIMATE_ADVANCE).max(NODE_SIZE)
}

/// What kind of session is coming up — drives step labels and logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectKind {
    Ssh,
    Wsl,
}

/// Visual state of one node on the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeVisual {
    /// Not reached yet.
    Pending,
    /// Current step — blue pulse.
    Active,
    /// Completed — emerald + check.
    Done,
}

/// What a press inside the connection modal hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionHit {
    ToggleLogs,
    Close,
    /// Anywhere else on the dialog or scrim — consume, don't pass through.
    Consume,
}

/// Live connecting modal for one host open.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionSequence {
    pub host_id: String,
    pub title: String,
    pub endpoint: String,
    pub kind: ConnectKind,
    /// Index of the active step (`0..STEP_COUNT`). When [`Self::succeeded`]
    /// every node is Done and this stays at `STEP_COUNT - 1`.
    pub step: usize,
    pub logs_open: bool,
    pub logs: Vec<String>,
    pub status: String,
    pub succeeded: bool,
    /// Measured (or estimated) advance width of each step label. The
    /// painter updates this each frame so hit-testing matches paint.
    label_widths: Cell<[f32; STEP_COUNT]>,
}

impl ConnectionSequence {
    pub fn start_ssh(
        host_id: impl Into<String>,
        title: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> Self {
        let mut seq = Self {
            host_id: host_id.into(),
            title: title.into(),
            endpoint: endpoint.into(),
            kind: ConnectKind::Ssh,
            step: 0,
            logs_open: false,
            logs: Vec::new(),
            status: String::new(),
            succeeded: false,
            label_widths: Cell::new([NODE_SIZE; STEP_COUNT]),
        };
        seq.refresh_estimated_label_widths();
        seq.apply_step_copy();
        seq
    }

    pub fn start_wsl(
        host_id: impl Into<String>,
        title: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> Self {
        let mut seq = Self {
            host_id: host_id.into(),
            title: title.into(),
            endpoint: endpoint.into(),
            kind: ConnectKind::Wsl,
            step: 0,
            logs_open: false,
            logs: Vec::new(),
            status: String::new(),
            succeeded: false,
            label_widths: Cell::new([NODE_SIZE; STEP_COUNT]),
        };
        seq.refresh_estimated_label_widths();
        seq.apply_step_copy();
        seq
    }

    fn refresh_estimated_label_widths(&self) {
        let mut widths = [NODE_SIZE; STEP_COUNT];
        for i in 0..STEP_COUNT {
            widths[i] = estimate_label_width(self.step_label(i));
        }
        self.label_widths.set(widths);
    }

    /// Record glyph advances from the painter so columns hug real text.
    pub fn set_label_widths(&self, widths: [f32; STEP_COUNT]) {
        self.label_widths.set(widths);
    }

    pub fn label_widths(&self) -> [f32; STEP_COUNT] {
        self.label_widths.get()
    }

    /// Content-hugging dialog width for the current label advances.
    pub fn content_width(&self) -> f32 {
        track_layout_from_labels(&self.label_widths()).0
    }

    /// Total dialog height for the current logs drawer state.
    pub fn height(&self) -> f32 {
        let logs = if self.logs_open {
            BUTTON_GAP + LOGS_MAX_HEIGHT
        } else {
            0.0
        };
        DIALOG_PAD
            + HEADER_HEIGHT
            + TRACK_HEIGHT
            + STATUS_HEIGHT
            + logs
            + BUTTON_GAP
            + BUTTON_HEIGHT
            + DIALOG_PAD
    }

    pub fn dialog_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let h = self.height();
        let w = self.content_width().min(window_width - 16.0);
        Rect::new(
            ((window_width - w) * 0.5).max(8.0),
            ((window_height - h) * 0.5).max(8.0),
            w,
            h.min(window_height - 16.0),
        )
    }

    pub fn header_icon_rect(&self, dialog: Rect) -> Rect {
        Rect::new(dialog.x + DIALOG_PAD, dialog.y + DIALOG_PAD, 44.0, 44.0)
    }

    pub fn logs_button_rect(&self, dialog: Rect) -> Rect {
        let w = 96.0;
        Rect::new(
            dialog.right() - DIALOG_PAD - w,
            dialog.y + DIALOG_PAD + 8.0,
            w,
            28.0,
        )
    }

    pub fn track_band(&self, dialog: Rect) -> Rect {
        Rect::new(
            dialog.x,
            dialog.y + DIALOG_PAD + HEADER_HEIGHT,
            dialog.width,
            TRACK_HEIGHT,
        )
    }

    /// Grey background rail between the first and last node centres.
    pub fn track_line_rect(&self, dialog: Rect) -> Rect {
        let band = self.track_band(dialog);
        let (_, node_cx) = track_layout_from_labels(&self.label_widths());
        let x0 = dialog.x + node_cx[0];
        let x1 = dialog.x + node_cx[STEP_COUNT - 1];
        Rect::new(
            x0,
            band.y + (TRACK_HEIGHT - TRACK_LINE_HEIGHT) * 0.5 - 4.0,
            (x1 - x0).max(0.0),
            TRACK_LINE_HEIGHT,
        )
    }

    /// Blue fill on the rail — fraction of the track for the current step.
    pub fn progress_fill_rect(&self, dialog: Rect) -> Rect {
        let track = self.track_line_rect(dialog);
        let frac = self.progress_fraction();
        Rect::new(track.x, track.y, track.width * frac, track.height)
    }

    /// `0.0`, `1/3`, `2/3`, `1.0` — and `1.0` when succeeded.
    pub fn progress_fraction(&self) -> f32 {
        if self.succeeded {
            return 1.0;
        }
        let max = (STEP_COUNT - 1) as f32;
        (self.step as f32 / max).clamp(0.0, 1.0)
    }

    pub fn node_center(&self, dialog: Rect, index: usize) -> (f32, f32) {
        let (_, node_cx) = track_layout_from_labels(&self.label_widths());
        let track = self.track_line_rect(dialog);
        let cx = dialog.x + node_cx[index.min(STEP_COUNT - 1)];
        let cy = track.y + track.height * 0.5;
        (cx, cy)
    }

    pub fn node_rect(&self, dialog: Rect, index: usize) -> Rect {
        let (cx, cy) = self.node_center(dialog, index);
        Rect::new(
            cx - NODE_SIZE * 0.5,
            cy - NODE_SIZE * 0.5,
            NODE_SIZE,
            NODE_SIZE,
        )
    }

    pub fn status_rect(&self, dialog: Rect) -> Rect {
        Rect::new(
            dialog.x + DIALOG_PAD,
            dialog.y + DIALOG_PAD + HEADER_HEIGHT + TRACK_HEIGHT,
            dialog.width - 2.0 * DIALOG_PAD,
            STATUS_HEIGHT,
        )
    }

    pub fn logs_rect(&self, dialog: Rect) -> Option<Rect> {
        if !self.logs_open {
            return None;
        }
        let y = self.status_rect(dialog).bottom() + BUTTON_GAP * 0.5;
        Some(Rect::new(
            dialog.x + DIALOG_PAD,
            y,
            dialog.width - 2.0 * DIALOG_PAD,
            LOGS_MAX_HEIGHT,
        ))
    }

    pub fn close_button_rect(&self, dialog: Rect) -> Rect {
        let y = if let Some(logs) = self.logs_rect(dialog) {
            logs.bottom() + BUTTON_GAP
        } else {
            self.status_rect(dialog).bottom() + BUTTON_GAP
        };
        Rect::new(
            dialog.x + DIALOG_PAD,
            y,
            dialog.width - 2.0 * DIALOG_PAD,
            BUTTON_HEIGHT,
        )
    }

    pub fn node_state(&self, index: usize) -> NodeVisual {
        if self.succeeded || index < self.step {
            NodeVisual::Done
        } else if index == self.step {
            NodeVisual::Active
        } else {
            NodeVisual::Pending
        }
    }

    pub fn step_icon(&self, index: usize) -> Icon {
        if self.node_state(index) == NodeVisual::Done {
            return Icon::Check;
        }
        match (self.kind, index) {
            (_, 0) => Icon::Monitor,
            (ConnectKind::Ssh, 1) | (ConnectKind::Wsl, 1) => Icon::Globe,
            (_, 2) => Icon::Lock,
            (ConnectKind::Ssh, _) => Icon::Server,
            (ConnectKind::Wsl, _) => Icon::SquareTerminal,
        }
    }

    pub fn step_label(&self, index: usize) -> &str {
        match (self.kind, index) {
            (_, 0) => "Local",
            (ConnectKind::Ssh, 1) => "DNS",
            (ConnectKind::Wsl, 1) => "Windows",
            (_, 2) => "Handshake",
            _ => self.title.as_str(),
        }
    }

    pub fn hit_test(
        &self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ConnectionHit {
        let dialog = self.dialog_rect(window_width, window_height);
        if self.logs_button_rect(dialog).contains(x, y) {
            return ConnectionHit::ToggleLogs;
        }
        if self.close_button_rect(dialog).contains(x, y) {
            return ConnectionHit::Close;
        }
        ConnectionHit::Consume
    }

    pub fn toggle_logs(&mut self) {
        self.logs_open = !self.logs_open;
    }

    /// Move to the next step and append its log line. No-op if already on
    /// the last step or succeeded.
    pub fn advance(&mut self) -> bool {
        if self.succeeded {
            return false;
        }
        if self.step + 1 >= STEP_COUNT {
            return false;
        }
        self.step += 1;
        self.apply_step_copy();
        true
    }

    /// Mark the sequence complete: every node Done, progress full.
    pub fn mark_success(&mut self) {
        if self.succeeded {
            return;
        }
        self.succeeded = true;
        self.step = STEP_COUNT - 1;
        self.status = format!("Connected to {} successfully", self.title);
        self.logs
            .push(format!(">>> Interactive session open on {}.", self.title));
    }

    fn apply_step_copy(&mut self) {
        let (status, log) = self.step_copy(self.step);
        self.status = status;
        self.logs.push(log);
    }

    fn step_copy(&self, step: usize) -> (String, String) {
        match (self.kind, step) {
            (_, 0) => (
                "Initialising the local client…".into(),
                "SSH client ready on the local interface.".into(),
            ),
            (ConnectKind::Ssh, 1) => (
                format!("Resolving {}…", self.endpoint_host()),
                "DNS lookup completed.".into(),
            ),
            (ConnectKind::Wsl, 1) => (
                "Contacting the Windows host…".into(),
                "WSL interop bridge is reachable.".into(),
            ),
            (ConnectKind::Ssh, 2) => (
                "Opening the encrypted tunnel…".into(),
                "TCP connected; key exchange in progress.".into(),
            ),
            (ConnectKind::Wsl, 2) => (
                format!("Starting {}…", self.title),
                format!("Launching distro `{}`.", self.title),
            ),
            (ConnectKind::Ssh, _) => (
                format!("Connecting to {}…", self.title),
                format!("PTY allocated on {}.", self.title),
            ),
            (ConnectKind::Wsl, _) => (
                format!("Opening a shell in {}…", self.title),
                format!("Shell ready inside {}.", self.title),
            ),
        }
    }

    fn endpoint_host(&self) -> &str {
        // "SSH host:22" or raw hostname — show whatever we have after the
        // first space if present, else the whole endpoint.
        self.endpoint
            .split_once(' ')
            .map(|(_, rest)| rest)
            .unwrap_or(self.endpoint.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_hits_the_four_stops() {
        let mut seq = ConnectionSequence::start_ssh("id", "Halyma", "SSH 1.2.3.4:22");
        assert!((seq.progress_fraction() - 0.0).abs() < 1e-4);
        seq.advance();
        assert!((seq.progress_fraction() - 1.0 / 3.0).abs() < 1e-3);
        seq.advance();
        assert!((seq.progress_fraction() - 2.0 / 3.0).abs() < 1e-3);
        seq.advance();
        assert!((seq.progress_fraction() - 1.0).abs() < 1e-4);
        seq.mark_success();
        assert_eq!(seq.progress_fraction(), 1.0);
        assert!(seq.nodes_all_done());
    }

    impl ConnectionSequence {
        fn nodes_all_done(&self) -> bool {
            (0..STEP_COUNT).all(|i| self.node_state(i) == NodeVisual::Done)
        }
    }

    #[test]
    fn nodes_move_pending_active_done() {
        let mut seq = ConnectionSequence::start_wsl("wsl:Ubuntu", "Ubuntu", "WSL");
        assert_eq!(seq.node_state(0), NodeVisual::Active);
        assert_eq!(seq.node_state(1), NodeVisual::Pending);
        seq.advance();
        assert_eq!(seq.node_state(0), NodeVisual::Done);
        assert_eq!(seq.node_state(1), NodeVisual::Active);
        assert_eq!(seq.step_icon(0), Icon::Check);
        assert_eq!(seq.step_icon(1), Icon::Globe);
    }

    #[test]
    fn logs_drawer_grows_the_dialog() {
        let mut seq = ConnectionSequence::start_ssh("id", "Box", "SSH x:22");
        let closed = seq.height();
        seq.toggle_logs();
        assert!(seq.height() > closed);
    }

    #[test]
    fn track_columns_use_even_spacing_from_widest_label() {
        let widths = [30.0, 40.0, 50.0, 93.75];
        let (dialog_w, cx) = track_layout_from_labels(&widths);
        let col_w = step_column_width(93.75);
        let pitch = col_w + STEP_COLUMN_GAP;
        // Equal centre-to-centre distance between consecutive nodes.
        for i in 0..STEP_COUNT - 1 {
            assert!(
                ((cx[i + 1] - cx[i]) - pitch).abs() < 0.01,
                "gap {} → {} was {}, want {}",
                i,
                i + 1,
                cx[i + 1] - cx[i],
                pitch
            );
        }
        let content =
            col_w * STEP_COUNT as f32 + STEP_COLUMN_GAP * (STEP_COUNT - 1) as f32;
        assert!((dialog_w - (content + 2.0 * DIALOG_PAD).max(DIALOG_WIDTH)).abs() < 0.01);
        // Widest label still fits inside dialog pad when centred on its node.
        let label_left = cx[3] - widths[3] * 0.5;
        let label_right = cx[3] + widths[3] * 0.5;
        assert!(label_left >= DIALOG_PAD - 0.01);
        assert!(label_right <= dialog_w - DIALOG_PAD + 0.01);
    }

    #[test]
    fn hit_test_finds_the_buttons() {
        let seq = ConnectionSequence::start_ssh("id", "Box", "SSH x:22");
        let dialog = seq.dialog_rect(1200.0, 800.0);
        let logs = seq.logs_button_rect(dialog);
        assert_eq!(
            seq.hit_test(1200.0, 800.0, logs.x + 2.0, logs.y + 2.0),
            ConnectionHit::ToggleLogs
        );
        let close = seq.close_button_rect(dialog);
        assert_eq!(
            seq.hit_test(1200.0, 800.0, close.x + 2.0, close.y + 2.0),
            ConnectionHit::Close
        );
        assert_eq!(
            seq.hit_test(1200.0, 800.0, 0.0, 0.0),
            ConnectionHit::Consume
        );
    }
}
