//! Audio regression tests: real recordings through real speech models.
//!
//! Unit tests cannot tell whether dictation still works. Version 0.4.0 shipped a one-line
//! tuning change that made Whisper repeat itself on short recordings and nearly doubled its
//! error rate; every unit test passed. These tests would have failed.
//!
//! They need speech models, which are too large for the repository, so they are `#[ignore]`d
//! by default. To run them:
//!
//! ```text
//! cargo run -- fetch --data-dir <folder> canary-180m-flash whisper-large-v3-turbo
//! SAYSO_TEST_DATA=<folder> cargo test audio_tests -- --ignored --test-threads=1
//! ```
//!
//! `SAYSO_TEST_MODELS` (comma-separated ids) limits the run to some of the models in that
//! folder; without it every model found there is tested. CI runs them with Canary and
//! Whisper Large v3 Turbo. Turbo matters: the 0.4.0 bug did not show up on Whisper Small.
//!
//! The recordings are in `tests/audio`: `clips` are ordinary dictations of four to eight
//! seconds, `short` are two to three seconds. Each `.wav` has a `.txt` with the right words.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

use crate::app::App;
use crate::audio::{self, SAMPLE_RATE};
use crate::bench::word_errors;
use crate::engine::{self, Engine};
use crate::models::{self, ModelDef};

const SR: usize = SAMPLE_RATE as usize;

/// The most errors per 100 words a model may make on each set. These are what the model
/// scored when the test was written, plus headroom for differences between machines; they
/// are a tripwire for regressions, not a claim about the model's accuracy. A model without
/// an entry gets a loose default.
struct Limits {
    clips: f64,
    short: f64,
}

fn limits(model: &str) -> Limits {
    let (clips, short) = match model {
        "canary-180m-flash" => (CANARY.0, CANARY.1),
        "whisper-small" => (WHISPER_SMALL.0, WHISPER_SMALL.1),
        "whisper-large-v3-turbo" => (WHISPER_TURBO.0, WHISPER_TURBO.1),
        "parakeet-tdt-0.6b-ultra" | "parakeet-tdt-0.6b-v3" => (PARAKEET.0, PARAKEET.1),
        _ => (25.0, 35.0),
    };
    Limits { clips, short }
}

// (clips, short), in percent. See `limits`. Measured when written: Canary 2.3 / 5.4,
// Whisper Turbo 4.5 / 5.4, Whisper Small 6.8 / 5.4, Parakeet Ultra 1.7 / 5.4. With the 0.4.0
// padding bug Whisper Turbo scored 10.8 / 55.4.
const CANARY: (f64, f64) = (8.0, 12.0);
const WHISPER_SMALL: (f64, f64) = (13.0, 12.0);
const WHISPER_TURBO: (f64, f64) = (10.0, 12.0);
const PARAKEET: (f64, f64) = (7.0, 12.0);

struct Recording {
    name: String,
    samples: Vec<f32>,
    text: String,
}

fn data_dir() -> PathBuf {
    let dir = std::env::var_os("SAYSO_TEST_DATA")
        .expect("set SAYSO_TEST_DATA to a folder prepared with `sayso-core fetch`; see core/src/audio_tests.rs");
    PathBuf::from(dir)
}

fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// The models to test: those named in SAYSO_TEST_MODELS, or every one that is downloaded.
fn test_models() -> Vec<&'static ModelDef> {
    let wanted: Option<Vec<String>> = std::env::var("SAYSO_TEST_MODELS")
        .ok()
        .map(|s| s.split(',').map(|id| id.trim().to_string()).collect());
    let found: Vec<&ModelDef> = models::CATALOG
        .iter()
        .filter(|m| m.is_installed(&models_dir()))
        .filter(|m| wanted.as_ref().map(|w| w.iter().any(|id| id == m.id)).unwrap_or(true))
        .collect();
    if let Some(wanted) = &wanted {
        for id in wanted {
            assert!(
                found.iter().any(|m| m.id == id),
                "SAYSO_TEST_MODELS names {id}, which is not downloaded in {}",
                models_dir().display()
            );
        }
    }
    assert!(
        !found.is_empty(),
        "no speech models found in {}",
        models_dir().display()
    );
    found
}

