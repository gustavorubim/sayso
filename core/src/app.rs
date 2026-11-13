//! Application state and the dictation state machine:
//! idle → recording → transcribing → idle.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;

use crate::audio::{self, Recorder, SAMPLE_RATE};
use crate::engine::{self, Engine};
use crate::history::{self, HistoryItem};
use crate::hotkey::{self, HotkeyCmd};
use crate::settings::Settings;
use crate::{models, paste, rewrite, rpc, textproc};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Serialize, Clone)]
pub struct Status {
    /// `idle`, `recording`, `transcribing`, `rewriting`, `done` or `error`.
    pub phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// One recording, from shortcut press to delivered text.
struct Session {
    recorder: Recorder,
    /// Started with the rewrite shortcut.
    rewrite: bool,
    /// In live mode, the thread that writes text at each pause.
    live: Option<JoinHandle<LiveOutcome>>,
}

struct LiveOutcome {
    text: String,
    samples: usize,
    error: Option<String>,
}

pub struct App {
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub history: Mutex<Vec<HistoryItem>>,
    phase: Mutex<Phase>,
    session: Mutex<Option<Session>>,
    engine: Mutex<Option<Engine>>,
    /// One flag per shortcut: 0 is plain dictation, 1 is dictate-and-rewrite.
    key_down: [AtomicBool; 2],
    /// OpenRouter key. Held in memory only; the shell stores it encrypted and passes it in.
    api_key: Mutex<Option<String>>,
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
            session: Mutex::new(None),
            engine: Mutex::new(None),
            key_down: [AtomicBool::new(false), AtomicBool::new(false)],
            api_key: Mutex::new(None),
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

