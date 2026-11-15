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
    /// Whether the text was rewritten by a language model after transcription.
    #[serde(default)]
    pub rewritten: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64) -> HistoryItem {
        HistoryItem {
            id,
            text: format!("dictation {id}"),
            model: "whisper-small".into(),
            duration_ms: 2000,
            words: 2,
            rewritten: id.is_multiple_of(2),
        }
    }

    #[test]
    fn save_then_load() {
        let dir = crate::test_support::temp_dir("history");
        let path = dir.join("history.json");
        let items: Vec<HistoryItem> = (1..=5).map(item).collect();
        save(&path, &items);
        let loaded = load(&path);
        assert_eq!(loaded.len(), 5);
        assert_eq!(loaded[4].text, "dictation 5");
        assert!(loaded[1].rewritten && !loaded[0].rewritten);
    }

    #[test]
    fn a_missing_or_corrupt_file_is_an_empty_history() {
        let dir = crate::test_support::temp_dir("history");
        assert!(load(&dir.join("absent.json")).is_empty());
        std::fs::write(dir.join("broken.json"), "[{").unwrap();
        assert!(load(&dir.join("broken.json")).is_empty());
    }

    #[test]
    fn entries_written_before_rewriting_existed_still_load() {
        let old = r#"[{"id":1,"text":"hello","model":"whisper-small","duration_ms":900,"words":1}]"#;
        let items: Vec<HistoryItem> = serde_json::from_str(old).unwrap();
        assert!(!items[0].rewritten);
    }

    #[test]
    fn timestamps_are_recent_milliseconds() {
        let now = now_ms();
        assert!(now > 1_700_000_000_000, "expected milliseconds since 1970, got {now}");
    }
}
