//! Application state and the dictation state machine:
//! idle → recording → transcribing → idle.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::audio::{self, Recorder, SAMPLE_RATE};
use crate::engine::Engine;
use crate::history::{self, HistoryItem};
use crate::hotkey::{self, HotkeyCmd};
use crate::settings::Settings;
use crate::{models, paste, rpc, textproc};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Serialize, Clone)]
pub struct Status {
    /// `idle`, `recording`, `transcribing`, `done` or `error`.
    pub phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub struct App {
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub history: Mutex<Vec<HistoryItem>>,
    phase: Mutex<Phase>,
    recorder: Mutex<Option<Recorder>>,
    engine: Mutex<Option<Engine>>,
    key_down: AtomicBool,
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    hotkey_tx: Mutex<Sender<HotkeyCmd>>,
}

fn emit_status(phase: &'static str, message: Option<String>) {
    rpc::emit("status", Status { phase, message });
}

impl App {
    pub fn new(data_dir: PathBuf, hotkey_tx: Sender<HotkeyCmd>) -> Self {
        let settings = Settings::load(&data_dir.join("settings.json"));
        let history = history::load(&data_dir.join("history.json"));
        Self {
            data_dir,
            settings: Mutex::new(settings),
            history: Mutex::new(history),
            phase: Mutex::new(Phase::Idle),
            recorder: Mutex::new(None),
            engine: Mutex::new(None),
            key_down: AtomicBool::new(false),
            downloads: Mutex::new(HashMap::new()),
            hotkey_tx: Mutex::new(hotkey_tx),
        }
    }

    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    pub fn status(&self) -> Status {
        let phase = match *self.phase.lock().unwrap() {
            Phase::Idle => "idle",
            Phase::Recording => "recording",
            Phase::Transcribing => "transcribing",
        };
        Status { phase, message: None }
    }

    // ---- settings -------------------------------------------------------

    pub fn save_settings(self: &Arc<Self>, mut new: Settings) -> Result<(), String> {
        let old = self.settings.lock().unwrap().clone();
        if !matches!(new.mode.as_str(), "hold" | "toggle") {
            new.mode = "hold".into();
        }
        if !matches!(new.output.as_str(), "paste" | "type" | "copy") {
            new.output = "paste".into();
        }

        if new.shortcut != old.shortcut {
            hotkey::parse(&new.shortcut)?;
            let (tx, rx) = mpsc::channel();
            self.hotkey_tx
                .lock()
                .unwrap()
                .send(HotkeyCmd::Set(new.shortcut.clone(), tx))
                .map_err(|_| "The shortcut service is not running.".to_string())?;
            rx.recv_timeout(Duration::from_secs(3))
                .map_err(|_| "The shortcut service did not answer.".to_string())??;
        }

        new.save(&self.data_dir.join("settings.json"))?;
        let engine_changed = new.model_id != old.model_id || new.language != old.language;
        *self.settings.lock().unwrap() = new;
        if engine_changed {
            self.warm_engine();
        }
        Ok(())
    }

    // ---- models ---------------------------------------------------------

    pub fn downloading(&self) -> HashSet<String> {
        self.downloads.lock().unwrap().keys().cloned().collect()
    }

