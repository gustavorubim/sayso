//! The model catalog. Every entry is a set of ONNX files in a Hugging Face
//! repository that the sherpa-onnx runtime can load.
//!
//! To add a model: add a `ModelDef` here, with exact file names and sizes from
//! the repository's "Files" tab, and pick the matching `Kind`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub const HF_BASE: &str = "https://huggingface.co";

/// Voice-activity model used to split long recordings into utterances.
pub const VAD_REPO: &str = "csukuangfj/vad";
pub const VAD_FILE: &str = "silero_vad.onnx";
pub const VAD_SIZE: u64 = 1_807_522;

#[derive(Clone, Copy)]
pub enum Kind {
    Whisper {
        encoder: &'static str,
        decoder: &'static str,
        tokens: &'static str,
    },
    /// NVIDIA NeMo transducer (Parakeet TDT).
    Transducer {
        encoder: &'static str,
        decoder: &'static str,
        joiner: &'static str,
        tokens: &'static str,
    },
    Moonshine {
        preprocessor: &'static str,
        encoder: &'static str,
        uncached_decoder: &'static str,
        cached_decoder: &'static str,
        tokens: &'static str,
    },
    SenseVoice {
        model: &'static str,
        tokens: &'static str,
    },
    Canary {
        encoder: &'static str,
        decoder: &'static str,
        tokens: &'static str,
    },
}

pub struct ModelDef {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    /// Repository the files are downloaded from.
    pub repo: &'static str,
    /// The original model card.
    pub source: &'static str,
    pub description: &'static str,
    pub languages: &'static str,
    /// Language codes the user may pick. Empty means the model decides by itself.
    pub language_choices: &'static [&'static str],
    /// Whether `auto` (detect the language) is allowed.
    pub auto_language: bool,
    pub files: &'static [(&'static str, u64)],
    pub kind: Kind,
    pub recommended: bool,
}

/// Languages offered for Whisper. `auto` is added by the UI.
const WHISPER_LANGS: &[&str] = &[
    "en", "es", "pt", "fr", "de", "it", "nl", "pl", "ru", "uk", "tr", "ar", "hi", "zh", "ja", "ko", "sv", "da", "no",
    "fi", "cs", "el", "he", "id", "vi", "th", "ro", "hu", "ca",
];

