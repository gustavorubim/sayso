//! Stress tests: what happens when things are done many times, at once, or go wrong.
//!
//! The first half needs nothing but the code and runs with every `cargo test`: the request
//! protocol under load, files being saved while they are read, and downloads that are cut
//! off, corrupted or oversized, against a small web server started inside the test.
//!
//! The second half needs speech models (see `audio_tests.rs`) and is `#[ignore]`d:
//!
//! ```text
//! SAYSO_TEST_DATA=<folder> cargo test stress_tests -- --ignored --test-threads=1
//! ```
//!
//! It dictates hundreds of times in a row, switches models back and forth, transcribes a
//! ten-minute recording, and feeds the engine audio no microphone should produce. CI runs it
//! on every push to main and before every release, not on pull requests: it takes minutes.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::audio::SAMPLE_RATE;
use crate::audio_tests::{data_dir, error_rate, limits, long_recording, recordings, test_models, transcribe, words};
use crate::download::{fetch_with, hex, Expected, CANCELLED};
use crate::history;
use crate::rpc::dispatch;
use crate::settings::Settings;
use crate::test_support::{app, temp_dir};

const SR: usize = SAMPLE_RATE as usize;

/// Runs `work` on another thread and fails the test if it has not finished in time. A
/// deadlock would otherwise hang the whole test run with no explanation.
fn within<T: Send + 'static>(limit: Duration, what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(limit)
        .unwrap_or_else(|_| panic!("{what} did not finish within {limit:?}: deadlock or runaway loop"))
}

// ---------------------------------------------------------------------------------------
// Protocol and state under load. No models.
// ---------------------------------------------------------------------------------------

#[test]
fn requests_from_many_threads_at_once_do_not_deadlock_or_corrupt_state() {
    let (app, dir) = app();
    let done = within(Duration::from_secs(120), "the request flood", move || {
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let app = app.clone();
                std::thread::spawn(move || {
                    for i in 0..300 {
                        match (t + i) % 7 {
                            0 => {
                                let mut s = app.settings.lock().unwrap().clone();
                                s.remove_fillers = i % 2 == 0;
                                s.trailing_space = t % 2 == 0;
                                app.save_settings(s).unwrap();
                            }
                            1 => app.remember(&format!("thread {t} dictation {i}"), 1000, false),
                            2 => {
                                let listed = dispatch(&app, "get_history", Value::Null).unwrap();
                                if let Some(first) = listed.as_array().and_then(|a| a.first()) {
                                    let _ = dispatch(&app, "delete_history_item", json!({"id": first["id"]}));
                                }
                            }
                            3 => assert!(dispatch(&app, "list_models", Value::Null).is_ok()),
                            4 => assert!(dispatch(&app, "get_settings", Value::Null).is_ok()),
                            5 => assert!(dispatch(&app, "get_status", Value::Null).is_ok()),
                            _ => {
                                app.on_hotkey(i % 3, true);
                                app.on_hotkey(i % 3, false);
                            }
                        }
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().expect("a request thread panicked");
        }
        app
    });

    // Whatever order things happened in, what is on disk is whole and matches memory.
    let on_disk = std::fs::read_to_string(dir.join("settings.json")).unwrap();
    let saved: Settings = serde_json::from_str(&on_disk).expect("settings.json is valid after the flood");
    assert_eq!(saved, *done.settings.lock().unwrap());
    let in_memory = done.history.lock().unwrap().len();
    assert_eq!(history::load(&dir.join("history.json")).len(), in_memory);
    assert_eq!(
        dispatch(&done, "get_status", Value::Null).unwrap(),
        json!({"phase": "idle"})
    );
}

#[test]
fn no_request_however_malformed_can_crash_the_engine() {
    let (app, _) = app();
    let methods = [
        "get_settings",
        "save_settings",
        "get_default_settings",
        "get_status",
        "list_models",
        "download_model",
        "cancel_download",
        "delete_model",
        "list_input_devices",
        "get_history",
        "delete_history_item",
        "clear_history",
        "transcribe_file",
        "rewrite_text",
        "copy_text_is_skipped", // would touch the real clipboard
        "set_api_key",
        "toggle_recording",
        "toggle_capture",
        "",
        "no such method",
        "🙂",
    ];
    let deep = (0..200).fold(json!(1), |inner, _| json!([inner]));
    let junk = [
        Value::Null,
        json!(true),
        json!(-1),
        json!(1.5e300),
        json!(""),
        json!("x".repeat(1_000_000)),
        json!([]),
        json!({}),
        json!({"id": null}),
        json!({"id": 12}),
        json!({"id": -5}),
        json!({"id": "../../../etc/passwd"}),
        json!({"id": "a\u{0}b"}),
        json!({"id": "x".repeat(100_000)}),
        json!({"path": ""}),
        json!({"path": "\u{0}"}),
        json!({"path": "CON"}),
        json!({"path": "\\\\?\\C:\\"}),
        json!({"path": 7, "live": "yes"}),
        json!({"text": null}),
        json!({"key": {"nested": true}}),
        json!({"shortcut": "", "mode": 3}),
        json!({"shortcut": "Ctrl+Shift+Space", "replacements": "not a list"}),
        json!({"replacements": [{"from": "(", "to": "$1"}], "shortcut": "Ctrl+Shift+Space"}),
        deep,
    ];
    let mut panics = Vec::new();
    for method in methods {
        for params in &junk {
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(&app, method, params.clone())));
            if outcome.is_err() {
                panics.push(format!(
                    "{method} with {}",
                    params.to_string().chars().take(60).collect::<String>()
                ));
            }
        }
    }
    assert!(
        panics.is_empty(),
        "these requests made a handler panic:\n{}",
        panics.join("\n")
    );
    // And it still answers afterwards.
    assert!(dispatch(&app, "get_settings", Value::Null).is_ok());
}