    pub fn begin_download(&self, id: &str) -> Result<Arc<AtomicBool>, String> {
        let mut downloads = self.downloads.lock().unwrap();
        if downloads.contains_key(id) {
            return Err("This model is already downloading.".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        downloads.insert(id.to_string(), cancel.clone());
        Ok(cancel)
    }

    pub fn end_download(&self, id: &str) {
        self.downloads.lock().unwrap().remove(id);
    }

    pub fn cancel_download(&self, id: &str) {
        if let Some(cancel) = self.downloads.lock().unwrap().get(id) {
            cancel.store(true, Ordering::SeqCst);
        }
    }

    /// A finished download becomes the active model when none is chosen yet.
    pub fn on_model_installed(self: &Arc<Self>, id: &str) {
        let mut settings = self.settings.lock().unwrap().clone();
        if settings.model_id.is_none() {
            settings.model_id = Some(id.to_string());
            if let Err(e) = self.save_settings(settings.clone()) {
                eprintln!("could not select the new model: {e}");
            }
            rpc::emit("settings-changed", settings);
        } else if settings.model_id.as_deref() == Some(id) {
            self.warm_engine();
        }
    }

    pub fn delete_model(&self, id: &str) -> Result<(), String> {
        let model = models::find(id).ok_or_else(|| format!("Unknown model: {id}"))?;
        if self.downloads.lock().unwrap().contains_key(id) {
            return Err("Cancel the download first.".into());
        }
        {
            // Release the files before deleting them (required on Windows).
            let mut engine = self.engine.lock().unwrap();
            if engine.as_ref().map(|e| e.model_id == id).unwrap_or(false) {
                *engine = None;
            }
        }
        let dir = model.dir(&self.models_dir());
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete the model: {e}"))?;
        }
        let mut settings = self.settings.lock().unwrap();
        if settings.model_id.as_deref() == Some(id) {
            settings.model_id = None;
            let _ = settings.save(&self.data_dir.join("settings.json"));
            rpc::emit("settings-changed", settings.clone());
        }
        Ok(())
    }

    /// Loads the selected model in the background so the first dictation is fast.
    pub fn warm_engine(self: &Arc<Self>) {
        let app = self.clone();
        std::thread::spawn(move || {
            let mut engine = app.engine.lock().unwrap();
            if let Err(e) = app.ensure_engine(&mut engine) {
                eprintln!("model not ready: {e}");
            }
        });
    }

    fn ensure_engine(&self, slot: &mut Option<Engine>) -> Result<(), String> {
        let (model_id, language) = {
            let s = self.settings.lock().unwrap();
            (s.model_id.clone(), s.language.clone())
        };
        let model_id = model_id.ok_or("Choose a speech model first.")?;
        let model = models::find(&model_id).ok_or("The selected model no longer exists. Choose another one.")?;
        let language = model.resolve_language(&language);
        if let Some(engine) = slot.as_ref() {
            if engine.model_id == model_id && engine.language == language {
                return Ok(());
            }
        }
        *slot = None; // free the old model before loading the new one
        rpc::emit("engine", "loading");
        let loaded = Engine::load(model, &self.models_dir(), &language);
        rpc::emit("engine", if loaded.is_ok() { "ready" } else { "failed" });
        *slot = Some(loaded?);
        Ok(())
    }

    /// Transcribes a WAV file with the selected model. Nothing is pasted or saved.
    pub fn transcribe_file(&self, path: &str) -> Result<String, String> {
        let wave = sherpa_onnx::Wave::read(path).ok_or("Could not read that file. Only WAV audio is supported.")?;
        let samples = audio::resample(wave.samples(), wave.sample_rate() as u32);
        let raw = {
            let mut engine = self.engine.lock().unwrap();
            self.ensure_engine(&mut engine)?;
            engine.as_ref().expect("engine loaded").transcribe(&samples)
        };
        let settings = self.settings.lock().unwrap().clone();
        Ok(textproc::process(&raw, &settings))
    }

    // ---- history --------------------------------------------------------

    fn save_history(&self, items: &[HistoryItem]) {
        history::save(&self.data_dir.join("history.json"), items);
        rpc::emit("history-changed", ());
    }

    pub fn delete_history_item(&self, id: u64) {
        let mut items = self.history.lock().unwrap();
        items.retain(|i| i.id != id);
        self.save_history(&items);
    }

    pub fn clear_history(&self) {
        let mut items = self.history.lock().unwrap();
        items.clear();
        self.save_history(&items);
    }

    // ---- dictation ------------------------------------------------------

    /// Called on the hotkey thread for every press and release.
    pub fn on_hotkey(self: &Arc<Self>, pressed: bool) {
        let hold = self.settings.lock().unwrap().mode == "hold";
        if pressed {
            // Ignore keyboard auto-repeat.
            if self.key_down.swap(true, Ordering::SeqCst) {
                return;
            }
            if hold {
                self.start_recording();
            } else {
                self.toggle();
            }
        } else {
            self.key_down.store(false, Ordering::SeqCst);
            if hold {
                self.stop_recording();
            }
        }
    }

    pub fn toggle(self: &Arc<Self>) {
        let phase = *self.phase.lock().unwrap();
        match phase {
            Phase::Idle => self.start_recording(),
            Phase::Recording => self.stop_recording(),
            Phase::Transcribing => {}
        }
    }

    fn start_recording(self: &Arc<Self>) {
        let mut phase = self.phase.lock().unwrap();
        if *phase != Phase::Idle {
            return;
        }
        let (model_id, device) = {
            let s = self.settings.lock().unwrap();
            (s.model_id.clone(), s.input_device.clone())
        };
        let installed = model_id
            .as_deref()
            .and_then(models::find)
            .map(|m| m.is_installed(&self.models_dir()))
            .unwrap_or(false);
        if !installed {
            emit_status("error", Some("Download a speech model first".into()));
            return;
        }
        match audio::start(device) {
            Ok(recorder) => {
                *self.recorder.lock().unwrap() = Some(recorder);
                *phase = Phase::Recording;
                emit_status("recording", None);
            }
            Err(e) => emit_status("error", Some(e)),
        }
    }

    fn stop_recording(self: &Arc<Self>) {
        let recorder = {
            let mut phase = self.phase.lock().unwrap();
            if *phase != Phase::Recording {
                return;
            }
            *phase = Phase::Transcribing;
            self.recorder.lock().unwrap().take()
        };
        emit_status("transcribing", None);

        let app = self.clone();
        std::thread::spawn(move || {
            let outcome = match recorder {
                Some(r) => app.transcribe_and_deliver(r),
                None => Ok(false),
            };
            *app.phase.lock().unwrap() = Phase::Idle;
            match outcome {
                Ok(true) => emit_status("done", None),
                Ok(false) => emit_status("idle", None),
                Err(e) => emit_status("error", Some(e)),
            }
        });
    }

    /// Returns `Ok(true)` if text was delivered, `Ok(false)` if there was nothing to say.
    fn transcribe_and_deliver(&self, recorder: Recorder) -> Result<bool, String> {
        let samples = recorder.finish()?;
        let duration_ms = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
        if duration_ms < 300 {
            return Ok(false); // an accidental tap
        }
        if audio::peak(&samples) < 0.004 {
            return Err("Heard only silence. Check the microphone in Settings.".into());
        }

        let (raw, model_id) = {
            let mut engine = self.engine.lock().unwrap();
            self.ensure_engine(&mut engine)?;
            let engine = engine.as_ref().expect("engine loaded");
            (engine.transcribe(&samples), engine.model_id.clone())
        };

        let settings = self.settings.lock().unwrap().clone();
        let text = textproc::process(&raw, &settings);
        if text.is_empty() {
            return Ok(false);
        }

        paste::deliver(&text, &settings.output, settings.restore_clipboard)?;

        if settings.save_history {
            let mut items = self.history.lock().unwrap();
            items.insert(
                0,
                HistoryItem {
                    id: history::now_ms(),
                    words: textproc::word_count(&text),
                    text: text.trim_end().to_string(),
                    model: model_id,
                    duration_ms,
                },
            );
            items.truncate(history::MAX_ITEMS);
            self.save_history(&items);
        }
        Ok(true)
    }
}
