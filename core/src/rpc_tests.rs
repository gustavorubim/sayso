//! Tests for the request protocol and the application state behind it. No audio, no models,
//! no network: these run everywhere.

// Tests change one setting at a time; spelling out a full struct each time would bury that.
#![allow(clippy::field_reassign_with_default)]

use serde_json::{json, Value};

use crate::history::{self, HistoryItem};
use crate::rpc::dispatch;
use crate::settings::{Replacement, Settings};
use crate::test_support::{app, app_with_hotkeys};

fn item(id: u64, text: &str) -> HistoryItem {
    HistoryItem {
        id,
        text: text.into(),
        model: "canary-180m-flash".into(),
        duration_ms: 1500,
        words: text.split_whitespace().count(),
        rewritten: false,
    }
}

#[test]
fn settings_round_trip_through_the_protocol() {
    let (app, dir) = app();
    let defaults: Settings = serde_json::from_value(dispatch(&app, "get_settings", Value::Null).unwrap()).unwrap();
    assert_eq!(defaults, Settings::default());

    let mut changed = defaults.clone();
    changed.mode = "toggle".into();
    changed.trailing_space = false;
    changed.replacements = vec![Replacement {
        from: "my email".into(),
        to: "me@example.com".into(),
    }];
    let saved = dispatch(&app, "save_settings", serde_json::to_value(&changed).unwrap()).unwrap();
    assert_eq!(serde_json::from_value::<Settings>(saved).unwrap(), changed);

    // It is on disk, and a fresh start reads it back.
    assert_eq!(Settings::load(&dir.join("settings.json")), changed);
}

#[test]
fn unknown_values_are_replaced_with_safe_ones() {
    let (app, _) = app();
    let mut s = Settings::default();
    s.mode = "sideways".into();
    s.output = "telepathy".into();
    s.timing = "yesterday".into();
    s.rewrite_model = "   ".into();
    s.rewrite_prompt = String::new();
    app.save_settings(s).unwrap();
    let saved = app.settings.lock().unwrap().clone();
    assert_eq!(saved.mode, "hold");
    assert_eq!(saved.output, "paste");
    assert_eq!(saved.timing, "after");
    assert_eq!(saved.rewrite_model, Settings::default().rewrite_model);
    assert_eq!(saved.rewrite_prompt, Settings::default().rewrite_prompt);
}

#[test]
fn malformed_settings_are_rejected_and_nothing_changes() {
    let (app, _) = app();
    assert!(dispatch(&app, "save_settings", json!({"shortcut": 42})).is_err());
    assert!(dispatch(&app, "save_settings", json!("nonsense")).is_err());
    assert_eq!(*app.settings.lock().unwrap(), Settings::default());
}

#[test]
fn an_unusable_shortcut_is_rejected() {
    let (app, _) = app();
    let mut s = Settings::default();
    s.shortcut = "Ctrl+Shift".into(); // modifiers only
    assert!(app.save_settings(s).is_err());
    assert_eq!(app.settings.lock().unwrap().shortcut, Settings::default().shortcut);
}

#[test]
fn shortcuts_must_differ_from_each_other() {
    let (app, _) = app();
    let mut s = Settings::default();
    s.rewrite_enabled = true;
    s.rewrite_shortcut = s.shortcut.clone();
    let err = app.save_settings(s.clone()).unwrap_err();
    assert!(err.contains("different"), "{err}");

    // The same keys written in another order are still the same shortcut.
    s.rewrite_shortcut = "Shift+Ctrl+Space".into();
    s.shortcut = "Ctrl+Shift+Space".into();
    assert!(app.save_settings(s.clone()).is_err());

    // A switched-off shortcut does not count.
    s.rewrite_enabled = false;
    assert!(app.save_settings(s).is_ok());
}