#[test]
fn mashing_every_shortcut_without_a_model_does_nothing_harmful() {
    let (app, _) = app();
    within(Duration::from_secs(60), "shortcut mashing", move || {
        for i in 0..3000 {
            let slot = i % 3;
            app.on_hotkey(slot, true);
            if i % 5 != 0 {
                app.on_hotkey(slot, false); // sometimes a release goes missing
            }
            if i % 11 == 0 {
                app.on_hotkey((slot + 1) % 3, false); // or arrives for a key that was never pressed
            }
        }
        assert_eq!(app.status().phase, "idle");
    });
}

#[test]
fn history_never_grows_past_its_limit() {
    let (app, dir) = app();
    for i in 0..history::MAX_ITEMS + 250 {
        app.remember(&format!("dictation number {i}"), 900, i % 2 == 0);
    }
    let items = app.history.lock().unwrap().clone();
    assert_eq!(items.len(), history::MAX_ITEMS);
    assert_eq!(
        items[0].text,
        format!("dictation number {}", history::MAX_ITEMS + 249),
        "newest first"
    );
    assert_eq!(history::load(&dir.join("history.json")).len(), history::MAX_ITEMS);

    // Nothing is kept when history is switched off, and empty text is never an entry.
    app.settings.lock().unwrap().save_history = false;
    app.remember("private", 900, false);
    assert_ne!(app.history.lock().unwrap()[0].text, "private");
    app.settings.lock().unwrap().save_history = true;
    app.remember("   ", 900, false);
    assert_eq!(app.history.lock().unwrap().len(), history::MAX_ITEMS);
}

