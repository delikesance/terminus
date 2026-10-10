/// One row in either pane list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpRow {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Last modification time, Unix seconds (UTC), when the backend reports it.
    pub modified: Option<i64>,
}

/// Progress of the transfer in flight (drives the Files view's transfer bar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpTransfer {
    pub label: String,
    pub done: u64,
    /// `0` while the size is not known yet (indeterminate bar).
    pub total: u64,
}

impl SftpTransfer {
    /// Completed fraction in `0..=1` (`0` while the total is unknown).
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).clamp(0.0, 1.0) as f32
        }
    }

    /// `"62 % · 11 of 18 MB"`; just the byte count while the total is unknown.
    pub fn caption(&self) -> String {
        if self.total == 0 {
            return if self.done == 0 {
                String::new()
            } else {
                format_bytes(self.done)
            };
        }
        let pct = (self.fraction() * 100.0).floor() as u32;
        let (done, total) = bytes_pair(self.done.min(self.total), self.total);
        format!("{pct} % \u{b7} {done} of {total}")
    }
}

/// Human size with the largest whole unit: `"18 MB"`, `"4 KB"`, `"12 B"`.
pub fn format_bytes(bytes: u64) -> String {
    let (n, unit) = unit_for(bytes);
    unit_text(bytes, n, unit)
}

fn unit_for(bytes: u64) -> (f64, &'static str) {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        (b / (KB * KB * KB), "GB")
    } else if b >= KB * KB {
        (b / (KB * KB), "MB")
    } else if b >= KB {
        (b / KB, "KB")
    } else {
        (b, "B")
    }
}

fn unit_text(bytes: u64, n: f64, unit: &str) -> String {
    if unit == "B" {
        format!("{bytes} B")
    } else if n >= 100.0 || (n - n.round()).abs() < 0.05 {
        format!("{} {unit}", n.round() as u64)
    } else {
        format!("{n:.1} {unit}")
    }
}

/// `(done, total)` as `("11", "18 MB")`: both in the unit of `total`, the
/// unit written once.
fn bytes_pair(done: u64, total: u64) -> (String, String) {
    let (t, unit) = unit_for(total);
    let scale = if t > 0.0 { total as f64 / t } else { 1.0 };
    let d = done as f64 / scale;
    let done_text = if unit == "B" {
        format!("{done}")
    } else if (d - d.round()).abs() < 0.05 || d >= 100.0 {
        format!("{}", d.round() as u64)
    } else {
        format!("{d:.1}")
    };
    (done_text, unit_text(total, t, unit))
}