#[test]
fn a_shortcut_the_system_refuses_leaves_settings_untouched() {
    let (app, _) = app_with_hotkeys(|slot, shortcut| match (slot, shortcut) {
        (2, Some(_)) => Err("Another app is using it.".into()),
        _ => Ok(()),
    });
    let mut s = Settings::default();
    s.capture_enabled = true;
    let err = app.save_settings(s).unwrap_err();
    assert!(err.contains("Another app"), "{err}");
    assert!(!app.settings.lock().unwrap().capture_enabled);
}

#[test]
fn only_changed_shortcuts_are_registered_again() {
    use std::sync::{Arc, Mutex};
    type Calls = Arc<Mutex<Vec<(usize, Option<String>)>>>;
    let calls: Calls = Arc::default();
    let seen = calls.clone();
    let (app, _) = app_with_hotkeys(move |slot, shortcut| {
        seen.lock().unwrap().push((slot, shortcut.map(str::to_string)));
        Ok(())
    });

    let mut s = Settings::default();
    s.remove_fillers = true; // nothing to do with shortcuts
    app.save_settings(s.clone()).unwrap();
    assert!(calls.lock().unwrap().is_empty());

    s.rewrite_enabled = true;
    app.save_settings(s.clone()).unwrap();
    let registered = calls.lock().unwrap().clone();
    assert!(registered.iter().all(|(slot, _)| *slot == 1), "{registered:?}");
    assert_eq!(
        registered.last().unwrap().1.as_deref(),
        Some(s.rewrite_shortcut.as_str())
    );
}

#[test]
fn status_starts_idle() {
    let (app, _) = app();
    assert_eq!(
        dispatch(&app, "get_status", Value::Null).unwrap(),
        json!({"phase": "idle"})
    );
}

#[test]
fn the_model_list_matches_the_catalog_and_nothing_is_installed() {
    let (app, _) = app();
    let list = dispatch(&app, "list_models", Value::Null).unwrap();
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), crate::models::CATALOG.len());
    assert!(list
        .iter()
        .all(|m| m["installed"] == json!(false) && m["downloading"] == json!(false)));
    assert_eq!(list.iter().filter(|m| m["recommended"] == json!(true)).count(), 1);
    for m in list {
        assert!(m["size_bytes"].as_u64().unwrap() > 100_000_000, "{}", m["id"]);
    }
}

#[test]
fn model_requests_validate_the_id() {
    let (app, _) = app();
    for method in ["download_model", "delete_model"] {
        assert!(dispatch(&app, method, json!({"id": "no-such-model"}))
            .unwrap_err()
            .contains("Unknown model"));
        assert!(dispatch(&app, method, json!({}))
            .unwrap_err()
            .contains("missing parameter"));
        // An id is looked up in the catalog; it is never used as a path.
        assert!(dispatch(&app, method, json!({"id": "../../outside"})).is_err());
    }
    // Deleting a model that is not downloaded is fine, and cancelling nothing is harmless.
    assert!(dispatch(&app, "delete_model", json!({"id": "whisper-small"})).is_ok());
    assert!(dispatch(&app, "cancel_download", json!({"id": "whisper-small"})).is_ok());
}

#[test]
fn deleting_the_model_in_use_clears_the_selection() {
    let (app, dir) = app();
    let folder = dir.join("models").join("whisper-small");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("leftover.bin"), b"x").unwrap();
    app.settings.lock().unwrap().model_id = Some("whisper-small".into());

    app.delete_model("whisper-small").unwrap();
    assert!(!folder.exists());
    assert_eq!(app.settings.lock().unwrap().model_id, None);
}

#[test]
fn only_one_download_per_model_at_a_time() {
    let (app, _) = app();
    let cancel = app.begin_download("whisper-small").unwrap();
    assert!(app.begin_download("whisper-small").is_err());
    assert!(app.downloading().contains("whisper-small"));
    assert!(app
        .delete_model("whisper-small")
        .unwrap_err()
        .contains("Cancel the download"));

    app.cancel_download("whisper-small");
    assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));
    app.end_download("whisper-small");
    assert!(app.begin_download("whisper-small").is_ok());
}