    fn set_hotkey(&self, slot: usize, shortcut: Option<String>) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.hotkey_tx
            .lock()
            .unwrap()
            .send(HotkeyCmd::Set(slot, shortcut, tx))
            .map_err(|_| "The shortcut service is not running.".to_string())?;
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|_| "The shortcut service did not answer.".to_string())?
    }

    pub fn set_api_key(&self, key: Option<String>) {
        *self.api_key.lock().unwrap() = key;
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

        if !matches!(new.timing.as_str(), "after" | "live") {
            new.timing = "after".into();
        }
        if new.rewrite_model.trim().is_empty() {
            new.rewrite_model = Settings::default().rewrite_model;
        }
        if new.rewrite_prompt.trim().is_empty() {
            new.rewrite_prompt = Settings::default().rewrite_prompt;
        }

        let main_key = hotkey::parse(&new.shortcut)?;
        if new.rewrite_enabled && hotkey::parse(&new.rewrite_shortcut)?.id() == main_key.id() {
            return Err("The rewrite shortcut must be different from the dictation shortcut.".into());
        }
        let rewrite_slot = |s: &Settings| s.rewrite_enabled.then(|| s.rewrite_shortcut.clone());
        // Free the second shortcut first, so swapping the two never collides.
        if rewrite_slot(&new) != rewrite_slot(&old) {
            self.set_hotkey(1, None)?;
        }
        if new.shortcut != old.shortcut {
            if let Err(e) = self.set_hotkey(0, Some(new.shortcut.clone())) {
                let _ = self.set_hotkey(1, rewrite_slot(&old));
                return Err(e);
            }
        }
        if rewrite_slot(&new) != rewrite_slot(&old) {
            if let Err(e) = self.set_hotkey(1, rewrite_slot(&new)) {
                let _ = self.set_hotkey(0, Some(old.shortcut.clone()));
                let _ = self.set_hotkey(1, rewrite_slot(&old));
                return Err(e);
            }
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

    /// Runs a WAV file through the live-writing path as if it were being spoken, and returns
    /// the pieces in the order they would have been written. Nothing is pasted.
    pub fn transcribe_file_live(&self, path: &str) -> Result<Vec<String>, String> {
        let wave = sherpa_onnx::Wave::read(path).ok_or("Could not read that file. Only WAV audio is supported.")?;
        let samples = audio::resample(wave.samples(), wave.sample_rate() as u32);
        let (tx, rx) = mpsc::channel();
        // 50 ms chunks, the same size the recorder sends.
        for chunk in samples.chunks(SAMPLE_RATE as usize / 20) {
            let _ = tx.send(chunk.to_vec());
        }
        drop(tx);
        let mut pieces = Vec::new();
        let outcome = self.live_worker(rx, &mut |text| {
            pieces.push(text.to_string());
            Ok(())
        });
        match outcome.error {
            Some(e) => Err(e),
            None => Ok(pieces),
        }
    }

    /// Rewrites `text` with the configured model. Used by the "try it" box in the interface.
    pub fn rewrite_text(&self, text: &str) -> Result<String, String> {
        let key = self
            .api_key
            .lock()
            .unwrap()
            .clone()
            .ok_or("Add your OpenRouter API key first.")?;
        let (model, prompt) = {
            let s = self.settings.lock().unwrap();
            (s.rewrite_model.clone(), s.rewrite_prompt.clone())
        };
        rewrite::rewrite(&key, &model, &prompt, text)
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
    /// `slot` 0 is plain dictation, 1 is dictate-and-rewrite.
    pub fn on_hotkey(self: &Arc<Self>, slot: usize, pressed: bool) {
        let rewrite = slot == 1;
        let hold = {
            let s = self.settings.lock().unwrap();
            // Live writing pastes while you are still speaking, and held modifier keys would
            // turn that paste into a different shortcut. So live mode is always press-to-toggle.
            s.mode == "hold" && (rewrite || s.timing != "live")
        };
        if pressed {
            // Ignore keyboard auto-repeat.
            if self.key_down[slot].swap(true, Ordering::SeqCst) {
                return;
            }
            if hold {
                self.start_recording(rewrite);
            } else {
                self.toggle(rewrite);
            }
        } else {
            self.key_down[slot].store(false, Ordering::SeqCst);
            if hold {
                // Releasing one shortcut must not end a recording the other one started.
                self.stop_recording(Some(rewrite));
            }
        }
    }

    pub fn toggle(self: &Arc<Self>, rewrite: bool) {
        let phase = *self.phase.lock().unwrap();
        match phase {
            Phase::Idle => self.start_recording(rewrite),
            Phase::Recording => self.stop_recording(None),
            Phase::Transcribing => {}
        }
    }

    fn start_recording(self: &Arc<Self>, rewrite: bool) {
        let mut phase = self.phase.lock().unwrap();
        if *phase != Phase::Idle {
            return;
        }
        let (model_id, device, timing) = {
            let s = self.settings.lock().unwrap();
            (s.model_id.clone(), s.input_device.clone(), s.timing.clone())
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
        if rewrite && self.api_key.lock().unwrap().is_none() {
            emit_status("error", Some("Add your OpenRouter API key under Rewrite first".into()));
            return;
        }

        // Live writing needs the voice-activity model to find the pauses.
        let live = !rewrite && timing == "live" && models::vad_path(&self.models_dir()).is_file();
        let (live_tx, live_rx) = if live {
            let (tx, rx) = mpsc::channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        match audio::start(device, live_tx) {
            Ok(recorder) => {
                let worker = live_rx.map(|rx| {
                    let app = self.clone();
                    std::thread::spawn(move || {
                        let (output, restore) = {
                            let s = app.settings.lock().unwrap();
                            (s.output.clone(), s.restore_clipboard)
                        };
                        app.live_worker(rx, &mut |text| paste::deliver(text, &output, restore))
                    })
                });
                *self.session.lock().unwrap() = Some(Session {
                    recorder,
                    rewrite,
                    live: worker,
                });
                *phase = Phase::Recording;
                emit_status("recording", None);
            }
            Err(e) => emit_status("error", Some(e)),
        }
    }

    /// Stops the recording. With `only`, does nothing unless the recording was started by
    /// that shortcut (`Some(true)` = the rewrite shortcut).
    fn stop_recording(self: &Arc<Self>, only: Option<bool>) {
        let session = {
            let mut phase = self.phase.lock().unwrap();
            if *phase != Phase::Recording {
                return;
            }
            let mut session = self.session.lock().unwrap();
            if let (Some(only), Some(current)) = (only, session.as_ref()) {
                if current.rewrite != only {
                    return;
                }
            }
            *phase = Phase::Transcribing;
            session.take()
        };
        emit_status("transcribing", None);

        let app = self.clone();
        std::thread::spawn(move || {
            let outcome = match session {
                Some(s) => app.finish_session(s),
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

    fn transcribe_samples(&self, samples: &[f32]) -> Result<String, String> {
        let mut engine = self.engine.lock().unwrap();
        self.ensure_engine(&mut engine)?;
        Ok(engine.as_ref().expect("engine loaded").transcribe(samples))
    }

    fn remember(&self, text: &str, duration_ms: u64, rewritten: bool) {
        let (save, model) = {
            let s = self.settings.lock().unwrap();
            (s.save_history, s.model_id.clone().unwrap_or_default())
        };
        if !save || text.trim().is_empty() {
            return;
        }
        let mut items = self.history.lock().unwrap();
        items.insert(
            0,
            HistoryItem {
                id: history::now_ms(),
                words: textproc::word_count(text),
                text: text.trim_end().to_string(),
                model,
                duration_ms,
                rewritten,
            },
        );
        items.truncate(history::MAX_ITEMS);
        self.save_history(&items);
    }

    /// Returns `Ok(true)` if text was delivered, `Ok(false)` if there was nothing to say.
    fn finish_session(&self, session: Session) -> Result<bool, String> {
        let Session {
            recorder,
            rewrite,
            live,
        } = session;

        if let Some(worker) = live {
            // Ending the recording closes the audio channel, which lets the worker finish.
            let _ = recorder.finish();
            let outcome = worker
                .join()
                .map_err(|_| "Live writing stopped unexpectedly.".to_string())?;
            let duration_ms = outcome.samples as u64 * 1000 / SAMPLE_RATE as u64;
            self.remember(&outcome.text, duration_ms, false);
            return match outcome.error {
                Some(e) => Err(e),
                None => Ok(!outcome.text.is_empty()),
            };
        }

        let samples = recorder.finish()?;
        let duration_ms = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
        if duration_ms < 300 {
            return Ok(false); // an accidental tap
        }
        if audio::peak(&samples) < 0.004 {
            return Err("Heard only silence. Check the microphone in Settings.".into());
        }

        let raw = self.transcribe_samples(&samples)?;
        let settings = self.settings.lock().unwrap().clone();
        let text = textproc::process(&raw, &settings);
        if text.is_empty() {
            return Ok(false);
        }

        if !rewrite {
            paste::deliver(&text, &settings.output, settings.restore_clipboard)?;
            self.remember(&text, duration_ms, false);
            return Ok(true);
        }

        emit_status("rewriting", None);
        let key = self.api_key.lock().unwrap().clone();
        let rewritten = match key {
            Some(key) => rewrite::rewrite(&key, &settings.rewrite_model, &settings.rewrite_prompt, &text),
            None => Err("Add your OpenRouter API key under Rewrite first.".to_string()),
        };
        match rewritten {
            Ok(mut better) => {
                if settings.trailing_space && !better.ends_with('\n') {
                    better.push(' ');
                }
                paste::deliver(&better, &settings.output, settings.restore_clipboard)?;
                self.remember(&better, duration_ms, true);
                Ok(true)
            }
            Err(e) => {
                // Don't lose what was said: deliver the plain transcript and say why.
                paste::deliver(&text, &settings.output, settings.restore_clipboard)?;
                self.remember(&text, duration_ms, false);
                Err(format!("{e} Your words were written without the rewrite."))
            }
        }
    }

    /// Live mode: receives 16 kHz audio while recording and writes each utterance as soon as
    /// the speaker pauses.
    fn live_worker(
        &self,
        audio: Receiver<Vec<f32>>,
        deliver: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> LiveOutcome {
        let mut settings = self.settings.lock().unwrap().clone();
        // Pieces are written one after another, so each needs a separator.
        settings.trailing_space = true;

        let Some(vad) = engine::create_vad(&models::vad_path(&self.models_dir()), 0.45, 15.0) else {
            let samples = audio.iter().map(|chunk| chunk.len()).sum();
            return LiveOutcome {
                text: String::new(),
                samples,
                error: Some("Live writing could not start: the voice-activity model did not load.".into()),
            };
        };

        let mut written = String::new();
        let mut error: Option<String> = None;
        let mut write = |segment: &[f32]| {
            if error.is_some() {
                return;
            }
            let result = self.transcribe_samples(segment).and_then(|raw| {
                let text = textproc::process(&raw, &settings);
                if text.is_empty() {
                    return Ok(());
                }
                deliver(&text)?;
                written.push_str(&text);
                Ok(())
            });
            if let Err(e) = result {
                error = Some(e);
            }
        };

        let mut samples = 0usize;
        let mut pending: Vec<f32> = Vec::new();
        for chunk in audio {
            samples += chunk.len();
            pending.extend_from_slice(&chunk);
            let mut used = 0;
            while pending.len() - used >= engine::VAD_WINDOW {
                vad.accept_waveform(&pending[used..used + engine::VAD_WINDOW]);
                used += engine::VAD_WINDOW;
                engine::drain_vad(&vad, &mut write);
            }
            pending.drain(..used);
        }
        if !pending.is_empty() {
            pending.resize(engine::VAD_WINDOW, 0.0);
            vad.accept_waveform(&pending);
        }
        vad.flush();
        engine::drain_vad(&vad, &mut write);

        LiveOutcome {
            text: written,
            samples,
            error,
        }
    }
}
