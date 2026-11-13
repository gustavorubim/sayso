use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MAX_ITEMS: usize = 300;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryItem {
    /// Milliseconds since the Unix epoch; doubles as the timestamp.
    pub id: u64,
    pub text: String,
    pub model: String,
    pub duration_ms: u64,
    pub words: usize,
}

pub fn load(path: &Path) -> Vec<HistoryItem> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, items: &[HistoryItem]) {
    if let Ok(json) = serde_json::to_string(items) {
        if let Err(e) = std::fs::write(path, json) {
            eprintln!("could not save history: {e}");
        }
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
