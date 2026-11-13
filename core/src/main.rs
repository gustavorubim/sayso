//! Sayso — native core.
//!
//! Runs as a child process of the desktop shell and talks JSON lines over
//! stdin/stdout (see `rpc.rs`). The main thread owns the global hotkey and the
//! OS event pump; everything else runs on worker threads.

mod app;
mod audio;
mod download;
mod engine;
mod history;
mod hotkey;
mod models;
mod paste;
mod rewrite;
mod rpc;
mod settings;
mod textproc;

use std::path::PathBuf;
use std::sync::{mpsc, Arc};

fn main() {
    let data_dir = data_dir_from_args();
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("cannot create data dir {}: {e}", data_dir.display());
        std::process::exit(2);
    }

    let (hk_tx, hk_rx) = mpsc::channel();
    let app = Arc::new(app::App::new(data_dir, hk_tx));

    {
        let app = app.clone();
        std::thread::spawn(move || {
            rpc::serve(app);
            // stdin closed: the shell is gone, so are we.
            std::process::exit(0);
        });
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
