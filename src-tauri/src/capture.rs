//! Grabs the user's current text selection.
//!
//! Every desktop OS lacks a portable "get selected text" API, so we do what
//! text expanders do: remember the clipboard, press Copy, read the result,
//! and put the old clipboard back.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::{thread::sleep, time::Duration};

#[cfg(target_os = "macos")]
const COPY_MODIFIER: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const COPY_MODIFIER: Key = Key::Control;

// On Windows a Unicode key is typed as a character event, which does not combine with Ctrl;
// the virtual key for C does, on every keyboard layout.
#[cfg(target_os = "windows")]
const COPY_KEY: Key = Key::C;
#[cfg(not(target_os = "windows"))]
const COPY_KEY: Key = Key::Unicode('c');

pub fn selected_text() -> Result<String, String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("無法讀取剪貼簿：{e}"))?;
    let previous = clipboard.get_text().ok();

    // A sentinel lets us tell "nothing selected" apart from "selection equals the old clipboard".
    let sentinel = format!("\u{0}contextlens-{}", std::process::id());
    clipboard.set_text(sentinel.clone()).map_err(|e| e.to_string())?;

    send_copy()?;

    let mut grabbed = None;
    for _ in 0..20 {
        sleep(Duration::from_millis(25));
        if let Ok(now) = clipboard.get_text() {
            if now != sentinel {
                grabbed = Some(now);
                break;
            }
        }
    }

    match previous {
        Some(old) => {
            let _ = clipboard.set_text(old);
        }
        None => {
            let _ = clipboard.clear();
        }
    }

    match grabbed {
        Some(t) if !t.trim().is_empty() => Ok(t),
        _ => Err("沒有偵測到選取的文字。先反白一段文字再按熱鍵。".into()),
    }
}

fn send_copy() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("無法模擬按鍵：{e}"))?;
    // The hotkey's own modifiers may still be down; release the ones Copy must not inherit.
    for key in [Key::Shift, Key::Alt] {
        let _ = enigo.key(key, Direction::Release);
    }
    let err = |e: enigo::InputError| format!("無法模擬複製：{e}");
    enigo.key(COPY_MODIFIER, Direction::Press).map_err(err)?;
    let click = enigo.key(COPY_KEY, Direction::Click);
    let release = enigo.key(COPY_MODIFIER, Direction::Release);
    click.map_err(err)?;
    release.map_err(err)
}