fn recordings(kind: &str) -> Vec<Recording> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("audio")
        .join(kind);
    let mut wavs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav"))
        .collect();
    wavs.sort();
    assert!(!wavs.is_empty(), "no recordings in {}", dir.display());
    wavs.iter()
        .map(|wav| {
            let wave = sherpa_onnx::Wave::read(&wav.to_string_lossy()).expect("readable wav");
            Recording {
                name: wav.file_stem().unwrap().to_string_lossy().into_owned(),
                samples: audio::resample(wave.samples(), wave.sample_rate() as u32),
                text: std::fs::read_to_string(wav.with_extension("txt")).expect("transcript next to the wav"),
            }
        })
        .collect()
}

/// Models are loaded once and shared; inference is serialised so results do not depend on
/// how the test runner schedules threads.
fn engine(model: &ModelDef) -> Arc<Engine> {
    static ENGINES: OnceLock<Mutex<HashMap<&'static str, Arc<Engine>>>> = OnceLock::new();
    let mut engines = ENGINES.get_or_init(Default::default).lock().unwrap();
    engines
        .entry(model.id)
        .or_insert_with(|| {
            Arc::new(Engine::load(model, &models_dir(), "en").unwrap_or_else(|e| panic!("{}: {e}", model.id)))
        })
        .clone()
}

fn transcribe(model: &ModelDef, samples: &[f32]) -> String {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let engine = engine(model);
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    engine.transcribe(samples)
}

/// Errors per 100 reference words over a set of (reference, transcript) pairs.
fn error_rate<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> f64 {
    let (mut errors, mut words) = (0, 0);
    for (reference, transcript) in pairs {
        let (e, n) = word_errors(reference, transcript);
        errors += e;
        words += n;
    }
    100.0 * errors as f64 / words.max(1) as f64
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// True if the same pair of neighbouring words occurs twice: "to do this he must, to do".
fn repeats_itself(text: &str) -> bool {
    let w = words(text);
    let pairs: Vec<(&String, &String)> = w.windows(2).map(|p| (&p[0], &p[1])).collect();
    pairs.iter().enumerate().any(|(i, p)| pairs[i + 1..].contains(p))
}

fn silence(seconds: f32) -> Vec<f32> {
    vec![0.0; (seconds * SR as f32) as usize]
}

/// All twelve clips as one recording, with a pause between them.
fn long_recording() -> (Vec<f32>, String) {
    let mut samples = Vec::new();
    let mut text = String::new();
    for r in recordings("clips") {
        samples.extend_from_slice(&r.samples);
        samples.extend(silence(0.7));
        text.push_str(&r.text);
        text.push(' ');
    }
    (samples, text)
}

// ---- helpers are tested without any model ----

#[test]
fn repetition_detector() {
    assert!(repeats_itself("To do this he must scheme. To do"));
    assert!(repeats_itself("This is a test. This is a test"));
    assert!(repeats_itself("and and and and you"));
    assert!(!repeats_itself("Sure you've looked round everywhere, boy?"));
    assert!(!repeats_itself("the cat and the dog"));
    assert!(!repeats_itself(""));
}

#[test]
fn fixtures_are_complete() {
    let clips = recordings("clips");
    let short = recordings("short");
    assert_eq!(clips.len(), 12);
    assert_eq!(short.len(), 8);
    for r in clips.iter().chain(&short) {
        assert!(!r.text.trim().is_empty(), "{} has no transcript", r.name);
        assert!(audio::peak(&r.samples) > 0.05, "{} is nearly silent", r.name);
    }
    assert!(clips.iter().all(|r| (4 * SR..=8 * SR).contains(&r.samples.len())));
    assert!(short.iter().all(|r| r.samples.len() < 3 * SR));
    // The repetition test relies on the right answers not repeating themselves.
    assert!(short.iter().all(|r| !repeats_itself(&r.text)));
}

// ---- tests that need speech models ----

#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn dictation_length_recordings_are_transcribed_accurately() {
    let clips = recordings("clips");
    for model in test_models() {
        let transcripts: Vec<String> = clips.iter().map(|r| transcribe(model, &r.samples)).collect();
        let rate = error_rate(
            clips
                .iter()
                .map(|r| r.text.as_str())
                .zip(transcripts.iter().map(String::as_str)),
        );
        eprintln!("{}: {rate:.1}% word errors on 12 clips", model.id);
        assert!(
            rate <= limits(model.id).clips,
            "{}: {rate:.1}% word errors on the dictation clips, limit {}%.\n{}",
            model.id,
            limits(model.id).clips,
            side_by_side(&clips, &transcripts)
        );
    }
}

