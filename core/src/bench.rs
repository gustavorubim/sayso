//! `sayso-core bench`: measures how fast dictation is, and what each tuning knob changes.
//!
//!   sayso-core bench --data-dir <dir with models/> --wav <long speech.wav> [--models a,b] [--quick]
//!
//! Everything is printed as tab-separated rows so results can be pasted into a table.
//! The benchmark never pastes or types anything.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::audio::{self, SAMPLE_RATE};
use crate::engine::{self, Engine, EngineOptions};
use crate::models::{self, ModelDef};

const SR: usize = SAMPLE_RATE as usize;

struct Args {
    data_dir: PathBuf,
    wav: PathBuf,
    only: Option<Vec<String>>,
    quick: bool,
    provider: String,
    sections: Option<Vec<String>>,
    background: bool,
    eval: Option<PathBuf>,
    limit: usize,
    whisper_padding: i32,
}

fn parse_args() -> Result<Args, String> {
    let mut data_dir = None;
    let mut wav = None;
    let mut only = None;
    let mut quick = false;
    let mut provider = "cpu".to_string();
    let mut sections = None;
    let mut background = false;
    let mut eval = None;
    let mut limit = usize::MAX;
    let mut whisper_padding = EngineOptions::default().whisper_tail_paddings;
    let mut args = std::env::args().skip(2);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--wav" => wav = args.next().map(PathBuf::from),
            "--models" => only = args.next().map(|s| s.split(',').map(str::to_string).collect()),
            "--quick" => quick = true,
            "--background" => background = true,
            "--eval" => eval = args.next().map(PathBuf::from),
            "--whisper-padding" => {
                whisper_padding = args.next().and_then(|n| n.parse().ok()).unwrap_or(whisper_padding)
            }
            "--limit" => limit = args.next().and_then(|n| n.parse().ok()).unwrap_or(usize::MAX),
            "--provider" => provider = args.next().unwrap_or_default(),
            "--sections" => sections = args.next().map(|s| s.split(',').map(str::to_string).collect()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        data_dir: data_dir.ok_or("--data-dir is required")?,
        wav: wav.ok_or("--wav is required")?,
        only,
        quick,
        provider,
        sections,
        background,
        eval,
        limit,
        whisper_padding,
    })
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Fastest of `runs` calls, in milliseconds, and the last result. The minimum is what the
/// code can do; anything slower is the rest of the machine getting in the way.
fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut times = Vec::with_capacity(runs);
    let mut last = None;
    for _ in 0..runs {
        let t = Instant::now();
        last = Some(f());
        times.push(ms(t.elapsed()));
    }
    (times.iter().cloned().fold(f64::INFINITY, f64::min), last.unwrap())
}

/// Memory this process holds, in megabytes. 0 where it cannot be measured (macOS).
#[cfg(windows)]
pub(crate) fn memory_mb() -> f64 {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 {
            return counters.WorkingSetSize as f64 / 1_048_576.0;
        }
    }
    0.0
}

#[cfg(target_os = "linux")]
pub(crate) fn memory_mb() -> f64 {
    // "VmRSS:   123456 kB"
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub(crate) fn memory_mb() -> f64 {
    0.0
}

/// Word errors (substitutions, insertions, deletions) in `other`, and the number of words in
/// `reference`. Case and punctuation are ignored.
pub(crate) fn word_errors(reference: &str, other: &str) -> (usize, usize) {
    let norm = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '\'')
                    .collect::<String>()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };
    let (a, b) = (norm(reference), norm(other));
    if a.is_empty() {
        return (b.len(), 0);
    }
    // Levenshtein distance over words.
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, wa) in a.iter().enumerate() {
        let mut row = vec![i + 1];
        for (j, wb) in b.iter().enumerate() {
            let cost = usize::from(wa != wb);
            row.push((prev[j] + cost).min(prev[j + 1] + 1).min(row[j] + 1));
        }
        prev = row;
    }
    (prev[b.len()], a.len())
}

struct Clip {
    name: &'static str,
    samples: Vec<f32>,
}

impl Clip {
    fn seconds(&self) -> f64 {
        self.samples.len() as f64 / SR as f64
    }
}

fn load(model: &ModelDef, models_dir: &Path, options: &EngineOptions) -> Result<Engine, String> {
    Engine::load_with(model, models_dir, "en", options)
}

