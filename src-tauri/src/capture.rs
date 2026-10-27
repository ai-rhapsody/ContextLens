//! Grabs the user's current text selection.
//!
//! Every desktop OS lacks a portable "get selected text" API, so we do what
//! text expanders do: remember the clipboard, press Copy, read the result,
//! and put the old clipboard back.

use crate::imaging;
use arboard::Clipboard;
use contextlens_core::Media;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

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

/// Placed on the clipboard before pressing Copy, so "nothing selected" can be told apart from
/// "the selection equals the old clipboard". Plain printable text only: Windows clipboard text
/// ends at the first NUL, so a sentinel starting with one would read back as an empty string.
fn sentinel() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!("⟦contextlens:{}:{nanos}⟧", std::process::id())
}

/// How long to wait for the app to put its selection on the clipboard. Office, browsers and
/// Electron apps can take a few hundred milliseconds.
const COPY_WAIT: Duration = Duration::from_millis(1000);

#[cfg(target_os = "windows")]
const NOTHING_SELECTED: &str = "沒有偵測到選取的文字。先反白一段文字再按熱鍵。\
若已經選取：目標程式以系統管理員身分執行時，Windows 不允許其他程式對它按 Ctrl+C，請以相同權限執行 ContextLens；\
終端機等不支援 Ctrl+C 複製的程式也抓不到，可以改用「截圖」熱鍵。";
#[cfg(target_os = "macos")]
const NOTHING_SELECTED: &str = "沒有偵測到選取的文字。先反白一段文字再按熱鍵。\
若已經選取：確認「系統設定 → 隱私權與安全性 → 輔助使用」裡 ContextLens 是開啟的；\
不支援 ⌘C 複製的程式抓不到，可以改用「截圖」熱鍵。";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const NOTHING_SELECTED: &str = "沒有偵測到選取的文字。先反白一段文字再按熱鍵。\
Wayland 下有些桌面不允許其他程式讀取選取的文字或模擬按鍵，可以改用「截圖」熱鍵。";

pub fn selected_text() -> Result<String, String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("無法讀取剪貼簿：{e}"))?;
    // On Linux, highlighting text already puts it in the PRIMARY selection: no key press needed.
    #[cfg(target_os = "linux")]
    if let Some(t) = primary_selection(&mut clipboard) {
        return Ok(t);
    }
    #[cfg(target_os = "macos")]
    mac::ensure_accessibility()?;
    let previous = clipboard.get_text().ok();

    let mark = sentinel();
    clipboard.set_text(mark.clone()).map_err(|e| format!("無法寫入剪貼簿：{e}"))?;

    wait_for_modifiers_released();
    send_copy()?;

    // Another program (a clipboard manager) may hold the clipboard for a moment, so read
    // errors are retried until the deadline rather than treated as "nothing selected".
    let deadline = Instant::now() + COPY_WAIT;
    let mut grabbed = None;
    while Instant::now() < deadline {
        sleep(Duration::from_millis(25));
        if let Ok(now) = clipboard.get_text() {
            if now != mark {
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
        _ => Err(NOTHING_SELECTED.into()),
    }
}

/// If Ctrl+C is sent while the user still holds the hotkey (say Ctrl+Shift+T), the app sees
/// Ctrl+Shift+C, which is not Copy (in a browser it opens the inspector). Wait for the release.
#[cfg(target_os = "windows")]
fn wait_for_modifiers_released() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    let held = || {
        [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            .any(|&vk| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0)
    };
    let deadline = Instant::now() + Duration::from_millis(600);
    while held() && Instant::now() < deadline {
        sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "macos")]
fn wait_for_modifiers_released() {
    let deadline = Instant::now() + Duration::from_millis(600);
    while mac::modifiers_held() && Instant::now() < deadline {
        sleep(Duration::from_millis(10));
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn wait_for_modifiers_released() {}

#[cfg(target_os = "linux")]
fn primary_selection(clipboard: &mut Clipboard) -> Option<String> {
    use arboard::{GetExtLinux, LinuxClipboardKind};
    let t = clipboard.get().clipboard(LinuxClipboardKind::Primary).text().ok()?;
    (!t.trim().is_empty()).then_some(t)
}

#[cfg(target_os = "macos")]
pub mod mac {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceFlagsState(state_id: i32) -> u64;
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    fn open_privacy_pane(pane: &str) {
        let url = format!("x-apple.systempreferences:com.apple.preference.security?{pane}");
        let _ = std::process::Command::new("open").arg(url).spawn();
    }

    /// Pressing ⌘C in another app needs the Accessibility permission. Without it the key
    /// press is silently dropped, so ask up front and open the right settings pane.
    pub fn ensure_accessibility() -> Result<(), String> {
        if unsafe { AXIsProcessTrusted() } {
            return Ok(());
        }
        open_privacy_pane("Privacy_Accessibility");
        Err("ContextLens 需要「輔助使用」權限才能讀取選取的文字。\
在剛打開的「系統設定 → 隱私權與安全性 → 輔助使用」裡開啟 ContextLens，再按一次熱鍵。"
            .into())
    }

    /// Screenshots of other apps need the Screen Recording permission; without it macOS
    /// returns only the wallpaper.
    pub fn ensure_screen_capture() -> Result<(), String> {
        if unsafe { CGPreflightScreenCaptureAccess() } {
            return Ok(());
        }
        // Shows the system prompt the first time; later it only adds ContextLens to the list.
        unsafe { CGRequestScreenCaptureAccess() };
        open_privacy_pane("Privacy_ScreenCapture");
        Err("ContextLens 需要「螢幕錄製」權限才能截圖。\
在「系統設定 → 隱私權與安全性 → 螢幕與系統錄音」裡開啟 ContextLens，然後結束並重新開啟 ContextLens。"
            .into())
    }

    /// Shift, Control, Option or Command still down (the hotkey's own keys).
    pub fn modifiers_held() -> bool {
        const MASK: u64 = 0x20000 | 0x40000 | 0x80000 | 0x100000;
        // kCGEventSourceStateCombinedSessionState
        unsafe { CGEventSourceFlagsState(0) & MASK != 0 }
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

#[cfg(target_os = "windows")]
const SCREENSHOT_KEYS: &str = "Win+Shift+S";
#[cfg(target_os = "macos")]
const SCREENSHOT_KEYS: &str = "⌘⌃⇧4 會把截圖放進剪貼簿";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const SCREENSHOT_KEYS: &str = "例如 Shift+PrtSc";

/// The image on the clipboard, e.g. after Win+Shift+S or a screenshot tool.
pub fn clipboard_image() -> Result<Media, String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("無法讀取剪貼簿：{e}"))?;
    let img = clipboard
        .get_image()
        .map_err(|_| format!("剪貼簿裡沒有圖片。先截圖（{SCREENSHOT_KEYS}）再按熱鍵。"))?;
    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
        .ok_or("剪貼簿的圖片格式無法讀取")?;
    imaging::png_media(&rgba, "剪貼簿圖片.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_is_plain_text_that_survives_the_windows_clipboard() {
        let s = sentinel();
        assert!(!s.contains('\0'));
        assert!(!s.trim().is_empty());
        assert_ne!(s, sentinel_after_a_moment());
    }

    fn sentinel_after_a_moment() -> String {
        sleep(Duration::from_millis(2));
        sentinel()
    }
}
