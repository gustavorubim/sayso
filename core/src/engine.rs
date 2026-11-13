//! Speech-to-text through sherpa-onnx (ONNX Runtime, CPU).

use std::path::{Path, PathBuf};

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

use crate::audio::SAMPLE_RATE;
use crate::models::{Kind, ModelDef};

/// Samples per voice-activity window (Silero expects 512 at 16 kHz).
pub const VAD_WINDOW: usize = 512;

/// Creates a voice-activity detector. A pause of `min_silence` seconds ends an utterance, and
/// an utterance is cut once it reaches `max_speech` seconds. `None` if the model cannot load.
pub fn create_vad(model: &Path, min_silence: f32, max_speech: f32) -> Option<VoiceActivityDetector> {
    let config = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(model.to_string_lossy().into_owned()),
            threshold: 0.5,
            min_silence_duration: min_silence,
            min_speech_duration: 0.25,
            window_size: VAD_WINDOW as i32,
            max_speech_duration: max_speech,
        },
        sample_rate: SAMPLE_RATE as i32,
        num_threads: 1,
        provider: Some("cpu".into()),
        debug: false,
        ..Default::default()
    };
    VoiceActivityDetector::create(&config, 60.0)
}

/// Hands every finished utterance to `f` and removes it from the detector.
pub fn drain_vad(vad: &VoiceActivityDetector, f: &mut dyn FnMut(&[f32])) {
    while !vad.is_empty() {
        if let Some(segment) = vad.front() {
            f(segment.samples());
        }
        vad.pop();
    }
}

/// Whisper's window is 30 s; keep every chunk comfortably below it.
const MAX_CHUNK_SECONDS: usize = 25;

pub struct Engine {
    pub model_id: String,
    pub language: String,
    recognizer: OfflineRecognizer,
    vad_model: Option<PathBuf>,
}

fn path(dir: &Path, file: &str) -> Option<String> {
    Some(dir.join(file).to_string_lossy().into_owned())
}

fn default_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 4) as i32
}

/// Tuning knobs for the recognizer. `Default` is what the app ships with.
#[derive(Clone, Debug)]
pub struct EngineOptions {
    pub threads: i32,
    /// ONNX Runtime execution provider: `cpu`, `cuda`, `directml`, `coreml`…
    pub provider: String,
    /// Whisper only: frames of silence appended to the audio. -1 lets sherpa-onnx decide.
    pub whisper_tail_paddings: i32,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            threads: default_threads(),
            provider: "cpu".into(),
            whisper_tail_paddings: -1,
        }
    }
}

impl Engine {
    pub fn load(model: &ModelDef, models_dir: &Path, language: &str) -> Result<Engine, String> {
        Self::load_with(model, models_dir, language, &EngineOptions::default())
    }

    pub fn load_with(
        model: &ModelDef,
        models_dir: &Path,
        language: &str,
        options: &EngineOptions,
    ) -> Result<Engine, String> {
        let dir = model.dir(models_dir);
        if !model.is_installed(models_dir) {
            return Err(format!("{} is not downloaded yet.", model.name));
        }
        let language = model.resolve_language(language);

        let mut config = OfflineRecognizerConfig {
            decoding_method: Some("greedy_search".into()),
            ..Default::default()
        };
        let m = &mut config.model_config;
        m.num_threads = options.threads;
        m.provider = Some(options.provider.clone());
        m.debug = false;

        match model.kind {
            Kind::Whisper {
                encoder,
                decoder,
                tokens,
            } => {
                m.whisper.encoder = path(&dir, encoder);
                m.whisper.decoder = path(&dir, decoder);
                // An empty language lets Whisper detect it.
                m.whisper.language = Some(if language == "auto" {
                    String::new()
                } else {
                    language.clone()
                });
                m.whisper.task = Some("transcribe".into());
                m.whisper.tail_paddings = options.whisper_tail_paddings;
                m.tokens = path(&dir, tokens);
            }
            Kind::Transducer {
                encoder,
                decoder,
                joiner,
                tokens,
            } => {
                m.transducer.encoder = path(&dir, encoder);
                m.transducer.decoder = path(&dir, decoder);
                m.transducer.joiner = path(&dir, joiner);
                m.model_type = Some("nemo_transducer".into());
                m.tokens = path(&dir, tokens);
            }
            Kind::Moonshine {
                preprocessor,
                encoder,
                uncached_decoder,
                cached_decoder,
                tokens,
            } => {
                m.moonshine.preprocessor = path(&dir, preprocessor);
                m.moonshine.encoder = path(&dir, encoder);
                m.moonshine.uncached_decoder = path(&dir, uncached_decoder);
                m.moonshine.cached_decoder = path(&dir, cached_decoder);
                m.tokens = path(&dir, tokens);
            }
            Kind::SenseVoice { model: file, tokens } => {
                m.sense_voice.model = path(&dir, file);
                m.sense_voice.language = Some(language.clone());
                m.sense_voice.use_itn = true;
                m.tokens = path(&dir, tokens);
            }
            Kind::Canary {
                encoder,
                decoder,
                tokens,
            } => {
                m.canary.encoder = path(&dir, encoder);
                m.canary.decoder = path(&dir, decoder);
                m.canary.src_lang = Some(language.clone());
                m.canary.tgt_lang = Some(language.clone());
                m.canary.use_pnc = true;
                m.tokens = path(&dir, tokens);
            }
        }

        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| format!("Could not load {}. Try deleting and downloading it again.", model.name))?;

        let vad = crate::models::vad_path(models_dir);
        Ok(Engine {
            model_id: model.id.to_string(),
            language,
            recognizer,
            vad_model: vad.is_file().then_some(vad),
        })
    }

    fn decode(&self, samples: &[f32]) -> String {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        self.recognizer.decode(&stream);
        stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    }

    /// Transcribes 16 kHz mono audio of any length.
    pub fn transcribe(&self, samples: &[f32]) -> String {
        let max_chunk = MAX_CHUNK_SECONDS * SAMPLE_RATE as usize;
        if samples.len() <= max_chunk {
            return self.decode(samples);
        }

        let segments = self
            .split_on_silence(samples)
            .unwrap_or_else(|| samples.chunks(max_chunk).map(<[f32]>::to_vec).collect());
        let parts: Vec<String> = segments
            .iter()
            .map(|s| self.decode(s))
            .filter(|t| !t.is_empty())
            .collect();
        parts.join(" ")
    }

    /// Splits long audio into utterances with Silero VAD. `None` if VAD is unavailable.
    fn split_on_silence(&self, samples: &[f32]) -> Option<Vec<Vec<f32>>> {
        let vad = create_vad(self.vad_model.as_ref()?, 0.4, MAX_CHUNK_SECONDS as f32)?;

        let mut segments = Vec::new();
        let mut keep = |segment: &[f32]| segments.push(segment.to_vec());
        for window in samples.chunks(VAD_WINDOW) {
            vad.accept_waveform(window);
            drain_vad(&vad, &mut keep);
        }
        vad.flush();
        drain_vad(&vad, &mut keep);

        if segments.is_empty() {
            None
        } else {
            Some(segments)
        }
    }
}
