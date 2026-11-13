//! Sayso — native core.
//!
//! Runs as a child process of the desktop shell and talks JSON lines over
//! stdin/stdout (see `rpc.rs`). The main thread owns the global hotkey and the
//! OS event pump; everything else runs on worker threads.

mod app;
mod audio;
mod bench;
mod download;
mod engine;
mod history;
mod hotkey;
mod models;
mod paste;
mod perf;
mod rewrite;
mod rpc;
mod settings;
mod textproc;

use std::path::PathBuf;
use std::sync::{mpsc, Arc};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("bench") {
        std::process::exit(bench::run());
    }

    // If a worker thread panics, state may be half-updated and locks poisoned. Exit so the
    // shell starts a fresh core instead of talking to a broken one. (Request handlers catch
    // their own panics first; see rpc.rs.)
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        if std::thread::current().name() != Some("rpc") {
            std::process::exit(101);
        }
    }));

    let data_dir = data_dir_from_args();
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("cannot create data dir {}: {e}", data_dir.display());
        std::process::exit(2);
    }

    let (hk_tx, hk_rx) = mpsc::channel();
    let app = Arc::new(app::App::new(data_dir, hk_tx));

    {
        let app = app.clone();
        std::thread::Builder::new()
            .name("rpc".into())
            .spawn(move || {
                rpc::serve(app);
                // stdin closed: the shell is gone, so are we.
                std::process::exit(0);
            })
            .expect("could not start the request thread");
    }

    app.warm_engine();
    hotkey::run(app, hk_rx);
}

fn data_dir_from_args() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--data-dir" {
            if let Some(p) = args.next() {
                return PathBuf::from(p);
            }
        }
    }
    std::env::temp_dir().join("sayso")
}
