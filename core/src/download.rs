//! Downloads model files from Hugging Face with progress, cancel and resume.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::app::App;
use crate::models::{self, ModelDef};
use crate::rpc;

#[derive(Serialize, Clone)]
struct Progress<'a> {
    id: &'a str,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
}

pub(crate) const CANCELLED: &str = "cancelled";

pub fn start(app: Arc<App>, id: &str) -> Result<(), String> {
    let model = models::find(id).ok_or_else(|| format!("Unknown model: {id}"))?;
    if model.is_installed(&app.models_dir()) {
        return Ok(());
    }
    let cancel = app.begin_download(id)?;

    std::thread::spawn(move || {
        let total = model.size_bytes() + models::VAD_SIZE;
        let result = download_model(&app, model, total, &cancel);
        app.end_download(model.id);

        let error = match result {
            Ok(()) => None,
            Err(e) if e == CANCELLED => Some("Download cancelled".to_string()),
            Err(e) => Some(e),
        };
        let ok = error.is_none();
        rpc::emit(
            "download-progress",
            Progress {
                id: model.id,
                downloaded: if ok { total } else { 0 },
                total,
                done: true,
                error,
            },
        );
        if ok {
            app.on_model_installed(model.id);
        }
    });
    Ok(())
}

/// `sayso-core fetch --data-dir <dir> <model id>...`: downloads models from the command
/// line, with the same pinned revisions and checksums as the app. Used to prepare the audio
/// regression tests; returns the process exit code.
pub fn fetch_command() -> i32 {
    let mut data_dir = None;
    let mut ids = Vec::new();
    let mut args = std::env::args().skip(2);
    while let Some(a) = args.next() {
        if a == "--data-dir" {
            data_dir = args.next().map(std::path::PathBuf::from);
        } else {
            ids.push(a);
        }
    }
    let Some(data_dir) = data_dir else {
        eprintln!("usage: sayso-core fetch --data-dir <dir> <model id>...");
        return 2;
    };
    if ids.is_empty() {
        eprintln!(
            "name at least one model: {}",
            models::CATALOG.iter().map(|m| m.id).collect::<Vec<_>>().join(", ")
        );
        return 2;
    }
    rpc::set_quiet(true);
    let (tx, _rx) = std::sync::mpsc::channel();
    let app = App::new(data_dir, tx);
    for id in &ids {
        let Some(model) = models::find(id) else {
            eprintln!("unknown model: {id}");
            return 2;
        };
        if model.is_installed(&app.models_dir()) && models::vad_path(&app.models_dir()).is_file() {
            println!("{id}: already downloaded");
            continue;
        }
        println!("{id}: downloading {} MB", model.size_bytes() / 1_000_000);
        let total = model.size_bytes() + models::VAD_SIZE;
        if let Err(e) = download_model(&app, model, total, &AtomicBool::new(false)) {
            eprintln!("{id}: {e}");
            return 1;
        }
        println!("{id}: done, checksums verified");
    }
    0
}

fn download_model(app: &App, model: &ModelDef, total: u64, cancel: &AtomicBool) -> Result<(), String> {
    let dir = model.dir(&app.models_dir());
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create the model folder: {e}"))?;

    let mut done_before = 0u64;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut report = |downloaded: u64| {
        if last_emit.elapsed() >= Duration::from_millis(120) {
            last_emit = Instant::now();
            rpc::emit(
                "download-progress",
                Progress {
                    id: model.id,
                    downloaded,
                    total,
                    done: false,
                    error: None,
                },
            );
        }
    };

    for (name, size, sha256) in model.files {
        let dest = dir.join(name);
        if !dest.is_file() {
            // Some models keep files in a subfolder (a tokenizer, for example).
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Could not create the model folder: {e}"))?;
            }
            let base = done_before;
            let expected = Expected { size: *size, sha256 };
            fetch(&model.file_url(name), &dest, &expected, cancel, &mut |n| {
                report(base + n)
            })?;
        }
        done_before += size;
    }

    // The voice-activity model is shared by all models and only needed for long recordings,
    // so a failure here is not fatal.
    let vad = models::vad_path(&app.models_dir());
    if !vad.is_file() {
        if let Some(parent) = vad.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let url = format!(
            "{}/{}/resolve/{}/{}",
            models::HF_BASE,
            models::VAD_REPO,
            models::VAD_REVISION,
            models::VAD_FILE
        );
        let base = done_before;
        let expected = Expected {
            size: models::VAD_SIZE,
            sha256: models::VAD_SHA256,
        };
        if let Err(e) = fetch(&url, &vad, &expected, cancel, &mut |n| report(base + n)) {
            if e == CANCELLED {
                return Err(e);
            }
            eprintln!("could not download the voice-activity model: {e}");
        }
    }
    Ok(())
}

