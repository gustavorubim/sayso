//! Delivers text to whatever application has keyboard focus.

use std::thread::sleep;
use std::time::Duration;

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};

/// Puts text on the clipboard only long enough to paste it, asking the system not to record
/// it in clipboard history or sync it to other devices.
fn set_transient(clipboard: &mut arboard::Clipboard, text: &str) -> Result<(), arboard::Error> {
    #[cfg(windows)]
    {
        use arboard::SetExtWindows;
        clipboard
            .set()
            .exclude_from_history()
            .exclude_from_cloud()
            .exclude_from_monitoring()
            .text(text.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        use arboard::SetExtApple;
        clipboard.set().exclude_from_history().text(text.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use arboard::SetExtLinux;
        clipboard.set().exclude_from_history().text(text.to_string())
    }
}

pub fn copy(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Could not copy: {e}"))
}

fn enigo() -> Result<Enigo, String> {
    Enigo::new(&EnigoSettings::default()).map_err(|e| {
        if cfg!(target_os = "macos") {
            format!("Allow Sayso under System Settings > Privacy & Security > Accessibility. ({e})")
        } else {
            format!("Could not send keystrokes: {e}")
        }
    })
}

fn press_paste(enigo: &mut Enigo) -> Result<(), String> {
    // macOS: use the physical V key code so no keyboard-layout lookup is needed off the main thread.
    #[cfg(target_os = "macos")]
    let (modifier, v) = (Key::Meta, Key::Other(9));
    #[cfg(not(target_os = "macos"))]
    let (modifier, v) = (Key::Control, Key::Unicode('v'));

    let e = |err: enigo::InputError| format!("Could not paste: {err}");
    enigo.key(modifier, Direction::Press).map_err(e)?;
    let result = enigo.key(v, Direction::Click).map_err(e);
    enigo.key(modifier, Direction::Release).map_err(e)?;
    result
}

/// `output` is `paste`, `type` or `copy` (see `Settings::output`).
pub fn deliver(text: &str, output: &str, restore_clipboard: bool) -> Result<(), String> {
    match output {
        "copy" => copy(text),
        "type" => enigo()?.text(text).map_err(|e| format!("Could not type the text: {e}")),
        _ => {
            let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;
            let previous = clipboard.get_text().ok();
            set_transient(&mut clipboard, text).map_err(|e| format!("Could not copy: {e}"))?;
            sleep(Duration::from_millis(70));

            let mut enigo = enigo()?;
            press_paste(&mut enigo)?;

            if restore_clipboard {
                // Give the target application time to read the clipboard first.
                sleep(Duration::from_millis(250));
                match previous {
                    Some(previous) => {
                        let _ = clipboard.set_text(previous);
                    }
                    // What was there before was not text (an image, files) and cannot be put
                    // back. Don't leave the dictation behind in its place.
                    None => {
                        let _ = clipboard.clear();
                    }
                }
            }
            Ok(())
        }
    }
}
