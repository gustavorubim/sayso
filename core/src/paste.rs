//! Delivers text to whatever application has keyboard focus.

use std::thread::sleep;
use std::time::Duration;

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};

pub fn copy(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Could not copy: {e}"))
}

fn enigo() -> Result<Enigo, String> {
    Enigo::new(&EnigoSettings::default()).map_err(|e| {
        if cfg!(target_os = "macos") {
            format!("Allow Shortcut to Text under System Settings > Privacy & Security > Accessibility. ({e})")
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
            clipboard
                .set_text(text.to_string())
                .map_err(|e| format!("Could not copy: {e}"))?;
            sleep(Duration::from_millis(70));

            let mut enigo = enigo()?;
            press_paste(&mut enigo)?;

            if restore_clipboard {
                if let Some(previous) = previous {
                    // Give the target application time to read the clipboard first.
                    sleep(Duration::from_millis(250));
                    let _ = clipboard.set_text(previous);
                }
            }
            Ok(())
        }
    }
}