/// Settings are rewritten on every change. If the app is killed, or the power goes, half-way
/// through a write, the next start must not find half a file and fall back to defaults.
#[test]
fn a_reader_never_sees_a_half_written_settings_file() {
    let dir = temp_dir("atomic");
    let path = dir.join("settings.json");
    Settings::default().save(&path).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let bad = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(AtomicUsize::new(0));
    let reader = {
        let (stop, bad, reads, path) = (stop.clone(), bad.clone(), reads.clone(), path.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // A read can fail outright for an instant while the file is being swapped
                // on Windows; what must never happen is reading a broken document.
                if let Ok(text) = std::fs::read_to_string(&path) {
                    reads.fetch_add(1, Ordering::Relaxed);
                    if serde_json::from_str::<Settings>(&text).is_err() {
                        bad.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
    };

    let mut settings = Settings::default();
    for i in 0..400 {
        // Vary the size a lot, so a torn write would be visible.
        settings.rewrite_prompt = "x".repeat((i % 40) * 500);
        settings.save(&path).unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    reader.join().unwrap();

    assert!(
        reads.load(Ordering::Relaxed) > 50,
        "the reader should have had a real chance to catch a bad file"
    );
    assert_eq!(
        bad.load(Ordering::Relaxed),
        0,
        "a reader saw a partially written settings file"
    );
    assert_eq!(Settings::load(&path), settings);
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary files were left behind");
}

// ---------------------------------------------------------------------------------------
// Downloads that go wrong, against a web server inside the test.
// ---------------------------------------------------------------------------------------

/// How the test server answers one connection.
#[derive(Clone)]
enum Reply {
    /// The file, honouring a `Range` request the way Hugging Face does.
    File,
    /// The file, but the connection is cut after this many bytes of body.
    CutAfter(usize),
    /// The whole file from the start, whatever range was asked for.
    IgnoreRange,
    /// Something else entirely, with a 200.
    Body(Vec<u8>),
    /// An error status.
    Status(u16),
    /// One byte every few milliseconds.
    Slow,
}

struct Server {
    url: String,
    /// The `Range` header of each request, in order (`None` if there was none).
    ranges: Arc<Mutex<Vec<Option<String>>>>,
}

fn serve(file: Vec<u8>, replies: Vec<Reply>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free local port");
    let url = format!("http://{}/model.onnx", listener.local_addr().unwrap());
    let ranges: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let seen = ranges.clone();
    std::thread::spawn(move || {
        for reply in replies {
            let Ok((mut socket, _)) = listener.accept() else { return };
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                match socket.read(&mut byte) {
                    Ok(1) => request.push(byte[0]),
                    _ => break,
                }
            }
            let request = String::from_utf8_lossy(&request).to_string();
            let range = request
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                .map(|l| l[6..].trim().to_string());
            seen.lock().unwrap().push(range.clone());
            let start = range
                .as_deref()
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.trim_end_matches('-').parse::<usize>().ok())
                .unwrap_or(0);

            let head = |status: &str, length: usize| {
                format!("HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
            };
            let partial = start > 0 && start < file.len();
            let (status, body): (&str, &[u8]) = if partial {
                ("206 Partial Content", &file[start..])
            } else {
                ("200 OK", &file)
            };
            match reply {
                Reply::File => {
                    if start >= file.len() && range.is_some() {
                        let _ = socket.write_all(head("416 Range Not Satisfiable", 0).as_bytes());
                    } else {
                        let _ = socket.write_all(head(status, body.len()).as_bytes());
                        let _ = socket.write_all(body);
                    }
                }
                Reply::CutAfter(n) => {
                    let _ = socket.write_all(head(status, body.len()).as_bytes());
                    let _ = socket.write_all(&body[..n.min(body.len())]);
                    let _ = socket.flush();
                    // Dropping the socket here ends the response early.
                }
                Reply::IgnoreRange => {
                    let _ = socket.write_all(head("200 OK", file.len()).as_bytes());
                    let _ = socket.write_all(&file);
                }
                Reply::Body(other) => {
                    let _ = socket.write_all(head("200 OK", other.len()).as_bytes());
                    let _ = socket.write_all(&other);
                }
                Reply::Status(code) => {
                    let _ = socket.write_all(head(&format!("{code} Error"), 0).as_bytes());
                }
                Reply::Slow => {
                    let _ = socket.write_all(head(status, body.len()).as_bytes());
                    for b in body {
                        if socket.write_all(&[*b]).is_err() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            }
        }
    });
    Server { url, ranges }
}

/// A "model file": 300 kB that does not repeat, so a splice in the wrong place is detected.
fn model_file() -> Vec<u8> {
    (0..300_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn download(server: &Server, dest: &Path, file: &[u8], cancel: &AtomicBool) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new().timeout_read(Duration::from_secs(10)).build();
    let checksum = sha256(file);
    let expected = Expected {
        size: file.len() as u64,
        sha256: &checksum,
    };
    fetch_with(&agent, &server.url, dest, &expected, cancel, &mut |_| {})
}

fn part_of(dest: &Path) -> std::path::PathBuf {
    dest.with_extension("onnx.part")
}

#[test]
fn a_clean_download_is_kept_and_leaves_nothing_else_behind() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    let server = serve(file.clone(), vec![Reply::File]);
    download(&server, &dest, &file, &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), file);
    assert!(!part_of(&dest).exists());
    assert_eq!(*server.ranges.lock().unwrap(), [None]);
}

#[test]
fn a_download_cut_off_midway_resumes_where_it_stopped() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    let server = serve(
        file.clone(),
        vec![Reply::CutAfter(120_000), Reply::CutAfter(50_000), Reply::File],
    );
    let cancel = AtomicBool::new(false);

    let first = download(&server, &dest, &file, &cancel).unwrap_err();
    assert!(first.contains("interrupted"), "{first}");
    assert!(
        !dest.exists(),
        "an incomplete file must never appear under its final name"
    );
    let kept = std::fs::metadata(part_of(&dest)).unwrap().len();
    assert!(kept > 0 && kept <= 120_000, "kept {kept} bytes");

    // Cut off a second time, further along.
    assert!(download(&server, &dest, &file, &cancel).is_err());
    let kept_again = std::fs::metadata(part_of(&dest)).unwrap().len();
    assert!(kept_again > kept, "the second attempt should have added to the first");

    download(&server, &dest, &file, &cancel).unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        file,
        "the resumed file must be byte-for-byte right"
    );
    assert!(!part_of(&dest).exists());

    let ranges = server.ranges.lock().unwrap().clone();
    assert_eq!(ranges[0], None);
    assert_eq!(ranges[1], Some(format!("bytes={kept}-")));
    assert_eq!(ranges[2], Some(format!("bytes={kept_again}-")));
}

#[test]
fn a_file_with_the_wrong_contents_is_thrown_away() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    let mut tampered = file.clone();
    tampered[150_000] ^= 1; // one bit, same size
    let server = serve(file.clone(), vec![Reply::Body(tampered), Reply::File]);
    let cancel = AtomicBool::new(false);

    let err = download(&server, &dest, &file, &cancel).unwrap_err();
    assert!(err.contains("checksum"), "{err}");
    assert!(
        !dest.exists() && !part_of(&dest).exists(),
        "nothing of a bad download may remain"
    );

    // And a retry starts clean.
    download(&server, &dest, &file, &cancel).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), file);
}