#[test]
fn the_first_downloaded_model_becomes_the_one_in_use() {
    let (app, _) = app();
    app.on_model_installed("canary-180m-flash");
    assert_eq!(
        app.settings.lock().unwrap().model_id.as_deref(),
        Some("canary-180m-flash")
    );
    // A later download does not take over.
    app.on_model_installed("whisper-small");
    assert_eq!(
        app.settings.lock().unwrap().model_id.as_deref(),
        Some("canary-180m-flash")
    );
}

#[test]
fn history_can_be_read_trimmed_and_cleared() {
    let (app, dir) = app();
    *app.history.lock().unwrap() = vec![item(3, "three"), item(2, "two"), item(1, "one")];

    let listed = dispatch(&app, "get_history", Value::Null).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 3);

    dispatch(&app, "delete_history_item", json!({"id": 2})).unwrap();
    let ids: Vec<u64> = app.history.lock().unwrap().iter().map(|i| i.id).collect();
    assert_eq!(ids, [3, 1]);
    assert_eq!(history::load(&dir.join("history.json")).len(), 2);

    assert!(dispatch(&app, "delete_history_item", json!({})).is_err());

    dispatch(&app, "clear_history", Value::Null).unwrap();
    assert!(app.history.lock().unwrap().is_empty());
    assert!(history::load(&dir.join("history.json")).is_empty());
}

#[test]
fn file_transcription_rejects_bad_paths_without_crashing() {
    let (app, _) = app();
    assert!(dispatch(&app, "transcribe_file", json!({"path": "bad\u{0}path.wav"})).is_err());
    assert!(dispatch(
        &app,
        "transcribe_file",
        json!({"path": "bad\u{0}path.wav", "live": true})
    )
    .is_err());
    assert!(dispatch(&app, "transcribe_file", json!({"path": "C:/does/not/exist.wav"})).is_err());
    assert!(dispatch(&app, "transcribe_file", json!({})).is_err());
}

#[test]
fn rewriting_needs_a_key_and_the_key_is_not_readable() {
    let (app, _) = app();
    assert!(dispatch(&app, "rewrite_text", json!({"text": "hello"}))
        .unwrap_err()
        .contains("API key"));

    dispatch(&app, "set_api_key", json!({"key": "  sk-test  "})).unwrap();
    // No request returns the key: not the settings, not the status.
    for method in ["get_settings", "get_default_settings", "get_status", "list_models"] {
        let reply = dispatch(&app, method, Value::Null).unwrap().to_string();
        assert!(!reply.contains("sk-test"), "{method} leaked the key");
    }

    // An empty or missing key removes it.
    dispatch(&app, "set_api_key", json!({"key": "   "})).unwrap();
    assert!(dispatch(&app, "rewrite_text", json!({"text": "hello"}))
        .unwrap_err()
        .contains("API key"));
    dispatch(&app, "set_api_key", json!({"key": null})).unwrap();
}

#[test]
fn unknown_requests_are_errors() {
    let (app, _) = app();
    assert!(dispatch(&app, "launch_missiles", Value::Null)
        .unwrap_err()
        .contains("unknown method"));
    assert!(dispatch(&app, "", Value::Null).is_err());
}

#[test]
fn recording_does_not_start_without_a_model() {
    let (app, _) = app();
    // No model is selected, so both shortcuts and the tray entry must do nothing but report it.
    dispatch(&app, "toggle_recording", Value::Null).unwrap();
    dispatch(&app, "toggle_capture", Value::Null).unwrap();
    app.on_hotkey(0, true);
    app.on_hotkey(0, false);
    assert_eq!(
        dispatch(&app, "get_status", Value::Null).unwrap(),
        json!({"phase": "idle"})
    );
}