pub const CATALOG: &[ModelDef] = &[
    ModelDef {
        id: "whisper-large-v3-turbo",
        name: "Whisper Large v3 Turbo",
        family: "Whisper",
        repo: "csukuangfj/sherpa-onnx-whisper-turbo",
        source: "openai/whisper-large-v3-turbo",
        description: "OpenAI's best speed-to-accuracy trade-off. Strong in about 100 languages, handles accents and noise well.",
        languages: "About 100 languages",
        language_choices: WHISPER_LANGS,
        auto_language: true,
        files: &[
            ("turbo-encoder.int8.onnx", 674_716_297),
            ("turbo-decoder.int8.onnx", 361_080_764),
            ("turbo-tokens.txt", 816_730),
        ],
        kind: Kind::Whisper {
            encoder: "turbo-encoder.int8.onnx",
            decoder: "turbo-decoder.int8.onnx",
            tokens: "turbo-tokens.txt",
        },
        recommended: false,
    },
    ModelDef {
        id: "parakeet-tdt-0.6b-v3",
        name: "Parakeet TDT 0.6B v3",
        family: "Parakeet",
        repo: "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
        source: "nvidia/parakeet-tdt-0.6b-v3",
        description: "NVIDIA's transducer. Very fast on a CPU with top-tier accuracy, automatic punctuation and capitalization.",
        languages: "25 European languages, detected automatically",
        language_choices: &[],
        auto_language: true,
        files: &[
            ("encoder.int8.onnx", 652_282_294),
            ("decoder.int8.onnx", 11_845_274),
            ("joiner.int8.onnx", 6_355_277),
            ("tokens.txt", 93_939),
        ],
        kind: Kind::Transducer {
            encoder: "encoder.int8.onnx",
            decoder: "decoder.int8.onnx",
            joiner: "joiner.int8.onnx",
            tokens: "tokens.txt",
        },
        recommended: true,
    },
    ModelDef {
        id: "distil-whisper-large-v3.5",
        name: "Distil-Whisper Large v3.5",
        family: "Whisper",
        repo: "csukuangfj/sherpa-onnx-whisper-distil-large-v3.5",
        source: "distil-whisper/distil-large-v3.5",
        description: "A distilled Whisper Large: close to its accuracy in English with a much smaller, faster decoder.",
        languages: "English",
        language_choices: &[],
        auto_language: false,
        files: &[
            ("distil-large-v3.5-encoder.int8.onnx", 668_147_593),
            ("distil-large-v3.5-decoder.int8.onnx", 314_871_757),
            ("distil-large-v3.5-tokens.txt", 816_730),
        ],
        kind: Kind::Whisper {
            encoder: "distil-large-v3.5-encoder.int8.onnx",
            decoder: "distil-large-v3.5-decoder.int8.onnx",
            tokens: "distil-large-v3.5-tokens.txt",
        },
        recommended: false,
    },
    ModelDef {
        id: "whisper-small",
        name: "Whisper Small",
        family: "Whisper",
        repo: "csukuangfj/sherpa-onnx-whisper-small",
        source: "openai/whisper-small",
        description: "A lighter multilingual Whisper for older laptops. Good accuracy, modest memory use.",
        languages: "About 100 languages",
        language_choices: WHISPER_LANGS,
        auto_language: true,
        files: &[
            ("small-encoder.int8.onnx", 112_442_483),
            ("small-decoder.int8.onnx", 262_226_114),
            ("small-tokens.txt", 816_730),
        ],
        kind: Kind::Whisper {
            encoder: "small-encoder.int8.onnx",
            decoder: "small-decoder.int8.onnx",
            tokens: "small-tokens.txt",
        },
        recommended: false,
    },
    ModelDef {
        id: "canary-180m-flash",
        name: "Canary 180M Flash",
        family: "Canary",
        repo: "csukuangfj/sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8",
        source: "nvidia/canary-180m-flash",
        description: "NVIDIA's compact encoder-decoder. Small download, punctuated output, four languages.",
        languages: "English, Spanish, German, French",
        language_choices: &["en", "es", "de", "fr"],
        auto_language: false,
        files: &[
            ("encoder.int8.onnx", 132_678_643),
            ("decoder.int8.onnx", 74_437_848),
            ("tokens.txt", 53_555),
        ],
        kind: Kind::Canary { encoder: "encoder.int8.onnx", decoder: "decoder.int8.onnx", tokens: "tokens.txt" },
        recommended: false,
    },
    ModelDef {
        id: "sensevoice-small",
        name: "SenseVoice Small",
        family: "SenseVoice",
        repo: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
        source: "FunAudioLLM/SenseVoiceSmall",
        description: "Alibaba's non-autoregressive model. Extremely fast, and the strongest choice here for East Asian languages.",
        languages: "Chinese, Cantonese, English, Japanese, Korean",
        language_choices: &["zh", "yue", "en", "ja", "ko"],
        auto_language: true,
        files: &[("model.int8.onnx", 239_233_841), ("tokens.txt", 315_894)],
        kind: Kind::SenseVoice { model: "model.int8.onnx", tokens: "tokens.txt" },
        recommended: false,
    },
    ModelDef {
        id: "moonshine-base",
        name: "Moonshine Base",
        family: "Moonshine",
        repo: "csukuangfj/sherpa-onnx-moonshine-base-en-int8",
        source: "UsefulSensors/moonshine-base",
        description: "Built for live dictation on small devices. Near-instant on short phrases, low memory use.",
        languages: "English",
        language_choices: &[],
        auto_language: false,
        files: &[
            ("preprocess.onnx", 14_077_290),
            ("encode.int8.onnx", 50_311_494),
            ("uncached_decode.int8.onnx", 122_120_451),
            ("cached_decode.int8.onnx", 99_983_837),
            ("tokens.txt", 436_688),
        ],
        kind: Kind::Moonshine {
            preprocessor: "preprocess.onnx",
            encoder: "encode.int8.onnx",
            uncached_decoder: "uncached_decode.int8.onnx",
            cached_decoder: "cached_decode.int8.onnx",
            tokens: "tokens.txt",
        },
        recommended: false,
    },
];

