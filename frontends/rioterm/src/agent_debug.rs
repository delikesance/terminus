//! Temporary NDJSON debug sink for session 2e299a. Remove after verification.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const SESSION: &str = "2e299a";

pub fn log(hypothesis_id: &str, location: &str, message: &str, data_json: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!(
        r#"{{"sessionId":"{SESSION}","hypothesisId":"{hypothesis_id}","location":"{location}","message":"{message}","data":{data_json},"timestamp":{ts}}}"#
    );
    for path in log_paths() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn log_paths() -> Vec<std::path::PathBuf> {
    let mut paths = vec![
        // WSL / Linux workspace (when the binary can see it)
        std::path::PathBuf::from("/home/nixos/development/terminus/.cursor/debug-2e299a.log"),
    ];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        paths.push(
            std::path::PathBuf::from(local)
                .join("terminus-dev")
                .join("debug-2e299a.log"),
        );
    }
    paths
}