#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn short_recordings_are_transcribed_accurately() {
    let short = recordings("short");
    for model in test_models() {
        let transcripts: Vec<String> = short.iter().map(|r| transcribe(model, &r.samples)).collect();
        let rate = error_rate(
            short
                .iter()
                .map(|r| r.text.as_str())
                .zip(transcripts.iter().map(String::as_str)),
        );
        eprintln!("{}: {rate:.1}% word errors on 8 short recordings", model.id);
        assert!(
            rate <= limits(model.id).short,
            "{}: {rate:.1}% word errors on the short recordings, limit {}%.\n{}",
            model.id,
            limits(model.id).short,
            side_by_side(&short, &transcripts)
        );
    }
}

/// The 0.4.0 regression: on a one-to-two-second recording, Whisper wrote the phrase and then
/// started it again. Cut every recording down to its first second and a half and check that
/// no model does that.
#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn very_short_recordings_are_not_repeated() {
    let mut all = recordings("clips");
    all.extend(recordings("short"));
    for model in test_models() {
        let mut repeated = Vec::new();
        for r in &all {
            let start = SR / 5;
            let piece = &r.samples[start..(start + SR * 3 / 2).min(r.samples.len())];
            let transcript = transcribe(model, piece);
            // Nobody says more than about seven words in a second and a half.
            let too_long = words(&transcript).len() > 9;
            if repeats_itself(&transcript) || too_long {
                repeated.push(format!("  {}: {transcript:?}", r.name));
            }
        }
        eprintln!(
            "{}: {} of {} very short recordings repeated",
            model.id,
            repeated.len(),
            all.len()
        );
        // One may slip through: a cut can land mid-word and confuse any model.
        assert!(
            repeated.len() <= 1,
            "{}: repeated or over-long text on {} of {} very short recordings:\n{}",
            model.id,
            repeated.len(),
            all.len(),
            repeated.join("\n")
        );
    }
}

#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn the_same_recording_gives_the_same_text() {
    let clip = &recordings("clips")[1];
    for model in test_models() {
        let first = transcribe(model, &clip.samples);
        assert_eq!(first, transcribe(model, &clip.samples), "{}", model.id);
        assert!(!first.is_empty(), "{}", model.id);
    }
}

/// Models invent words when given nothing ("Thank you.", "you"). The app refuses to
/// transcribe a silent recording, but the engine should still not produce a sentence.
#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn silence_does_not_become_a_sentence() {
    // Faint noise, like a microphone in a quiet room.
    let hiss: Vec<f32> = (0..3 * SR)
        .map(|i| ((i as f32 * 12.9898).sin() * 43_758.547).fract() * 0.002)
        .collect();
    for model in test_models() {
        for (what, samples) in [("silence", silence(3.0)), ("faint noise", hiss.clone())] {
            let transcript = transcribe(model, &samples);
            assert!(
                words(&transcript).len() <= 3,
                "{}: {what} was transcribed as {transcript:?}",
                model.id
            );
        }
    }
}

/// A recording longer than 25 seconds is split at pauses. Nothing may be lost or mangled at
/// the joins.
#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn long_recordings_are_split_without_losing_words() {
    let (samples, reference) = long_recording();
    assert!(samples.len() > 60 * SR, "the long recording should be over a minute");
    for model in test_models() {
        let transcript = transcribe(model, &samples);
        let rate = error_rate([(reference.as_str(), transcript.as_str())]);
        let (said, expected) = (words(&transcript).len(), words(&reference).len());
        eprintln!(
            "{}: {rate:.1}% word errors on a {} s recording",
            model.id,
            samples.len() / SR
        );
        assert!(
            said * 100 >= expected * 90,
            "{}: only {said} of {expected} words came back from the long recording:\n{transcript}",
            model.id
        );
        assert!(
            rate <= limits(model.id).clips + 4.0,
            "{}: {rate:.1}% word errors on the long recording:\n{transcript}",
            model.id
        );
    }
}