pub fn run() -> i32 {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    // The app always asks the system not to throttle the engine. Measure the same way, unless
    // `--background` asks to see what an ordinary background process gets.
    if args.background {
        println!("# running as an ordinary background process");
    } else {
        println!("# full speed requested: {}", crate::perf::request_full_speed());
    }
    let models_dir = args.data_dir.join("models");
    let Some(wave) = sherpa_onnx::Wave::read(&args.wav.to_string_lossy()) else {
        eprintln!("could not read {}", args.wav.display());
        return 2;
    };
    let speech = audio::resample(wave.samples(), wave.sample_rate() as u32);
    // Skip the opening applause, then cut clips that start and end between utterances, the
    // way a real dictation does. Cutting mid-word sends some models down slow fallback paths.
    let from = 21 * SR;
    let bounds = utterance_bounds(&speech[from..], &models::vad_path(&models_dir));
    if bounds.is_empty() {
        eprintln!("the voice-activity model is missing; download any model first");
        return 2;
    }
    let clip = |name, seconds: usize| {
        let start = bounds[0].0.saturating_sub(SR / 10);
        // The last utterance that ends within the target length (always at least one).
        let end = bounds
            .iter()
            .map(|b| b.1)
            .take_while(|&e| e - start <= seconds * SR + SR / 2)
            .last()
            .unwrap_or(bounds[0].1);
        let end = (end + SR / 10).min(speech.len() - from);
        Clip {
            name,
            samples: speech[from + start..from + end].to_vec(),
        }
    };
    let clips = [clip("3s", 3), clip("10s", 10), clip("30s", 30)];
    println!(
        "# clip lengths: {}",
        clips
            .iter()
            .map(|c| format!("{} = {:.1} s", c.name, c.seconds()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let runs = if args.quick { 3 } else { 7 };
    let base = EngineOptions {
        provider: args.provider.clone(),
        whisper_tail_paddings: args.whisper_padding,
        ..EngineOptions::default()
    };
    let wanted = |section: &str| {
        args.sections
            .as_ref()
            .map(|s| s.iter().any(|x| x == section))
            .unwrap_or(true)
    };
    println!("# provider: {}", base.provider);

    let installed: Vec<&ModelDef> = models::CATALOG
        .iter()
        .filter(|m| m.is_installed(&models_dir))
        .filter(|m| {
            args.only
                .as_ref()
                .map(|o| o.iter().any(|id| id == m.id))
                .unwrap_or(true)
        })
        .collect();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!("# logical cores: {cores}; default threads: {}", base.threads);
    println!(
        "# models: {}",
        installed.iter().map(|m| m.id).collect::<Vec<_>>().join(", ")
    );

    // ---- Accuracy on a labelled test set ----
    // `--eval <dir>` holds pairs of files: `name.wav` and `name.txt` with the correct words.
    // Reports the word error rate (errors per 100 reference words) and the speed for each
    // model. Runs only when asked for, because it needs a test set.
    if let Some(dir) = &args.eval {
        let mut items: Vec<(Vec<f32>, String)> = Vec::new();
        let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        names.sort();
        for wav in names.iter().filter(|p| p.extension().is_some_and(|e| e == "wav")) {
            if items.len() >= args.limit {
                break;
            }
            let Ok(text) = std::fs::read_to_string(wav.with_extension("txt")) else {
                continue;
            };
            let Some(wave) = sherpa_onnx::Wave::read(&wav.to_string_lossy()) else {
                continue;
            };
            items.push((audio::resample(wave.samples(), wave.sample_rate() as u32), text));
        }
        let seconds: f64 = items.iter().map(|(a, _)| a.len() as f64 / SR as f64).sum();
        println!(
            "\n# accuracy set: {} recordings, {:.0} s of audio",
            items.len(),
            seconds
        );
        println!("## accuracy\tmodel\twer_pct\terrors\twords\ttime_s\ttimes_faster_than_real_time");
        for model in &installed {
            let Ok(engine) = load(model, &models_dir, &base) else {
                println!("accuracy\t{}\tFAILED to load", model.id);
                continue;
            };
            let _ = engine.transcribe(&items[0].0); // warm up
            let (mut errors, mut words) = (0usize, 0usize);
            let t = Instant::now();
            for (samples, reference) in &items {
                let (e, n) = word_errors(reference, &engine.transcribe(samples));
                errors += e;
                words += n;
            }
            let elapsed = t.elapsed().as_secs_f64();
            println!(
                "accuracy\t{}\t{:.2}\t{errors}\t{words}\t{elapsed:.1}\t{:.1}",
                model.id,
                100.0 * errors as f64 / words.max(1) as f64,
                seconds / elapsed
            );
        }
        return 0;
    }

    // ---- 0. Stability: the same measurement on five freshly loaded engines ----
    // If these disagree, something outside the model (core scheduling, throttling) is deciding
    // the speed, and that matters more than any tuning knob.
    if wanted("stability") {
        println!("\n## stability\tmodel\tthreads\trun1_ms\trun2_ms\trun3_ms\trun4_ms\trun5_ms\tworst_over_best");
        for model in &installed {
            for threads in [4, 6] {
                let options = EngineOptions {
                    threads,
                    ..base.clone()
                };
                let mut results = Vec::new();
                for _ in 0..5 {
                    let Ok(engine) = load(model, &models_dir, &options) else {
                        continue;
                    };
                    let _ = engine.transcribe(&clips[0].samples);
                    results.push(time(3, || engine.transcribe(&clips[1].samples)).0);
                }
                if results.is_empty() {
                    continue;
                }
                let best = results.iter().cloned().fold(f64::INFINITY, f64::min);
                let worst = results.iter().cloned().fold(0.0, f64::max);
                println!(
                    "stability\t{}\t{threads}\t{}\t{:.2}",
                    model.id,
                    results.iter().map(|r| format!("{r:.0}")).collect::<Vec<_>>().join("\t"),
                    worst / best
                );
            }
        }
    }

    // ---- 1. Baseline: every model with the settings the app ships with ----
    if wanted("baseline") {
        println!("\n## baseline\tmodel\tload_ms\tmem_mb\tcold_10s_ms\twarm_3s_ms\twarm_10s_ms\twarm_30s_ms\trtf_10s");
        let mut reference: Vec<(String, String)> = Vec::new();
        for model in &installed {
            let before = memory_mb();
            let t = Instant::now();
            let engine = match load(model, &models_dir, &base) {
                Ok(e) => e,
                Err(e) => {
                    println!("baseline\t{}\tFAILED: {e}", model.id);
                    continue;
                }
            };
            let load_ms = ms(t.elapsed());
            let mem = memory_mb() - before;
            let t = Instant::now();
            let _ = engine.transcribe(&clips[1].samples);
            let cold = ms(t.elapsed());
            let warm: Vec<(f64, String)> = clips
                .iter()
                .map(|c| time(runs, || engine.transcribe(&c.samples)))
                .collect();
            println!(
                "baseline\t{}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.3}",
                model.id,
                load_ms,
                mem,
                cold,
                warm[0].0,
                warm[1].0,
                warm[2].0,
                warm[1].0 / 1000.0 / clips[1].seconds()
            );
            reference.push((model.id.to_string(), warm[1].1.clone()));
        }
        println!("\n## transcript of the 10 s clip per model");
        for (id, text) in &reference {
            println!("text\t{id}\t{text}");
        }
    }
    // ---- 2. Threads ----
    if wanted("threads") {
        println!("\n## threads\tmodel\tthreads\twarm_3s_ms\twarm_10s_ms");
        let mut counts: Vec<i32> = vec![1, 2, 4, 6, 8, 12, 16, 24];
        counts.retain(|&n| n as usize <= cores);
        for model in &installed {
            for &threads in &counts {
                let options = EngineOptions {
                    threads,
                    ..base.clone()
                };
                let Ok(engine) = load(model, &models_dir, &options) else {
                    continue;
                };
                let _ = engine.transcribe(&clips[0].samples); // warm up
                let (t3, _) = time(runs, || engine.transcribe(&clips[0].samples));
                let (t10, _) = time(runs, || engine.transcribe(&clips[1].samples));
                println!("threads\t{}\t{threads}\t{t3:.0}\t{t10:.0}", model.id);
            }
        }
    }
    // ---- 3. Pipeline costs outside the model ----
    if wanted("pipeline") {
        let vad_path = models::vad_path(&models_dir);
        println!("\n## pipeline\tstep\tmedian_ms\tnote");
        let raw48: Vec<f32> = speech[from..from + 10 * SR].iter().flat_map(|s| [*s, *s, *s]).collect();
        let (resample_ms, _) = time(runs, || audio::resample(&raw48, 48_000));
        println!("pipeline\tresample 10 s of 48 kHz audio\t{resample_ms:.1}\tdone once, when recording stops");
        let (vad_ms, _) = time(runs, || trim_silence(&clips[1].samples, &vad_path));
        println!("pipeline\tvoice-activity pass over 10 s\t{vad_ms:.1}\tlive mode runs this continuously");
        let (vad_load, _) = time(runs, || engine::create_vad(&vad_path, 0.45, 15.0).is_some());
        println!("pipeline\tload the voice-activity model\t{vad_load:.1}\tpaid at the start of every live recording");

        let mut open = Vec::new();
        let mut close = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            match audio::start(audio::Source::Microphone(None), None) {
                Ok(recorder) => {
                    open.push(ms(t.elapsed()));
                    std::thread::sleep(Duration::from_millis(400));
                    let t = Instant::now();
                    let _ = recorder.finish();
                    close.push(ms(t.elapsed()));
                }
                Err(e) => {
                    println!("pipeline\topen the microphone\tFAILED\t{e}");
                    break;
                }
            }
        }
        if !open.is_empty() {
            println!(
                "pipeline\topen the microphone\t{:.0}\tspeech during this window is lost",
                median(open)
            );
            println!(
                "pipeline\tstop recording and collect audio\t{:.0}\tincludes a 50 ms polling sleep",
                median(close)
            );
        }
        // Computer audio: open the loopback, listen briefly, and report what arrived.
        // Nothing is transcribed or pasted.
        let (tx, rx) = std::sync::mpsc::channel();
        let t = Instant::now();
        match audio::start(audio::Source::SystemAudio, Some(tx)) {
            Ok(recorder) => {
                let opened = ms(t.elapsed());
                std::thread::sleep(Duration::from_millis(2500));
                let _ = recorder.finish();
                let heard: Vec<f32> = rx.iter().flatten().collect();
                println!(
                    "pipeline\topen computer audio capture\t{opened:.0}\t{:.1} s of 16 kHz audio received, level {:.4}, peak {:.3}",
                    heard.len() as f64 / SR as f64,
                    audio::rms(&heard),
                    audio::peak(&heard)
                );
            }
            Err(e) => println!("pipeline\topen computer audio capture\tFAILED\t{e}"),
        }
        println!("pipeline\tpaste: fixed wait before the keystroke\t70\thard-coded sleep in paste.rs");
        println!("pipeline\thotkey polling interval\t8\tworst-case delay noticing a key press");
    }
    0
}

/// Start and end (in samples) of every utterance the voice-activity model finds.
fn utterance_bounds(samples: &[f32], vad_model: &Path) -> Vec<(usize, usize)> {
    let Some(vad) = engine::create_vad(vad_model, 0.3, 25.0) else {
        return Vec::new();
    };
    let mut bounds = Vec::new();
    let mut collect = |vad: &sherpa_onnx::VoiceActivityDetector| {
        while !vad.is_empty() {
            if let Some(segment) = vad.front() {
                let start = segment.start() as usize;
                bounds.push((start, start + segment.n() as usize));
            }
            vad.pop();
        }
    };
    for window in samples.chunks(engine::VAD_WINDOW) {
        vad.accept_waveform(window);
        collect(&vad);
    }
    vad.flush();
    collect(&vad);
    bounds
}

/// Returns only the speech in `samples`, with silence at the ends and long pauses removed.
/// Falls back to the input if no speech is found or the detector is unavailable.
pub fn trim_silence(samples: &[f32], vad_model: &Path) -> Vec<f32> {
    let Some(vad) = engine::create_vad(vad_model, 0.3, 25.0) else {
        return samples.to_vec();
    };
    let mut speech = Vec::with_capacity(samples.len());
    let mut keep = |segment: &[f32]| speech.extend_from_slice(segment);
    for window in samples.chunks(engine::VAD_WINDOW) {
        vad.accept_waveform(window);
        engine::drain_vad(&vad, &mut keep);
    }
    vad.flush();
    engine::drain_vad(&vad, &mut keep);
    if speech.is_empty() {
        samples.to_vec()
    } else {
        speech
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_errors_counts_substitutions_insertions_and_deletions() {
        assert_eq!(word_errors("the cat sat", "the cat sat"), (0, 3));
        assert_eq!(word_errors("the cat sat", "the dog sat"), (1, 3));
        assert_eq!(word_errors("the cat sat", "the cat"), (1, 3));
        assert_eq!(word_errors("the cat sat", "the big cat sat"), (1, 3));
        assert_eq!(word_errors("the cat sat", ""), (3, 3));
    }

    #[test]
    fn word_errors_ignores_case_and_punctuation_but_not_apostrophes() {
        assert_eq!(word_errors("hello world", "Hello, World!"), (0, 2));
        assert_eq!(word_errors("don't stop", "Don't stop."), (0, 2));
        assert_eq!(word_errors("dont stop", "don't stop"), (1, 2));
    }

    #[test]
    fn word_errors_with_an_empty_reference() {
        assert_eq!(word_errors("", ""), (0, 0));
        assert_eq!(word_errors("", "two words"), (2, 0));
    }

    #[test]
    fn a_repeated_phrase_counts_as_errors() {
        // The 0.4.0 Whisper regression: the phrase written, then started again.
        let (errors, words) = word_errors("this is a test", "This is a test. This is a test");
        assert_eq!((errors, words), (4, 4));
    }

    #[test]
    fn median_and_best_time() {
        assert_eq!(median(vec![5.0, 1.0, 3.0]), 3.0);
        let mut calls = 0;
        let (best, last) = time(4, || {
            calls += 1;
            calls
        });
        assert_eq!(last, 4);
        assert!(best >= 0.0);
    }
}
