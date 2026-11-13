//! Speech-to-text through sherpa-onnx (ONNX Runtime, CPU).

use std::path::{Path, PathBuf};

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

use crate::audio::SAMPLE_RATE;
use crate::models::{Kind, ModelDef};

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

fn threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 4) as i32
}

impl Engine {
    pub fn load(model: &ModelDef, models_dir: &Path, language: &str) -> Result<Engine, String> {
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
        m.num_threads = threads();
        m.provider = Some("cpu".into());
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
                m.whisper.tail_paddings = -1;
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
        let model = self.vad_model.as_ref()?;
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model.to_string_lossy().into_owned()),
                threshold: 0.5,
                min_silence_duration: 0.4,
                min_speech_duration: 0.25,
                window_size: 512,
                max_speech_duration: MAX_CHUNK_SECONDS as f32,
            },
            sample_rate: SAMPLE_RATE as i32,
            num_threads: 1,
            provider: Some("cpu".into()),
            debug: false,
            ..Default::default()
        };
        let vad = VoiceActivityDetector::create(&config, 60.0)?;

        let mut segments = Vec::new();
        let mut drain = |vad: &VoiceActivityDetector| {
            while !vad.is_empty() {
                if let Some(seg) = vad.front() {
                    segments.push(seg.samples().to_vec());
                }
                vad.pop();
            }
        };
        for window in samples.chunks(512) {
            vad.accept_waveform(window);
            drain(&vad);
        }
        vad.flush();
        drain(&vad);

        if segments.is_empty() {
            None
        } else {
            Some(segments)
        }
    }
}
