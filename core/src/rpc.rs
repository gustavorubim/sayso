//! JSON-lines protocol with the desktop shell.
//!
//! Request  (stdin):  `{"id": 1, "method": "get_settings", "params": {...}}`
//! Response (stdout): `{"id": 1, "result": ...}` or `{"id": 1, "error": "..."}`
//! Event    (stdout): `{"event": "status", "payload": ...}`

use std::io::{BufRead, Write};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use crate::app::App;
use crate::settings::Settings;
use crate::{audio, download, models};

fn write_line(v: &Value) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let _ = writeln!(lock, "{v}");
    let _ = lock.flush();
}

/// Send an event to the shell.
pub fn emit(event: &str, payload: impl Serialize) {
    write_line(&json!({ "event": event, "payload": payload }));
}

pub fn serve(app: Arc<App>) {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("bad request: {e}");
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("").to_string();
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        match dispatch(&app, &method, params) {
            Ok(result) => write_line(&json!({ "id": id, "result": result })),
            Err(error) => write_line(&json!({ "id": id, "error": error })),
        }
    }
}

fn str_param(params: &Value, key: &str) -> Result<String, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing parameter: {key}"))
}

fn to_value(v: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn dispatch(app: &Arc<App>, method: &str, params: Value) -> Result<Value, String> {
    match method {
        "get_settings" => to_value(app.settings.lock().unwrap().clone()),
        "save_settings" => {
            let new: Settings = serde_json::from_value(params).map_err(|e| e.to_string())?;
            app.save_settings(new)?;
            to_value(app.settings.lock().unwrap().clone())
        }
        "get_status" => to_value(app.status()),
        "list_models" => to_value(models::list(&app.models_dir(), &app.downloading())),
        "download_model" => {
            download::start(app.clone(), &str_param(&params, "id")?)?;
            Ok(Value::Null)
        }
        "cancel_download" => {
            app.cancel_download(&str_param(&params, "id")?);
            Ok(Value::Null)
        }
        "delete_model" => {
            app.delete_model(&str_param(&params, "id")?)?;
            Ok(Value::Null)
        }
        "list_input_devices" => to_value(audio::input_devices()),
        "get_history" => to_value(app.history.lock().unwrap().clone()),
        "delete_history_item" => {
            let id = params
                .get("id")
                .and_then(Value::as_u64)
                .ok_or("missing parameter: id")?;
            app.delete_history_item(id);
            Ok(Value::Null)
        }
        "clear_history" => {
            app.clear_history();
            Ok(Value::Null)
        }
        "toggle_recording" => {
            app.toggle();
            Ok(Value::Null)
        }
        "transcribe_file" => to_value(app.transcribe_file(&str_param(&params, "path")?)?),
        "copy_text" => {
            crate::paste::copy(&str_param(&params, "text")?)?;
            Ok(Value::Null)
        }
        other => Err(format!("unknown method: {other}")),
    }
}
