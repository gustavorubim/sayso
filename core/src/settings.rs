use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Accelerator such as `Ctrl+Shift+Space` (key part is a KeyboardEvent.code name).
    pub shortcut: String,
    /// `hold` (push to talk) or `toggle` (press to start, press to stop).
    pub mode: String,
    pub model_id: Option<String>,
    /// `auto` or a language code such as `en`.
    pub language: String,
    /// Input device name; `None` uses the system default.
    pub input_device: Option<String>,
    /// `paste` (clipboard + paste keystroke), `type` (simulated typing), or `copy` (clipboard only).
    pub output: String,
    pub restore_clipboard: bool,
    pub show_overlay: bool,
    pub launch_at_login: bool,
    pub remove_fillers: bool,
    pub voice_commands: bool,
    pub trailing_space: bool,
    pub save_history: bool,
    pub replacements: Vec<Replacement>,
    /// `after` (write once the recording stops) or `live` (write at each pause while speaking).
    pub timing: String,
    /// Whether the second shortcut, which rewrites the transcript through OpenRouter, is active.
    pub rewrite_enabled: bool,
    pub rewrite_shortcut: String,
    /// OpenRouter model id, such as `anthropic/claude-haiku-5.5`.
    pub rewrite_model: String,
    /// How the model should rewrite the dictation.
    pub rewrite_prompt: String,
    /// Whether the third shortcut, which transcribes the computer's own sound, is active.
    pub capture_enabled: bool,
    pub capture_shortcut: String,
}

pub const DEFAULT_REWRITE_PROMPT: &str = "Fix typos, grammar and punctuation. Rephrase where it makes the text \
clearer and more natural to read, and remove false starts and repeated words. Keep the meaning, the language it was \
spoken in, and the speaker's tone. Do not add information that was not said.";

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: if cfg!(target_os = "macos") {
                "Alt+Space".into()
            } else {
                "Ctrl+Shift+Space".into()
            },
            mode: "hold".into(),
            model_id: None,
            language: "auto".into(),
            input_device: None,
            output: "paste".into(),
            restore_clipboard: true,
            show_overlay: true,
            launch_at_login: false,
            remove_fillers: false,
            voice_commands: true,
            trailing_space: true,
            save_history: true,
            replacements: Vec::new(),
            timing: "after".into(),
            rewrite_enabled: false,
            // The dictation shortcut plus one more modifier.
            rewrite_shortcut: if cfg!(target_os = "macos") {
                "Alt+Shift+Space".into()
            } else {
                "Ctrl+Shift+Alt+Space".into()
            },
            rewrite_model: "anthropic/claude-haiku-5.5".into(),
            rewrite_prompt: DEFAULT_REWRITE_PROMPT.into(),
            capture_enabled: false,
            capture_shortcut: if cfg!(target_os = "macos") {
                "Ctrl+Alt+KeyH".into()
            } else {
                "Ctrl+Shift+Alt+KeyH".into()
            },
        }
    }
}

impl Settings {
    /// The shortcut for each hotkey slot (dictation, rewrite, computer audio), `None` where
    /// that feature is off.
    pub fn shortcuts(&self) -> [Option<String>; 3] {
        [
            Some(self.shortcut.clone()),
            self.rewrite_enabled.then(|| self.rewrite_shortcut.clone()),
            self.capture_enabled.then(|| self.capture_shortcut.clone()),
        ]
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        write_atomically(path, json.as_bytes()).map_err(|e| format!("Could not save settings: {e}"))
    }
}

/// Replaces `path` with `contents` in one step: the new contents are written to a temporary
/// file beside it, which is then renamed over the original. A reader, or a crash, sees
/// either the old file or the new one, never half of each.
pub fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.{}.{unique}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        // On Windows a rename can fail for a moment while another program (a virus scanner,
        // a reader in our own tests) has the destination open.
        let mut attempt = 0;
        loop {
            match std::fs::rename(&tmp, path) {
                Ok(()) => return Ok(()),
                Err(e) if attempt < 20 && e.kind() == std::io::ErrorKind::PermissionDenied => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(e) => return Err(e),
            }
        }
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"mode":"toggle"}"#).unwrap();
        assert_eq!(s.mode, "toggle");
        assert_eq!(s.output, "paste");
        assert!(s.restore_clipboard);
    }

    #[test]
    fn unknown_fields_from_a_newer_version_are_ignored() {
        let s: Settings = serde_json::from_str(r#"{"mode":"toggle","a_setting_from_the_future":true}"#).unwrap();
        assert_eq!(s.mode, "toggle");
    }

    #[test]
    fn a_missing_or_corrupt_file_gives_defaults() {
        let dir = crate::test_support::temp_dir("settings");
        assert_eq!(Settings::load(&dir.join("absent.json")), Settings::default());
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();
        assert_eq!(Settings::load(&dir.join("broken.json")), Settings::default());
    }

    #[test]
    fn save_then_load_keeps_everything() {
        let dir = crate::test_support::temp_dir("settings");
        let s = Settings {
            shortcut: "Ctrl+Alt+KeyD".into(),
            model_id: Some("whisper-small".into()),
            language: "pt".into(),
            input_device: Some("Microphone (USB)".into()),
            replacements: vec![Replacement {
                from: "línea".into(),
                to: "line \"one\"\n".into(),
            }],
            rewrite_prompt: "Keep it short.".into(),
            capture_enabled: true,
            ..Settings::default()
        };
        s.save(&dir.join("settings.json")).unwrap();
        assert_eq!(Settings::load(&dir.join("settings.json")), s);
    }

    #[test]
    fn shortcut_slots_follow_the_feature_switches() {
        let mut s = Settings::default();
        assert_eq!(s.shortcuts(), [Some(s.shortcut.clone()), None, None]);
        s.rewrite_enabled = true;
        s.capture_enabled = true;
        assert_eq!(
            s.shortcuts(),
            [
                Some(s.shortcut.clone()),
                Some(s.rewrite_shortcut.clone()),
                Some(s.capture_shortcut.clone())
            ]
        );
    }

    #[test]
    fn default_shortcuts_are_usable_and_distinct() {
        let s = Settings::default();
        let ids: Vec<u32> = [&s.shortcut, &s.rewrite_shortcut, &s.capture_shortcut]
            .iter()
            .map(|k| crate::hotkey::parse(k).unwrap_or_else(|e| panic!("{e}")).id())
            .collect();
        assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
    }

    #[test]
    fn optional_features_are_off_until_asked_for() {
        let s = Settings::default();
        assert!(!s.rewrite_enabled, "nothing is sent to a network service by default");
        assert!(!s.capture_enabled);
        assert!(!s.launch_at_login);
        assert_eq!(s.model_id, None);
    }

    /// The interface has its own copy of this list (src/shared/types.ts). Both are checked
    /// against the same file, so adding a setting on one side only fails a test.
    #[test]
    fn settings_fields_match_the_shared_contract() {
        let contract = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/contract/settings-keys.json");
        let expected: Vec<String> =
            serde_json::from_str(&std::fs::read_to_string(&contract).expect("tests/contract/settings-keys.json"))
                .unwrap();
        let value = serde_json::to_value(Settings::default()).unwrap();
        let mut actual: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        actual.sort();
        assert_eq!(
            actual, expected,
            "update tests/contract/settings-keys.json and src/shared/types.ts together"
        );
    }
}
