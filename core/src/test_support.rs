//! Shared helpers for unit tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use crate::app::App;
use crate::hotkey::HotkeyCmd;

/// A fresh, empty folder under the system temp directory.
pub fn temp_dir(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "sayso-test-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// An `App` on an empty data folder. Shortcut registrations are accepted without touching
/// the real keyboard, and events are not written to stdout.
pub fn app() -> (Arc<App>, PathBuf) {
    app_with_hotkeys(|_, _| Ok(()))
}

/// Like `app`, but `register` decides the outcome of every shortcut registration. It gets
/// the slot and the shortcut (`None` clears the slot).
pub fn app_with_hotkeys(
    register: impl Fn(usize, Option<&str>) -> Result<(), String> + Send + 'static,
) -> (Arc<App>, PathBuf) {
    crate::rpc::set_quiet(true);
    let dir = temp_dir("app");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(HotkeyCmd::Set(slot, shortcut, reply)) = rx.recv() {
            let _ = reply.send(register(slot, shortcut.as_deref()));
        }
    });
    (Arc::new(App::new(dir.clone(), tx)), dir)
}
