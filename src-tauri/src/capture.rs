//! Grabs the user's current text selection.
//!
//! Every desktop OS lacks a portable "get selected text" API, so we do what
//! text expanders do: remember the clipboard, press Copy, read the result,
//! and put the old clipboard back.

use crate::imaging;
use arboard::Clipboard;
use contextlens_core::{i18n::tr, n_, t, Media};
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
const NOTHING_SELECTED: &str = n_!("沒有偵測到選取的文字。先反白一段文字再按熱鍵。\
若已經選取：目標程式以系統管理員身分執行時，Windows 不允許其他程式對它按 Ctrl+C，請以相同權限執行 ContextLens；\
終端機等不支援 Ctrl+C 複製的程式也抓不到，可以改用「截圖」熱鍵。");
#[cfg(target_os = "macos")]
const NOTHING_SELECTED: &str = n_!("沒有偵測到選取的文字。先反白一段文字再按熱鍵。\
若已經選取：確認「系統設定 → 隱私權與安全性 → 輔助使用」裡 ContextLens 是開啟的；\
不支援 ⌘C 複製的程式抓不到，可以改用「截圖」熱鍵。");
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const NOTHING_SELECTED: &str = n_!("沒有偵測到選取的文字。先反白一段文字再按熱鍵。\
Wayland 下有些桌面不允許其他程式讀取選取的文字或模擬按鍵，可以改用「截圖」熱鍵。");

pub fn selected_text() -> Result<String, String> {
    let mut clipboard = Clipboard::new().map_err(|e| t!("無法讀取剪貼簿：{e}", e = e))?;
    // On Linux, highlighting text already puts it in the PRIMARY selection: no key press needed.
    #[cfg(target_os = "linux")]
    if let Some(t) = primary_selection(&mut clipboard) {
        return Ok(t);
    }
    #[cfg(target_os = "macos")]
    mac::ensure_accessibility()?;
    let previous = Saved::take(&mut clipboard);

    let mark = sentinel();
    clipboard.set_text(mark.clone()).map_err(|e| t!("無法寫入剪貼簿：{e}", e = e))?;

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

    previous.restore(&mut clipboard);

    match grabbed {
        Some(t) if !t.trim().is_empty() => Ok(t),
        _ => Err(tr(NOTHING_SELECTED)),
    }
}

/// Where the selection is on screen, in physical pixels: the top of its first line and the
/// bottom of its last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub top: f64,
    pub bottom: f64,
}

/// How long the popup may wait to learn where the selection is.
#[cfg(target_os = "windows")]
const SPAN_WAIT: Duration = Duration::from_millis(500);

/// Starts finding where the selection is, while Copy runs; the returned call collects the
/// answer. Lets the popup open beneath the whole selection rather than under the cursor,
/// which a triple-click leaves in the middle of the paragraph. `None` when the app does not say.
pub fn begin_selection_span() -> impl FnOnce() -> Option<Span> {
    #[cfg(target_os = "windows")]
    {
        // UI Automation can stall on a busy app; the cursor is a fine fallback.
        let deadline = Instant::now() + SPAN_WAIT;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(uia::selection_span(deadline));
        });
        move || rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).ok().flatten()
    }
    #[cfg(not(target_os = "windows"))]
    {
        || None
    }
}

/// The span covering every line rectangle ([left, top, width, height]) that has a size.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn span_of(lines: &[[f64; 4]]) -> Option<Span> {
    lines.iter().filter(|r| r[2] > 0.0 && r[3] > 0.0).fold(None, |s, r| {
        let (top, bottom) = (r[1], r[1] + r[3]);
        Some(s.map_or(Span { top, bottom }, |s: Span| Span { top: s.top.min(top), bottom: s.bottom.max(bottom) }))
    })
}

#[cfg(target_os = "windows")]
mod uia {
    use super::Span;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, SAFEARRAY,
    };
    use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetUBound, SafeArrayUnaccessData};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, UIA_TextPatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    pub fn selection_span(deadline: Instant) -> Option<Span> {
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let span = poll(deadline);
            if initialised {
                CoUninitialize();
            }
            span
        }
    }

    /// Chrome builds its accessibility tree only once a client asks, so the first answers
    /// can be empty: ask again until the deadline.
    unsafe fn poll(deadline: Instant) -> Option<Span> {
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        loop {
            if let Some(span) = find(&uia) {
                return Some(span);
            }
            if Instant::now() + Duration::from_millis(60) >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(60));
        }
    }

    /// The selection in the focused control, else in the one under the cursor.
    unsafe fn find(uia: &IUIAutomation) -> Option<Span> {
        let focused = uia.GetFocusedElement().ok();
        let mut at = POINT::default();
        let under = GetCursorPos(&mut at).ok().and_then(|_| uia.ElementFromPoint(at).ok());
        [focused, under].into_iter().flatten().find_map(|e| selection_in(uia, e))
    }

    /// The selection of the nearest element, from `e` up, that holds text (a web page's
    /// document, a Word page, an edit box).
    unsafe fn selection_in(uia: &IUIAutomation, mut e: IUIAutomationElement) -> Option<Span> {
        let walker = uia.RawViewWalker().ok()?;
        for _ in 0..12 {
            if let Ok(text) = e.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                return selection(&text);
            }
            e = walker.GetParentElement(&e).ok()?;
        }
        None
    }

    unsafe fn selection(text: &IUIAutomationTextPattern) -> Option<Span> {
        let ranges = text.GetSelection().ok()?;
        let mut lines = Vec::new();
        for i in 0..ranges.Length().ok()? {
            lines.extend(rectangles(ranges.GetElement(i).ok()?.GetBoundingRectangles().ok()?));
        }
        super::span_of(&lines)
    }

    /// Reads and frees the [left, top, width, height, …] array UI Automation hands back.
    unsafe fn rectangles(array: *mut SAFEARRAY) -> Vec<[f64; 4]> {
        if array.is_null() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut data: *mut core::ffi::c_void = core::ptr::null_mut();
        if let (Ok(upper), Ok(())) = (SafeArrayGetUBound(array, 1), SafeArrayAccessData(array, &mut data)) {
            let values = core::slice::from_raw_parts(data as *const f64, (upper + 1).max(0) as usize);
            out = values.chunks_exact(4).map(|r| [r[0], r[1], r[2], r[3]]).collect();
            let _ = SafeArrayUnaccessData(array);
        }
        let _ = SafeArrayDestroy(array);
        out
    }
}