/// What a downloaded file must be: its exact size and SHA-256.
pub(crate) struct Expected<'a> {
    pub size: u64,
    pub sha256: &'a str,
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads `url` to `dest` through a `.part` file, resuming if one exists. The finished
/// file is only kept if it matches `expected`.
fn fetch(
    url: &str,
    dest: &Path,
    expected: &Expected,
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), String> {
    fetch_with(&https_agent(), url, dest, expected, cancel, on_bytes)
}

/// The client for real downloads: HTTPS only.
fn https_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .user_agent(concat!("sayso/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .build()
}

pub(crate) fn fetch_with(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    expected: &Expected,
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), String> {
    let part = dest.with_extension(match dest.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{ext}.part"),
        None => "part".to_string(),
    });
    let mut existing = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if existing >= expected.size {
        // A leftover that is already as long as the whole file cannot be resumed.
        let _ = fs::remove_file(&part);
        existing = 0;
    }

    let mut request = agent.get(url);
    if existing > 0 {
        request = request.set("Range", &format!("bytes={existing}-"));
    }
    let response = match request.call() {
        Ok(r) => r,
        // The .part file is already complete or stale: start over.
        Err(ureq::Error::Status(416, _)) => {
            let _ = fs::remove_file(&part);
            agent.get(url).call().map_err(describe)?
        }
        Err(e) => return Err(describe(e)),
    };

    let resumed = response.status() == 206;
    let mut file: File = if resumed {
        OpenOptions::new().append(true).open(&part)
    } else {
        File::create(&part)
    }
    .map_err(|e| format!("Could not write to disk: {e}"))?;

    // The checksum covers the whole file, so a resumed download starts by hashing what is
    // already on disk.
    let mut hasher = Sha256::new();
    if resumed {
        let mut earlier = File::open(&part).map_err(|e| format!("Could not read the partial download: {e}"))?;
        std::io::copy(&mut earlier, &mut hasher).map_err(|e| format!("Could not read the partial download: {e}"))?;
    }
    let mut downloaded = if resumed { existing } else { 0 };
    let mut reader = response.into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(CANCELLED.to_string());
        }
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("The download was interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("Could not write to disk: {e}"))?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if downloaded > expected.size {
            drop(file);
            let _ = fs::remove_file(&part);
            return Err("The download is larger than it should be, so it was discarded.".into());
        }
        on_bytes(downloaded);
    }
    file.flush().map_err(|e| format!("Could not write to disk: {e}"))?;
    drop(file);
    if downloaded != expected.size || hex(&hasher.finalize()) != expected.sha256 {
        let _ = fs::remove_file(&part);
        return Err("The downloaded file did not match its checksum, so it was discarded. Try again.".into());
    }
    fs::rename(&part, dest).map_err(|e| format!("Could not finish the download: {e}"))
}

fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("Hugging Face answered with HTTP {code}."),
        ureq::Error::Transport(t) => format!("Could not reach Hugging Face. Check your connection. ({t})"),
    }
}