#[test]
fn a_file_larger_than_expected_is_stopped_early() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    let mut bloated = file.clone();
    bloated.extend(vec![7u8; 5_000_000]);
    let server = serve(file.clone(), vec![Reply::Body(bloated)]);
    let err = download(&server, &dest, &file, &AtomicBool::new(false)).unwrap_err();
    assert!(err.contains("larger"), "{err}");
    assert!(!dest.exists() && !part_of(&dest).exists());
}

#[test]
fn a_file_shorter_than_expected_is_not_accepted() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    let server = serve(file.clone(), vec![Reply::Body(file[..100_000].to_vec())]);
    assert!(download(&server, &dest, &file, &AtomicBool::new(false)).is_err());
    assert!(!dest.exists());
}

#[test]
fn a_corrupt_leftover_is_caught_when_the_download_resumes() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    // Something else wrote 40 kB of rubbish where a partial download would be.
    std::fs::write(part_of(&dest), vec![0xAAu8; 40_000]).unwrap();
    let server = serve(file.clone(), vec![Reply::File, Reply::File]);
    let cancel = AtomicBool::new(false);

    let err = download(&server, &dest, &file, &cancel).unwrap_err();
    assert!(err.contains("checksum"), "{err}");
    assert!(!dest.exists() && !part_of(&dest).exists());
    download(&server, &dest, &file, &cancel).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), file);
}

#[test]
fn a_server_that_ignores_the_resume_request_still_gives_a_correct_file() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    std::fs::write(part_of(&dest), &file[..90_000]).unwrap();
    let server = serve(file.clone(), vec![Reply::IgnoreRange]);
    download(&server, &dest, &file, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        file,
        "the file must not contain the first 90 kB twice"
    );
}

