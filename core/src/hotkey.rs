//! Global hotkeys with press *and* release events (needed for hold-to-talk).
//!
//! Hotkeys must be registered and pumped on one thread — the main thread on
//! macOS — so this module owns the main thread for the life of the process.
//!
//! There are three slots: 0 is plain dictation, 1 is dictate-and-rewrite, 2 is computer audio.

use std::str::FromStr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::app::App;
use crate::rpc;

pub const SLOTS: usize = 3;

pub enum HotkeyCmd {
    /// Register `shortcut` in a slot, or clear the slot with `None`. The result is sent back.
    Set(usize, Option<String>, Sender<Result<(), String>>),
}

pub fn parse(shortcut: &str) -> Result<HotKey, String> {
    HotKey::from_str(shortcut).map_err(|e| format!("\"{shortcut}\" is not a usable shortcut: {e}"))
}

struct Registered {
    manager: Option<GlobalHotKeyManager>,
    slots: [Option<HotKey>; SLOTS],
}

impl Registered {
    fn set(&mut self, slot: usize, shortcut: Option<&str>) -> Result<(), String> {
        let manager = self
            .manager
            .as_ref()
            .ok_or("Global shortcuts are not available in this session (on Linux, Wayland is not supported yet).")?;
        let wanted = shortcut.map(parse).transpose()?;
        if self.slots[slot].map(|h| h.id()) == wanted.map(|h| h.id()) {
            return Ok(());
        }
        if let Some(hotkey) = wanted {
            manager.register(hotkey).map_err(|e| {
                format!(
                    "Could not register {}. Another app may be using it. ({e})",
                    shortcut.unwrap_or_default()
                )
            })?;
        }
        if let Some(old) = std::mem::replace(&mut self.slots[slot], wanted) {
            let _ = manager.unregister(old);
        }
        Ok(())
    }

    fn slot_of(&self, id: u32) -> Option<usize> {
        self.slots.iter().position(|h| h.map(|h| h.id()) == Some(id))
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
    let mut registered = Registered {
        manager,
        slots: [None; SLOTS],
    };

    for (slot, shortcut) in app.settings.lock().unwrap().clone().shortcuts().into_iter().enumerate() {
        if let Some(shortcut) = shortcut {
            if let Err(e) = registered.set(slot, Some(&shortcut)) {
                rpc::emit("hotkey-error", e);
            }
        }
    }

    let events = GlobalHotKeyEvent::receiver();
    loop {
        pump();

        while let Ok(event) = events.try_recv() {
            if let Some(slot) = registered.slot_of(event.id) {
                app.on_hotkey(slot, event.state == HotKeyState::Pressed);
            }
        }

        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                HotkeyCmd::Set(slot, shortcut, reply) => {
                    let _ = reply.send(registered.set(slot, shortcut.as_deref()));
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
            "Ctrl+Alt+Space",
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

    #[test]
    fn different_shortcuts_have_different_ids() {
        assert_ne!(
            parse("Ctrl+Shift+Space").unwrap().id(),
            parse("Ctrl+Alt+Space").unwrap().id()
        );
        assert_eq!(
            parse("Ctrl+Shift+Space").unwrap().id(),
            parse("Shift+Ctrl+Space").unwrap().id()
        );
    }
}