/// What was on the clipboard before Copy, put back afterwards so a screenshot or copied files
/// survive the hotkey. The richest form is kept: files, an image, formatted text, plain text.
enum Saved {
    Files(Vec<std::path::PathBuf>),
    Image(arboard::ImageData<'static>),
    Html { html: String, text: Option<String> },
    Text(String),
    Nothing,
}

impl Saved {
    fn take(clipboard: &mut Clipboard) -> Saved {
        if let Ok(files) = clipboard.get().file_list() {
            if !files.is_empty() {
                return Saved::Files(files);
            }
        }
        if let Ok(image) = clipboard.get_image() {
            return Saved::Image(image);
        }
        let text = clipboard.get_text().ok();
        if let Ok(html) = clipboard.get().html() {
            if !html.trim().is_empty() {
                return Saved::Html { html, text };
            }
        }
        text.map_or(Saved::Nothing, Saved::Text)
    }

    fn restore(self, clipboard: &mut Clipboard) {
        let put = match self {
            Saved::Files(files) => clipboard.set().file_list(&files),
            Saved::Image(image) => clipboard.set_image(image),
            Saved::Html { html, text } => clipboard.set_html(html, text),
            Saved::Text(text) => clipboard.set_text(text),
            Saved::Nothing => clipboard.clear(),
        };
        if let Err(e) = put {
            crate::diag::log(&format!("clipboard: could not restore what was there ({e})"));
        }
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
        Err(contextlens_core::t!(
            "ContextLens 需要「輔助使用」權限才能讀取選取的文字。\
在剛打開的「系統設定 → 隱私權與安全性 → 輔助使用」裡開啟 ContextLens，再按一次熱鍵。"
        ))
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
        Err(contextlens_core::t!(
            "ContextLens 需要「螢幕錄製」權限才能截圖。\
在「系統設定 → 隱私權與安全性 → 螢幕與系統錄音」裡開啟 ContextLens，然後結束並重新開啟 ContextLens。"
        ))
    }

    /// Shift, Control, Option or Command still down (the hotkey's own keys).
    pub fn modifiers_held() -> bool {
        const MASK: u64 = 0x20000 | 0x40000 | 0x80000 | 0x100000;
        // kCGEventSourceStateCombinedSessionState
        unsafe { CGEventSourceFlagsState(0) & MASK != 0 }
    }
}

fn send_copy() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| t!("無法模擬按鍵：{e}", e = e))?;
    // The hotkey's own modifiers may still be down; release the ones Copy must not inherit.
    for key in [Key::Shift, Key::Alt] {
        let _ = enigo.key(key, Direction::Release);
    }
    let err = |e: enigo::InputError| t!("無法模擬複製：{e}", e = e);
    enigo.key(COPY_MODIFIER, Direction::Press).map_err(err)?;
    let click = enigo.key(COPY_KEY, Direction::Click);
    let release = enigo.key(COPY_MODIFIER, Direction::Release);
    click.map_err(err)?;
    release.map_err(err)
}

#[cfg(target_os = "windows")]
const SCREENSHOT_KEYS: &str = "Win+Shift+S";
#[cfg(target_os = "macos")]
const SCREENSHOT_KEYS: &str = n_!("⌘⌃⇧4 會把截圖放進剪貼簿");
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const SCREENSHOT_KEYS: &str = n_!("例如 Shift+PrtSc");

/// The image on the clipboard, e.g. after Win+Shift+S or a screenshot tool.
pub fn clipboard_image() -> Result<Media, String> {
    let mut clipboard = Clipboard::new().map_err(|e| t!("無法讀取剪貼簿：{e}", e = e))?;
    let img = clipboard
        .get_image()
        .map_err(|_| t!("剪貼簿裡沒有圖片。先截圖（{keys}）再按熱鍵。", keys = tr(SCREENSHOT_KEYS)))?;
    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
        .ok_or_else(|| t!("剪貼簿的圖片格式無法讀取"))?;
    imaging::png_media(&rgba, &format!("{}.png", t!("剪貼簿圖片")))
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

    #[test]
    fn span_covers_every_line_and_skips_empty_ones() {
        let lines = [[10.0, 100.0, 300.0, 20.0], [10.0, 120.0, 0.0, 0.0], [10.0, 140.0, 120.0, 20.0]];
        assert_eq!(span_of(&lines), Some(Span { top: 100.0, bottom: 160.0 }));
        assert_eq!(span_of(&[]), None);
    }

    fn sentinel_after_a_moment() -> String {
        sleep(Duration::from_millis(2));
        sentinel()
    }
}