#[test]
fn a_leftover_as_long_as_the_whole_file_is_not_trusted() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    std::fs::write(part_of(&dest), vec![1u8; file.len()]).unwrap();
    let server = serve(file.clone(), vec![Reply::File]);
    download(&server, &dest, &file, &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), file);
    assert_eq!(
        *server.ranges.lock().unwrap(),
        [None],
        "it must start over, not ask to resume"
    );
}

#[test]
fn cancelling_stops_promptly_and_keeps_what_arrived() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    let server = serve(file.clone(), vec![Reply::Slow]);
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            cancel.store(true, Ordering::SeqCst);
        });
    }
    let started = Instant::now();
    let err = download(&server, &dest, &file, &cancel).unwrap_err();
    assert_eq!(err, CANCELLED);
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "cancelling took {:?}",
        started.elapsed()
    );
    assert!(!dest.exists());
}

#[test]
fn server_errors_are_reported_in_plain_words() {
    let (file, dir) = (model_file(), temp_dir("download"));
    let dest = dir.join("model.onnx");
    for code in [403u16, 404, 500, 503] {
        let server = serve(file.clone(), vec![Reply::Status(code)]);
        let err = download(&server, &dest, &file, &AtomicBool::new(false)).unwrap_err();
        assert!(err.contains(&code.to_string()), "{code}: {err}");
        assert!(!dest.exists());
    }
}

