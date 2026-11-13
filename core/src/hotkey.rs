//! Global hotkey with press *and* release events (needed for hold-to-talk).
//!
//! Hotkeys must be registered and pumped on one thread — the main thread on
//! macOS — so this module owns the main thread for the life of the process.

use std::str::FromStr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::app::App;
use crate::rpc;

pub enum HotkeyCmd {
    /// Replace the registered shortcut; the result is sent back.
    Set(String, Sender<Result<(), String>>),
}

pub fn parse(shortcut: &str) -> Result<HotKey, String> {
    HotKey::from_str(shortcut).map_err(|e| format!("\"{shortcut}\" is not a usable shortcut: {e}"))
}

struct Registered {
    manager: Option<GlobalHotKeyManager>,
    current: Option<HotKey>,
}

impl Registered {
    fn set(&mut self, shortcut: &str) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or("Global shortcuts are not available in this session (on Linux, Wayland is not supported yet).")?;
        let hotkey = parse(shortcut)?;
        if self.current.map(|h| h.id()) == Some(hotkey.id()) {
            return Ok(());
        }
        manager
            .register(hotkey)
            .map_err(|e| format!("Could not register {shortcut}. Another app may be using it. ({e})"))?;
        if let Some(old) = self.current.replace(hotkey) {
            let _ = manager.unregister(old);
        }
        Ok(())
    }
}

pub fn run(app: Arc<App>, commands: Receiver<HotkeyCmd>) -> ! {
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("global hotkeys unavailable: {e}");
            None
        }
    };
    let mut registered = Registered { manager, current: None };

    let initial = app.settings.lock().unwrap().shortcut.clone();
    if let Err(e) = registered.set(&initial) {
        rpc::emit("hotkey-error", e);
    }

    let events = GlobalHotKeyEvent::receiver();
    loop {
        pump();

        while let Ok(event) = events.try_recv() {
            if registered.current.map(|h| h.id()) == Some(event.id) {
                app.on_hotkey(event.state == HotKeyState::Pressed);
            }
        }

        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                HotkeyCmd::Set(shortcut, reply) => {
                    let _ = reply.send(registered.set(&shortcut));
                }
            }
        }
    }
}

/// Lets the OS deliver hotkey messages, then yields for a few milliseconds.
#[cfg(windows)]
fn pump() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(8));
}

#[cfg(target_os = "macos")]
fn pump() {
    use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
    CFRunLoop::run_in_mode(
        unsafe { kCFRunLoopDefaultMode },
        std::time::Duration::from_millis(10),
        false,
    );
}

#[cfg(not(any(windows, target_os = "macos")))]
fn pump() {
    // X11: global-hotkey listens on its own thread.
    std::thread::sleep(std::time::Duration::from_millis(8));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts_built_by_the_ui() {
        for s in [
            "Ctrl+Shift+Space",
            "Alt+Space",
            "Ctrl+Alt+KeyD",
            "Super+Shift+F9",
            "Ctrl+Digit1",
            "F8",
        ] {
            assert!(parse(s).is_ok(), "{s} should parse");
        }
        assert!(parse("Ctrl+Shift").is_err());
        assert!(parse("").is_err());
    }
}