pub fn find(id: &str) -> Option<&'static ModelDef> {
    CATALOG.iter().find(|m| m.id == id)
}

impl ModelDef {
    pub fn size_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.1).sum()
    }

    pub fn dir(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.id)
    }

    pub fn is_installed(&self, models_dir: &Path) -> bool {
        let dir = self.dir(models_dir);
        self.files.iter().all(|(name, _)| dir.join(name).is_file())
    }

    pub fn file_url(&self, file: &str) -> String {
        format!("{HF_BASE}/{}/resolve/main/{file}", self.repo)
    }

    /// The language to run with, given the user's preference.
    pub fn resolve_language(&self, wanted: &str) -> String {
        if wanted != "auto" && self.language_choices.contains(&wanted) {
            return wanted.to_string();
        }
        if self.auto_language {
            return "auto".to_string();
        }
        self.language_choices.first().copied().unwrap_or("en").to_string()
    }
}

pub fn vad_path(models_dir: &Path) -> PathBuf {
    models_dir.join("_vad").join(VAD_FILE)
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub repo: &'static str,
    pub source: &'static str,
    pub description: &'static str,
    pub languages: &'static str,
    pub language_choices: &'static [&'static str],
    pub auto_language: bool,
    pub size_bytes: u64,
    pub recommended: bool,
    pub installed: bool,
    pub downloading: bool,
}

pub fn list(models_dir: &Path, downloading: &HashSet<String>) -> Vec<ModelInfo> {
    CATALOG
        .iter()
        .map(|m| ModelInfo {
            id: m.id,
            name: m.name,
            family: m.family,
            repo: m.repo,
            source: m.source,
            description: m.description,
            languages: m.languages,
            language_choices: m.language_choices,
            auto_language: m.auto_language,
            size_bytes: m.size_bytes(),
            recommended: m.recommended,
            installed: m.is_installed(models_dir),
            downloading: downloading.contains(m.id),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_files_listed() {
        let mut seen = HashSet::new();
        for m in CATALOG {
            assert!(seen.insert(m.id), "duplicate id {}", m.id);
            assert!(!m.files.is_empty());
            assert!(m.size_bytes() > 1_000_000);
        }
    }

    #[test]
    fn kind_files_are_in_the_download_list() {
        for m in CATALOG {
            let needed: Vec<&str> = match m.kind {
                Kind::Whisper {
                    encoder,
                    decoder,
                    tokens,
                } => vec![encoder, decoder, tokens],
                Kind::Transducer {
                    encoder,
                    decoder,
                    joiner,
                    tokens,
                } => vec![encoder, decoder, joiner, tokens],
                Kind::Moonshine {
                    preprocessor,
                    encoder,
                    uncached_decoder,
                    cached_decoder,
                    tokens,
                } => {
                    vec![preprocessor, encoder, uncached_decoder, cached_decoder, tokens]
                }
                Kind::SenseVoice { model, tokens } => vec![model, tokens],
                Kind::Canary {
                    encoder,
                    decoder,
                    tokens,
                } => vec![encoder, decoder, tokens],
            };
            for f in needed {
                assert!(
                    m.files.iter().any(|(name, _)| *name == f),
                    "{}: {f} is not downloaded",
                    m.id
                );
            }
        }
    }

    #[test]
    fn language_resolution() {
        let whisper = find("whisper-small").unwrap();
        assert_eq!(whisper.resolve_language("pt"), "pt");
        assert_eq!(whisper.resolve_language("auto"), "auto");
        let canary = find("canary-180m-flash").unwrap();
        assert_eq!(canary.resolve_language("auto"), "en");
        assert_eq!(canary.resolve_language("de"), "de");
        assert_eq!(canary.resolve_language("ja"), "en");
        let moonshine = find("moonshine-base").unwrap();
        assert_eq!(moonshine.resolve_language("fr"), "en");
    }
}