/// Microphones record at 44.1 or 48 kHz, some headsets at 8 or 22 kHz. The audio is
/// converted to 16 kHz before recognition; the conversion must not hurt.
#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn other_sample_rates_are_converted_correctly() {
    let clips = recordings("clips");
    let clip = &clips[1];
    for model in test_models() {
        let direct = transcribe(model, &clip.samples);
        for rate in [48_000u32, 44_100, 22_050] {
            let recorded = audio::linear_resample(&clip.samples, SAMPLE_RATE, rate);
            let converted = audio::resample(&recorded, rate);
            let ratio = converted.len() as f64 / clip.samples.len() as f64;
            assert!(
                (0.99..=1.01).contains(&ratio),
                "{rate} Hz changed the length by {ratio}"
            );
            let transcript = transcribe(model, &converted);
            let (errors, n) = word_errors(&direct, &transcript);
            assert!(
                errors * 100 <= n * 15,
                "{}: at {rate} Hz the text changed.\n  16 kHz: {direct}\n  {rate} Hz: {transcript}",
                model.id
            );
        }
    }
}

/// "Write as you speak": the same audio, fed in 50 ms pieces as the recorder does, must come
/// out as several pieces of text that add up to the whole thing.
#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn writing_as_you_speak_writes_everything_once() {
    let (samples, reference) = long_recording();
    let wav = std::env::temp_dir().join(format!("sayso-live-test-{}.wav", std::process::id()));
    assert!(sherpa_onnx::write(&wav.to_string_lossy(), &samples, SAMPLE_RATE as i32));

    for model in test_models() {
        crate::rpc::set_quiet(true);
        let (tx, _rx) = mpsc::channel();
        let app = App::new(data_dir(), tx);
        {
            let mut settings = app.settings.lock().unwrap();
            settings.model_id = Some(model.id.to_string());
            settings.language = "en".into();
            settings.replacements.clear();
        }
        let pieces = app
            .transcribe_file_live(&wav.to_string_lossy())
            .expect("live transcription");
        let joined = pieces.concat();
        let rate = error_rate([(reference.as_str(), joined.as_str())]);
        eprintln!(
            "{}: {} pieces, {rate:.1}% word errors written live",
            model.id,
            pieces.len()
        );
        assert!(
            pieces.len() >= 6,
            "{}: expected a piece per pause, got {}",
            model.id,
            pieces.len()
        );
        assert!(
            pieces.iter().all(|p| p.ends_with(' ') || p.ends_with('\n')),
            "pieces must not run together"
        );
        assert!(
            rate <= limits(model.id).clips + 4.0,
            "{}: {rate:.1}% word errors when writing live:\n{joined}",
            model.id
        );
    }
    let _ = std::fs::remove_file(wav);
}

#[test]
#[ignore = "needs speech models; see the top of core/src/audio_tests.rs"]
fn voice_activity_detection_finds_speech_and_ignores_silence() {
    let vad_model = models::vad_path(&models_dir());
    assert!(
        vad_model.is_file(),
        "the voice-activity model is missing; it comes with any model download"
    );
    let count = |samples: &[f32]| {
        let vad = engine::create_vad(&vad_model, 0.3, 25.0).expect("voice-activity model loads");
        let mut found = 0;
        let mut bump = |_: &[f32]| found += 1;
        for window in samples.chunks(engine::VAD_WINDOW) {
            vad.accept_waveform(window);
            engine::drain_vad(&vad, &mut bump);
        }
        vad.flush();
        engine::drain_vad(&vad, &mut bump);
        found
    };
    assert_eq!(count(&silence(5.0)), 0, "silence must contain no speech");
    let (long, _) = long_recording();
    let segments = count(&long);
    assert!(
        (8..=30).contains(&segments),
        "expected roughly one segment per clip, found {segments}"
    );
}

fn side_by_side(recordings: &[Recording], transcripts: &[String]) -> String {
    recordings
        .iter()
        .zip(transcripts)
        .map(|(r, t)| {
            format!(
                "  {}\n    expected: {}\n    got:      {}",
                r.name,
                r.text.trim(),
                t.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
