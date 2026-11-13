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
}

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
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("Could not save settings: {e}"))
    }
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
}