#[test]
fn an_unreachable_server_is_an_error_not_a_hang() {
    let (file, dir) = (model_file(), temp_dir("download"));
    // A port nothing listens on.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let server = Server {
        url: format!("http://127.0.0.1:{port}/model.onnx"),
        ranges: Arc::default(),
    };
    let started = Instant::now();
    let err = download(&server, &dir.join("model.onnx"), &file, &AtomicBool::new(false)).unwrap_err();
    assert!(err.contains("Could not reach"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[test]
fn real_downloads_refuse_plain_http() {
    // The production client must not be talked into an unencrypted connection.
    let (file, dir) = (model_file(), temp_dir("download"));
    let server = serve(file.clone(), vec![Reply::File]);
    let checksum = sha256(&file);
    let expected = Expected {
        size: file.len() as u64,
        sha256: &checksum,
    };
    let agent = ureq::AgentBuilder::new().https_only(true).build();
    let result = fetch_with(
        &agent,
        &server.url,
        &dir.join("model.onnx"),
        &expected,
        &AtomicBool::new(false),
        &mut |_| {},
    );
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------------------
// The engine under load. These need speech models.
// ---------------------------------------------------------------------------------------

fn memory() -> f64 {
    crate::bench::memory_mb()
}

/// An `App` using the test models folder, with `model` selected.
fn app_with_model(model: &str) -> Arc<crate::app::App> {
    crate::rpc::set_quiet(true);
    let (tx, _rx) = std::sync::mpsc::channel();
    let app = Arc::new(crate::app::App::new(data_dir(), tx));
    {
        let mut s = app.settings.lock().unwrap();
        *s = Settings {
            model_id: Some(model.into()),
            language: "en".into(),
            save_history: false,
            ..Settings::default()
        };
    }
    app
}

#[test]
#[ignore = "needs speech models; see the top of core/src/stress_tests.rs"]
fn hundreds_of_dictations_in_a_row_stay_correct_and_do_not_leak_memory() {
    let clips = recordings("clips");
    // The fastest model, so three hundred dictations stay affordable on a small CI machine.
    let model = test_models()
        .into_iter()
        .find(|m| m.id == "canary-180m-flash")
        .unwrap_or(test_models()[0]);
    let expected: Vec<String> = clips.iter().map(|c| transcribe(model, &c.samples)).collect();

    // Let the engine settle before measuring.
    for c in &clips {
        transcribe(model, &c.samples);
    }
    let before = memory();
    let started = Instant::now();
    let rounds = 25;
    for round in 0..rounds {
        for (clip, want) in clips.iter().zip(&expected) {
            assert_eq!(
                &transcribe(model, &clip.samples),
                want,
                "round {round}, {}: the text changed",
                clip.name
            );
        }
    }
    let after = memory();
    eprintln!(
        "{}: {} dictations in {:.0} s; memory {before:.0} MB -> {after:.0} MB",
        model.id,
        rounds * clips.len(),
        started.elapsed().as_secs_f64()
    );
    if before > 0.0 {
        assert!(
            after - before < 150.0,
            "{}: memory grew by {:.0} MB over {} dictations",
            model.id,
            after - before,
            rounds * clips.len()
        );
    }
}

#[test]
#[ignore = "needs speech models; see the top of core/src/stress_tests.rs"]
fn switching_models_back_and_forth_works_every_time_and_frees_the_old_one() {
    let models = test_models();
    let clip = &recordings("clips")[1];
    let app = app_with_model(models[0].id);
    let mut after_first_cycle = 0.0;
    for round in 0..6 {
        for model in &models {
            let mut s = app.settings.lock().unwrap().clone();
            s.model_id = Some(model.id.to_string());
            *app.settings.lock().unwrap() = s;
            let text = app
                .transcribe_samples(&clip.samples)
                .unwrap_or_else(|e| panic!("{}: {e}", model.id));
            let rate = error_rate([(clip.text.as_str(), text.as_str())]);
            assert!(
                rate <= 25.0,
                "round {round}, {}: {rate:.0}% word errors after a switch: {text}",
                model.id
            );
        }
        if round == 0 {
            after_first_cycle = memory();
        }
    }
    let end = memory();
    eprintln!(
        "{} switches; memory after the first cycle {after_first_cycle:.0} MB, at the end {end:.0} MB",
        6 * models.len()
    );
    if after_first_cycle > 0.0 {
        // The same model is loaded at both points, so memory should be about the same. If
        // each switch leaked a model, this would be gigabytes higher.
        assert!(
            end - after_first_cycle < 400.0,
            "memory grew by {:.0} MB across model switches",
            end - after_first_cycle
        );
    }
}

#[test]
#[ignore = "needs speech models; see the top of core/src/stress_tests.rs"]
fn a_ten_minute_recording_is_transcribed_whole_both_ways() {
    let (piece, piece_text) = long_recording();
    let mut samples = Vec::new();
    let mut reference = String::new();
    while samples.len() < 10 * 60 * SR {
        samples.extend_from_slice(&piece);
        reference.push_str(&piece_text);
    }
    let minutes = samples.len() as f64 / SR as f64 / 60.0;
    // The fastest model is enough: this is about the splitting, not the recognition.
    let model = test_models()
        .into_iter()
        .find(|m| m.id == "canary-180m-flash")
        .unwrap_or(test_models()[0]);
    let expected = words(&reference).len();

    let started = Instant::now();
    let whole = transcribe(model, &samples);
    let batch_seconds = started.elapsed().as_secs_f64();
    let said = words(&whole).len();
    let rate = error_rate([(reference.as_str(), whole.as_str())]);
    eprintln!(
        "{}: {minutes:.1} min recording in {batch_seconds:.0} s, {rate:.1}% word errors",
        model.id
    );
    assert!(said * 100 >= expected * 90, "only {said} of {expected} words came back");
    assert!(
        rate <= limits(model.id).clips + 5.0,
        "{rate:.1}% word errors on the ten-minute recording"
    );
    assert!(
        batch_seconds < minutes * 60.0,
        "slower than real time: {batch_seconds:.0} s for {minutes:.1} min"
    );

    let app = app_with_model(model.id);
    let pieces = app.transcribe_live(&samples, SR / 20).expect("live transcription");
    let live = pieces.concat();
    let live_rate = error_rate([(reference.as_str(), live.as_str())]);
    eprintln!(
        "{}: written live in {} pieces, {live_rate:.1}% word errors",
        model.id,
        pieces.len()
    );
    assert!(pieces.len() >= 60, "expected a piece per pause, got {}", pieces.len());
    assert!(
        live_rate <= limits(model.id).clips + 5.0,
        "{live_rate:.1}% word errors written live"
    );
}

#[test]
#[ignore = "needs speech models; see the top of core/src/stress_tests.rs"]
fn writing_as_you_speak_does_not_depend_on_how_the_audio_arrives() {
    let (samples, reference) = long_recording();
    let model = test_models()[0];
    let app = app_with_model(model.id);
    // Sound cards hand over audio in all sorts of sizes; the text must not depend on it.
    for chunk in [37, 160, 800, 4096, 50_000, samples.len()] {
        let pieces = app
            .transcribe_live(&samples, chunk)
            .unwrap_or_else(|e| panic!("chunks of {chunk}: {e}"));
        let rate = error_rate([(reference.as_str(), pieces.concat().as_str())]);
        assert!(pieces.len() >= 6, "chunks of {chunk}: only {} pieces", pieces.len());
        assert!(
            rate <= limits(model.id).clips + 4.0,
            "{}: {rate:.1}% word errors when audio arrives {chunk} samples at a time",
            model.id
        );
    }
}

#[test]
#[ignore = "needs speech models; see the top of core/src/stress_tests.rs"]
fn several_requests_at_once_each_get_the_right_text() {
    let clips = Arc::new(recordings("clips"));
    let model = test_models()[0];
    let app = app_with_model(model.id);
    let expected: Arc<Vec<String>> = Arc::new(
        clips
            .iter()
            .map(|c| app.transcribe_samples(&c.samples).unwrap())
            .collect(),
    );
    within(Duration::from_secs(600), "concurrent transcription", move || {
        let threads: Vec<_> = (0..6)
            .map(|t| {
                let (app, clips, expected) = (app.clone(), clips.clone(), expected.clone());
                std::thread::spawn(move || {
                    for i in 0..clips.len() {
                        let n = (i + t * 2) % clips.len();
                        let text = app.transcribe_samples(&clips[n].samples).unwrap();
                        assert_eq!(
                            text, expected[n],
                            "thread {t} got someone else's text for {}",
                            clips[n].name
                        );
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().expect("a transcription thread panicked");
        }
    });
}

/// Audio a healthy microphone never produces, but a broken driver, a virtual device or a
/// bug upstream might. The engine may return anything; it must return.
#[test]
#[ignore = "needs speech models; see the top of core/src/stress_tests.rs"]
fn audio_that_makes_no_sense_does_not_crash_the_engine() {
    let clip = recordings("clips").remove(0).samples;
    let noise = |n: usize, level: f32| -> Vec<f32> {
        (0..n)
            .map(|i| (((i as f32 * 12.9898).sin() * 43_758.547).fract()) * level)
            .collect()
    };
    let mut with_gaps = clip.clone();
    for (i, s) in with_gaps.iter_mut().enumerate() {
        if i % 97 == 0 {
            *s = f32::NAN;
        }
    }
    let cases: Vec<(&str, Vec<f32>)> = vec![
        ("nothing at all", vec![]),
        ("a single sample", vec![0.3]),
        ("a hundredth of a second", vec![0.1; 160]),
        ("all not-a-number", vec![f32::NAN; SR]),
        ("all infinite", vec![f32::INFINITY; SR]),
        ("negative infinity", vec![f32::NEG_INFINITY; SR]),
        ("speech with not-a-number scattered through it", with_gaps),
        ("a million times too loud", clip.iter().map(|s| s * 1e6).collect()),
        ("too quiet to hear", clip.iter().map(|s| s * 1e-9).collect()),
        ("stuck at full scale", vec![1.0; 2 * SR]),
        (
            "the fastest possible wobble",
            (0..2 * SR).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect(),
        ),
        ("loud static for a minute", noise(60 * SR, 1.0)),
        ("one sample under the split limit", noise(25 * SR - 1, 0.01)),
        ("exactly the split limit", noise(25 * SR, 0.01)),
        ("one sample over the split limit", noise(25 * SR + 1, 0.01)),
    ];
    for model in test_models() {
        for (what, samples) in &cases {
            eprintln!("{}: {what}", model.id);
            let text = transcribe(model, samples);
            // Whatever comes out is bounded: no model may write an essay about static.
            let seconds = (samples.len() / SR).max(1);
            assert!(
                words(&text).len() <= seconds * 6 + 12,
                "{}: {what} produced {} words: {:?}",
                model.id,
                words(&text).len(),
                text.chars().take(200).collect::<String>()
            );
        }
        // And it still works afterwards.
        let after = transcribe(model, &clip);
        assert!(
            words(&after).len() >= 5,
            "{}: broken after the hostile input: {after:?}",
            model.id
        );
    }
}
