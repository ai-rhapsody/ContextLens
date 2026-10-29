mod capture;
mod diag;
mod imaging;
mod live;
mod secrets;
mod snip;
mod store;

use contextlens_core::{
    caps, conversation, live::Backend, prompt, Caps, ChatRequest, Conversation, Delta, History, HistoryEntry,
    Hotkey, Media, Provider, Settings, Source, Task,
};
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use tauri::{
    ipc::Channel,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// What a hotkey grabbed, waiting for the popup to pick it up.
#[derive(Clone, Serialize, Default)]
struct Capture {
    text: String,
    media: Option<Media>,
    task: Task,
    tutor: bool,
    /// Set when capturing failed; the popup shows it instead of translating.
    error: Option<String>,
}

struct AppState {
    settings: Mutex<Settings>,
    pending: Mutex<Option<Capture>>,
    snip: Mutex<Option<snip::Snip>>,
    history: Mutex<History>,
    /// Bumped for every translation; an older stream stops when it sees a newer number.
    generation: AtomicU64,
    /// A pinned popup stays where the user put it instead of jumping to the cursor.
    popup_pinned: AtomicBool,
    live: Mutex<Option<live::LiveHandle>>,
    /// The conversation the chat window was last asked to show.
    chat_pending: Mutex<Option<u64>>,
    /// Recent conversations with their media, newest last, for follow-up questions.
    convos: Mutex<VecDeque<Conversation>>,
    /// Sources of the last Live session, reused when a hotkey starts the next one.
    live_sources: Mutex<Vec<live::LiveSource>>,
    /// The screen area Live watches when the screen source is on.
    live_screen: Mutex<Option<live::ScreenTarget>>,
    http: contextlens_core::reqwest::Client,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StreamEvent {
    Delta { text: String },
    Source { title: String, url: String },
    /// The answer is complete and can be followed up under this id.
    Conversation { id: u64 },
    /// Search fell back to the model's own knowledge; `reason` says why.
    Offline { reason: String },
    /// The question a follow-up is answering (shown above its answer).
    Question { text: String },
    Done,
    Error { message: String },
}

// ---------- windows ----------

fn popup(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("popup")
}

/// A window that has never been shown can report no size yet; fall back to a logical size.
fn outer_size_or(win: &WebviewWindow, fallback: [u32; 2], scale: f64) -> (f64, f64) {
    match win.outer_size() {
        Ok(s) if s.width > 50 && s.height > 50 => (s.width as f64, s.height as f64),
        _ => (fallback[0] as f64 * scale, fallback[1] as f64 * scale),
    }
}

fn place_near_cursor(app: &AppHandle, win: &WebviewWindow, selection: Option<capture::Span>) {
    let Ok(cursor) = app.cursor_position() else { return };
    let (mut x, mut y) = (cursor.x + 16.0, cursor.y + 16.0);
    // Fall back to the window's or the primary monitor when the point lookup finds none.
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(mon) = monitor {
        let configured = app.state::<AppState>().settings.lock().unwrap().popup_size.unwrap_or([440, 420]);
        let scale = mon.scale_factor();
        let (w, h) = outer_size_or(win, configured, scale);
        let (mp, ms) = (mon.position(), mon.size());
        let (top, bottom) = (mp.y as f64 + 8.0, mp.y as f64 + ms.height as f64 - 8.0);
        // Open below the selection, or above it when there is no room, but never clamp onto it
        // and cover the text being translated. When the app does not say where the selection
        // is, it is taken to end on the cursor's line.
        let span = selection.filter(|s| s.top >= top - 8.0 && s.bottom <= bottom + 8.0 && s.bottom > s.top);
        let (sel_top, sel_bottom, gap) = match span {
            Some(s) => (s.top, s.bottom, 8.0 * scale),
            None => (cursor.y, cursor.y, 28.0 * scale),
        };
        let below = sel_bottom + gap;
        let above = sel_top - gap - h;
        y = if below + h <= bottom {
            below
        } else if above >= top {
            above
        } else if bottom - sel_bottom >= sel_top - top {
            bottom - h
        } else {
            top
        };
        let max_x = mp.x as f64 + ms.width as f64 - w - 8.0;
        x = x.min(max_x).max(mp.x as f64 + 8.0);
        y = y.max(top);
    }
    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn show_popup(app: &AppHandle, capture: Capture) {
    show_popup_at(app, capture, None);
}

/// `selection`: where the captured text is on screen, to open beneath it.
fn show_popup_at(app: &AppHandle, capture: Capture, selection: Option<capture::Span>) {
    let Some(win) = popup(app) else { return };
    let state = app.state::<AppState>();
    *state.pending.lock().unwrap() = Some(capture);
    // Pinned: back to where the user put it, even after it was closed or the app restarted.
    if state.popup_pinned.load(Ordering::SeqCst) {
        if let Some([x, y]) = state.settings.lock().unwrap().popup_pos {
            let _ = win.set_position(PhysicalPosition::new(x, y));
        }
    } else {
        place_near_cursor(app, &win, selection);
    }
    let _ = win.show();
    let _ = win.set_focus();
    // The popup also calls take_pending_capture on mount, so a first show cannot be missed.
    let _ = app.emit_to("popup", "capture", ());
}

/// Saves the translation window's size, and its position when pinned.
fn remember_popup_size(app: &AppHandle) {
    let Some(win) = popup(app) else { return };
    let (Ok(size), Ok(scale)) = (win.inner_size(), win.scale_factor()) else { return };
    let logical = size.to_logical::<f64>(scale);
    let new = Some([logical.width.round() as u32, logical.height.round() as u32]);
    let pinned = app.state::<AppState>().popup_pinned.load(Ordering::SeqCst);
    let pos = win.outer_position().ok().map(|p| [p.x, p.y]);
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    let mut changed = s.popup_size != new;
    s.popup_size = new;
    if pinned && pos.is_some() && s.popup_pos != pos {
        s.popup_pos = pos;
        changed = true;
    }
    if changed {
        let _ = store::save(app, &s);
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// The tray icon toggles the settings window: hide it when it is in front, otherwise bring it up.
fn toggle_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let front = win.is_visible().unwrap_or(false)
            && !win.is_minimized().unwrap_or(false)
            && win.is_focused().unwrap_or(false);
        if front {
            let _ = win.hide();
            return;
        }
    }
    open_settings(app);
}

/// `--hotkey 2` → index 1 (hotkeys are numbered from 1 on the hotkeys page).
fn hotkey_arg(argv: &[String]) -> Option<usize> {
    let i = argv.iter().position(|a| a == "--hotkey")?;
    argv.get(i + 1)?.parse::<usize>().ok()?.checked_sub(1)
}

/// Passed by the sign-in launcher so ContextLens starts with only its tray icon.
const START_IN_TRAY_ARG: &str = "--tray";

#[derive(Serialize)]
struct PlatformInfo {
    os: &'static str,
    /// A Wayland session, where global hotkeys cannot be grabbed.
    wayland: bool,
    /// The command a desktop shortcut can run to press a hotkey.
    exe: String,
}

#[tauri::command]
fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        wayland: cfg!(target_os = "linux")
            && (std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland")
                || std::env::var_os("WAYLAND_DISPLAY").is_some()),
        // Inside an AppImage the running file is a temporary mount; APPIMAGE is the real one.
        exe: std::env::var("APPIMAGE")
            .ok()
            .or_else(|| std::env::current_exe().ok().map(|p| p.display().to_string()))
            .unwrap_or_else(|| "contextlens".into()),
    }
}

#[tauri::command]
fn autostart_get(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn autostart_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let l = app.autolaunch();
    let r = if enabled { l.enable() } else { l.disable() };
    r.map_err(|e| format!("無法變更開機啟動：{e}"))
}

/// Overlay windows are "snip-<monitor index>".
fn snip_index(label: &str) -> Option<usize> {
    label.strip_prefix("snip-")?.parse().ok()
}

fn close_snip_windows(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if snip_index(&label).is_some() {
            let _ = w.destroy();
        }
    }
}

/// Stores the capture, then opens its overlays. The capture must be in place first: an
/// overlay can finish loading and ask for its picture before the next one is even built.
fn start_snip(app: &AppHandle, s: snip::Snip) -> Result<(), String> {
    close_snip_windows(app);
    let geometry: Vec<(i32, i32, u32, u32)> =
        s.shots.iter().map(|sh| (sh.monitor_x, sh.monitor_y, sh.image.width(), sh.image.height())).collect();
    *app.state::<AppState>().snip.lock().unwrap() = Some(s);
    let opened = open_snip_windows(app, &geometry);
    if opened.is_err() {
        app.state::<AppState>().snip.lock().unwrap().take();
        close_snip_windows(app);
    }
    opened
}

/// One overlay per monitor, so each matches its monitor's scaling exactly. The one under
/// the cursor gets the keyboard focus.
fn open_snip_windows(app: &AppHandle, monitors: &[(i32, i32, u32, u32)]) -> Result<(), String> {
    let cursor = app.cursor_position().ok();
    let mut focus = None;
    for (i, &(mx, my, mw, mh)) in monitors.iter().enumerate() {
        let win = WebviewWindowBuilder::new(app, format!("snip-{i}"), WebviewUrl::App("index.html".into()))
            .title("ContextLens 擷取")
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .shadow(false)
            .visible(false)
            .build()
            .map_err(|e| e.to_string())?;
        // Physical units, so the overlay covers the captured monitor pixel for pixel. Moving
        // onto a monitor with other scaling makes Windows resize the window for the new DPI,
        // so place it again once it is there and shown.
        let place = |w: &WebviewWindow| {
            let _ = w.set_position(PhysicalPosition::new(mx, my));
            let _ = w.set_size(PhysicalSize::new(mw, mh));
        };
        place(&win);
        let _ = win.show();
        place(&win);
        let under_cursor = cursor.is_some_and(|c| {
            let (x, y) = (c.x as i32 - mx, c.y as i32 - my);
            x >= 0 && y >= 0 && (x as u32) < mw && (y as u32) < mh
        });
        if under_cursor || focus.is_none() {
            focus = Some(win);
        }
    }
    if let Some(w) = focus {
        let _ = w.set_focus();
    }
    Ok(())
}

fn open_live_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("live") else { return };
    if !win.is_visible().unwrap_or(false) {
        // Bottom centre of the monitor under the cursor, like subtitles.
        let monitor = app
            .cursor_position()
            .ok()
            .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
            .or_else(|| app.primary_monitor().ok().flatten());
        if let Some(mon) = monitor {
            let (mp, ms) = (mon.position(), mon.size());
            let (w, h) = outer_size_or(&win, [760, 240], mon.scale_factor());
            let margin = 72.0 * mon.scale_factor();
            let x = mp.x as f64 + (ms.width as f64 - w) / 2.0;
            let y = mp.y as f64 + ms.height as f64 - h - margin;
            let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.max(mp.y as f64).round() as i32));
        }
    }
    let _ = win.show();
    let _ = win.set_focus();
}

fn live_status(app: &AppHandle, state: &'static str, message: Option<String>) {
    diag::log(&format!("live: {state}{}", message.as_deref().map(|m| format!(" ({m})")).unwrap_or_default()));
    let _ = app.emit_to("live", "live", live::LiveUi::Status { state, message });
}

fn stop_live(app: &AppHandle) {
    if let Some(h) = app.state::<AppState>().live.lock().unwrap().take() {
        h.stop();
    }
    live_status(app, "stopped", None);
    let _ = app.emit_to("live", "live-ended", ());
}

/// Chooses where a Live session goes. Audio needs a real-time model (Gemini Live, OpenAI
/// Realtime); the screen alone works with any model that reads images. The chosen provider
/// comes first, then the default provider, then the others, keeping only those with a key.
/// `interpret`: the session only listens and adds no notes, so Gemini's interpreter model
/// (which takes no instructions and no screen) can serve it.
fn pick_engine(s: &Settings, audio: bool, interpret: bool) -> Result<live::Engine, String> {
    let order: Vec<Provider> = match s.live_provider {
        Some(p) => vec![p],
        None => std::iter::once(s.provider).chain(Provider::ALL.into_iter().filter(|p| *p != s.provider)).collect(),
    };
    for p in order {
        let Some(api_key) = secrets::get(p) else { continue };
        if audio {
            let (Some(backend), Some(model)) = (Backend::for_provider(p), s.live_model_for(p)) else { continue };
            let interpreter = s.models.gemini_translate.trim();
            let usable = live::language_code(&s.target_language).is_some();
            if p == Provider::Gemini && interpret && usable && !interpreter.is_empty() {
                let fallback = caps::is_live_model(p, model).then(|| model.to_string());
                return Ok(live::Engine::Realtime {
                    backend: Backend::GeminiTranslate,
                    model: interpreter.into(),
                    api_key,
                    fallback,
                });
            }
            if caps::is_live_model(p, model) {
                return Ok(live::Engine::Realtime { backend, model: model.into(), api_key, fallback: None });
            }
        } else if caps::for_model(p, s.model_for(p)).image {
            return Ok(live::Engine::Reader { provider: p, model: s.model_for(p).into(), api_key });
        }
    }
    Err(match (audio, s.live_provider) {
        (true, Some(p)) => format!(
            "{} 沒有可用的即時模型或金鑰。到「模型與金鑰」設定即時模型，或在字幕列改用其他服務。",
            p.display_name()
        ),
        (true, None) => "聽聲音需要 Gemini 或 ChatGPT 的金鑰和即時模型。到「模型與金鑰」設定。".into(),
        (false, _) => "看螢幕需要一個能讀圖片的模型和它的金鑰。".into(),
    })
}

fn start_live(app: &AppHandle, sources: Vec<live::LiveSource>, tutor: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(old) = state.live.lock().unwrap().take() {
        old.stop();
    }
    if sources.is_empty() {
        return Err("至少要開一個來源。".into());
    }
    let mut notice: Option<String> = None;
    let mut sources = sources;
    // No microphone (common on desktops): listen to what the computer plays instead.
    if sources.contains(&live::LiveSource::Mic) && !live::has_microphone() {
        sources.retain(|s| *s != live::LiveSource::Mic);
        if live::SYSTEM_AUDIO_SUPPORTED {
            if !sources.contains(&live::LiveSource::System) {
                sources.push(live::LiveSource::System);
            }
            notice = Some("找不到麥克風，改聽系統聲音（電腦正在播放的聲音）。".into());
        } else if sources.is_empty() {
            return Err("找不到麥克風。接上麥克風後再按「開始」，或改用「螢幕」。".into());
        } else {
            notice = Some("找不到麥克風，這次不聽麥克風。".into());
        }
        let _ = app.emit_to("live", "live-changed", ());
    }
    // Remember the choice even if starting fails, so Start retries it once the problem is fixed.
    *state.live_sources.lock().unwrap() = sources.clone();
    // Remembered across restarts too, so a video-watching setup does not reopen with the mic.
    let settings = {
        let mut s = state.settings.lock().unwrap();
        if s.live_sources != sources {
            s.live_sources = sources.clone();
            let _ = store::save(app, &s);
        }
        s.clone()
    };
    let audio = sources.iter().any(|s| *s != live::LiveSource::Screen);
    let interpret = audio && !tutor && !sources.contains(&live::LiveSource::Screen);
    let engine = match pick_engine(&settings, audio, interpret) {
        Ok(e) => e,
        // Nothing can listen, but the screen can still be watched: do that, and say why.
        Err(why) if audio && sources.contains(&live::LiveSource::Screen) => {
            sources.retain(|s| *s == live::LiveSource::Screen);
            let e = pick_engine(&settings, false, false)?;
            notice = Some(format!("{why} 這次先只看螢幕。"));
            e
        }
        Err(why) => return Err(why),
    };
    let watches_screen = sources.contains(&live::LiveSource::Screen);
    let screen = state.live_screen.lock().unwrap().clone();
    // Keep the caption bar out of the frames, so the model never reads its own captions
    // (Windows 10 2004+ and macOS honour this).
    if let Some(w) = app.get_webview_window("live") {
        let _ = w.set_content_protected(watches_screen);
    }
    let handle = live::start(
        app.clone(),
        live::StartArgs {
            sources,
            tutor,
            engine,
            target_language: settings.target_language,
            idle_minutes: settings.live_idle_minutes,
            screen,
            notice,
        },
    )?;
    *state.live.lock().unwrap() = Some(handle);
    // The caption bar shows the running model.
    let _ = app.emit_to("live", "live-changed", ());
    Ok(())
}

// ---------- hotkeys ----------

fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    // One hotkey taken by another app must not cost the others.
    let mut failed = Vec::new();
    for hk in &settings.hotkeys {
        let hotkey = hk.clone();
        let r = gs.on_shortcut(hk.accelerator.as_str(), move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                trigger(app.clone(), hotkey.clone());
            }
        });
        if let Err(e) = r {
            failed.push(format!("{}（{e}）", hk.accelerator));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("無法註冊熱鍵 {}，可能已被其他程式使用。", failed.join("、")))
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn trigger(app: AppHandle, hotkey: Hotkey) {
    tauri::async_runtime::spawn(async move {
        let (tutor, task) = (hotkey.tutor, hotkey.task);
        let preset = hotkey.live_sources.clone();
        let failed = |message: String| Capture { tutor, task, error: Some(message), ..Default::default() };
        match hotkey.source {
            Source::Selection => {
                // Let the user's fingers leave the hotkey before we press Copy.
                let _ = blocking(|| Ok(std::thread::sleep(std::time::Duration::from_millis(150)))).await;
                // Ask where the selection is alongside Copy, while its app still has the focus.
                let read = blocking(|| {
                    let span = capture::begin_selection_span();
                    let text = capture::selected_text();
                    Ok((span(), text))
                })
                .await;
                let (span, text) = read.unwrap_or_else(|e| (None, Err(e)));
                diag::log(&match (&span, &text) {
                    (Some(s), Ok(t)) => format!("selection: {} chars, on screen at y {:.0}..{:.0}", t.chars().count(), s.top, s.bottom),
                    (None, Ok(t)) => format!("selection: {} chars, position unknown (popup follows the cursor)", t.chars().count()),
                    (_, Err(e)) => format!("selection: none ({e})"),
                });
                let capture = match text {
                    Ok(text) => Capture { text, tutor, task, ..Default::default() },
                    Err(e) => failed(e),
                };
                show_popup_at(&app, capture, span);
            }
            Source::ClipboardImage => {
                let capture = match blocking(capture::clipboard_image).await {
                    Ok(media) => Capture { media: Some(media), tutor, task, ..Default::default() },
                    Err(e) => failed(e),
                };
                show_popup(&app, capture);
            }
            Source::Live => {
                let running = app.state::<AppState>().live.lock().unwrap().is_some();
                if running {
                    stop_live(&app);
                } else {
                    open_live_window(&app);
                    let state = app.state::<AppState>();
                    let mut sources = if preset.is_empty() { state.live_sources.lock().unwrap().clone() } else { preset };
                    // The screen needs a region first: start the rest, then let the user pick it.
                    // Picking adds the screen to the running session.
                    let pick = sources.contains(&live::LiveSource::Screen) && state.live_screen.lock().unwrap().is_none();
                    if pick {
                        sources.retain(|s| *s != live::LiveSource::Screen);
                    }
                    if !sources.is_empty() {
                        if let Err(e) = start_live(&app, sources, tutor) {
                            live_status(&app, "error", Some(e));
                        }
                    }
                    if pick {
                        let taken = blocking(|| snip::take(false, Task::Translate, snip::Purpose::LiveTarget)).await;
                        if let Err(e) = taken.and_then(|s| start_snip(&app, s)) {
                            live_status(&app, "error", Some(e));
                        }
                    }
                }
            }
            Source::Screen => {
                match blocking(move || snip::take(tutor, task, snip::Purpose::Translate)).await {
                    Ok(s) => {
                        if let Err(e) = start_snip(&app, s) {
                            show_popup(&app, failed(e));
                        }
                    }
                    Err(e) => show_popup(&app, failed(e)),
                }
            }
        }
    });
}

// ---------- commands: settings and keys ----------

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<AppState>, mut settings: Settings) -> Result<(), String> {
    settings.validate()?;
    // The popup's size, pin and position are owned by the window, not the settings page.
    {
        let current = state.settings.lock().unwrap();
        settings.popup_size = current.popup_size;
        settings.popup_pinned = current.popup_pinned;
        settings.popup_pos = current.popup_pos;
        // Also owned elsewhere: the caption bar and Live sessions.
        settings.live_provider = current.live_provider;
        settings.live_sources = current.live_sources.clone();
        settings.audio_only_models = current.audio_only_models.clone();
    }
    // Register first: if a hotkey is taken by another app, nothing is saved.
    if let Err(e) = register_hotkeys(&app, &settings) {
        let old = state.settings.lock().unwrap().clone();
        let _ = register_hotkeys(&app, &old);
        return Err(e);
    }
    store::save(&app, &settings)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[derive(Deserialize)]
struct KeyArgs {
    provider: Provider,
}

// Keychain calls can block (an OS unlock prompt, D-Bus), so these run off the main thread.
#[tauri::command]
async fn set_api_key(args: KeyArgs, key: String) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("金鑰不能是空的".into());
    }
    secrets::set(args.provider, &key)
}

#[tauri::command]
async fn delete_api_key(args: KeyArgs) -> Result<(), String> {
    secrets::delete(args.provider)
}

/// Which providers have a key stored. The keys themselves never leave Rust.
/// What a problem report needs: versions, the settings that matter (no keys), and the recent log.
#[tauri::command]
fn diagnostics(state: State<AppState>) -> String {
    let s = state.settings.lock().unwrap().clone();
    let keys: Vec<String> = Provider::ALL
        .iter()
        .map(|p| format!("{} {}", p.display_name(), if secrets::get(*p).is_some() { "有" } else { "無" }))
        .collect();
    let m = &s.models;
    let mut out = vec![
        format!("ContextLens {} · {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH),
        format!("目標語言：{} · 預設服務：{} · 金鑰：{}", s.target_language, s.provider.display_name(), keys.join("、")),
        format!(
            "模型：Gemini {} / {} · ChatGPT {} / {} · Claude {} / {}",
            m.gemini, m.gemini_explain, m.openai, m.openai_explain, m.anthropic, m.anthropic_explain
        ),
        format!("Live：{} / {} / 口譯 {} · 來源 {:?}", m.gemini_live, m.openai_realtime, m.gemini_translate, s.live_sources),
        format!("已是我的語言時翻成英文：{}", if s.own_language_to_english { "開" } else { "關" }),
        String::new(),
        "最近的記錄：".into(),
    ];
    out.extend(diag::tail(150));
    out.join("\n")
}

/// Shows the log file in the system's file manager.
#[tauri::command]
fn reveal_log(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let path = diag::path().ok_or("記錄檔還沒建立")?;
    if !path.exists() {
        diag::log("log opened");
    }
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn key_status() -> Vec<(Provider, bool)> {
    Provider::ALL.iter().map(|p| (*p, secrets::get(*p).is_some())).collect()
}

#[derive(Serialize)]
struct ProviderInfo {
    provider: Provider,
    model: String,
    caps: Caps,
    has_key: bool,
    /// Can search the web (Search task).
    web: bool,
    /// The real-time model, for providers that have a real-time API.
    live_model: Option<String>,
}

/// Each provider with its configured model, what that model takes, and whether a key is stored.
#[tauri::command]
async fn provider_info(state: State<'_, AppState>) -> Result<Vec<ProviderInfo>, String> {
    let settings = state.settings.lock().unwrap().clone();
    Ok(Provider::ALL
        .iter()
        .map(|&p| {
            let model = settings.model_for(p).to_string();
            let live_model = settings.live_model_for(p).map(String::from);
            // "Live" means the provider's real-time model is usable, not the text model.
            let caps = Caps {
                live: live_model.as_deref().is_some_and(|m| caps::is_live_model(p, m)),
                ..caps::for_model(p, &model)
            };
            ProviderInfo {
                provider: p,
                web: contextlens_core::can_search(p, &model),
                caps,
                has_key: secrets::get(p).is_some(),
                model,
                live_model,
            }
        })
        .collect())
}

// ---------- commands: popup ----------

#[tauri::command]
fn take_pending_capture(state: State<AppState>) -> Option<Capture> {
    state.pending.lock().unwrap().take()
}

#[tauri::command]
fn hide_popup(app: AppHandle) {
    remember_popup_size(&app);
    if let Some(w) = popup(&app) {
        let _ = w.hide();
    }
}

#[tauri::command]
fn set_popup_pinned(app: AppHandle, state: State<AppState>, pinned: bool) {
    state.popup_pinned.store(pinned, Ordering::SeqCst);
    let pos = popup(&app).and_then(|w| w.outer_position().ok()).map(|p| [p.x, p.y]);
    let mut s = state.settings.lock().unwrap();
    s.popup_pinned = pinned;
    if pinned {
        s.popup_pos = pos;
    }
    let _ = store::save(&app, &s);
}

#[tauri::command]
fn show_settings(app: AppHandle) {
    open_settings(&app);
}

// ---------- commands: snipping overlay ----------

/// The overlay asks for its own monitor's screenshot; its window label says which.
#[tauri::command]
fn snip_view(window: WebviewWindow, state: State<AppState>) -> Result<snip::SnipView, String> {
    let index = snip_index(window.label()).ok_or("不是擷取視窗")?;
    let guard = state.snip.lock().unwrap();
    snip::view(guard.as_ref().ok_or("沒有進行中的擷取")?, index)
}

#[tauri::command]
async fn finish_snip(
    app: AppHandle,
    window: WebviewWindow,
    rect: snip::Rect,
    label: Option<String>,
) -> Result<(), String> {
    let index = snip_index(window.label()).ok_or("不是擷取視窗")?;
    let mut s = app.state::<AppState>().snip.lock().unwrap().take().ok_or("沒有進行中的擷取")?;
    close_snip_windows(&app);
    if index >= s.shots.len() {
        return Err("沒有這個螢幕".into());
    }
    let shot = s.shots.swap_remove(index);
    if s.purpose == snip::Purpose::LiveTarget {
        let (x, y, w, h) = snip::clamp(rect, shot.image.width(), shot.image.height()).ok_or("選取範圍太小")?;
        let target = live::ScreenTarget {
            monitor_x: shot.monitor_x,
            monitor_y: shot.monitor_y,
            x, y, w, h,
            label: label.unwrap_or_else(|| format!("區域 {w}×{h}")),
        };
        let state = app.state::<AppState>();
        *state.live_screen.lock().unwrap() = Some(target);
        // Restart with the screen added to whatever was already on.
        let mut sources = match state.live.lock().unwrap().as_ref() {
            Some(h) => h.sources.clone(),
            None => state.live_sources.lock().unwrap().clone(),
        };
        if !sources.contains(&live::LiveSource::Screen) {
            sources.push(live::LiveSource::Screen);
        }
        let tutor = state.live.lock().unwrap().as_ref().is_some_and(|h| h.tutor);
        open_live_window(&app);
        let app2 = app.clone();
        if let Err(e) = blocking(move || start_live(&app2, sources, tutor)).await {
            live_status(&app, "error", Some(e));
        }
        let _ = app.emit_to("live", "live-changed", ()); // the caption bar re-reads its state
        return Ok(());
    }
    let (tutor, task) = (s.tutor, s.task);
    let capture = match blocking(move || snip::crop(&shot, rect)).await {
        Ok(media) => Capture { media: Some(media), tutor, task, ..Default::default() },
        Err(e) => Capture { tutor, task, error: Some(e), ..Default::default() },
    };
    show_popup(&app, capture);
    Ok(())
}

#[tauri::command]
fn cancel_snip(app: AppHandle, state: State<AppState>) {
    state.snip.lock().unwrap().take();
    close_snip_windows(&app);
}

// ---------- commands: translation ----------

#[derive(Deserialize)]
struct Input {
    #[serde(default)]
    text: String,
    media: Option<Media>,
}

/// Streams one completion with the stored key. Stops quietly when `gen` is no longer
/// the latest generation or when `emit` returns false.
async fn stream(
    state: &AppState,
    provider: Provider,
    model: &str,
    req: &ChatRequest,
    gen: Option<u64>,
    emit: impl FnMut(Delta) -> bool,
) -> Result<(), String> {
    stream_raw(state, provider, model, req, gen, emit).await.map_err(|e| e.to_string())
}

/// `stream`, keeping the error's kind so a caller can react to it (e.g. a rate limit).
async fn stream_raw(
    state: &AppState,
    provider: Provider,
    model: &str,
    req: &ChatRequest,
    gen: Option<u64>,
    mut emit: impl FnMut(Delta) -> bool,
) -> Result<(), contextlens_core::ProviderError> {
    let key = secrets::get(provider).ok_or_else(|| {
        contextlens_core::ProviderError::Unsupported(format!(
            "還沒設定 {} 的 API 金鑰。到「模型與金鑰」貼上金鑰。",
            provider.display_name()
        ))
    })?;
    let mut sources = 0;
    let result = contextlens_core::stream_completion(&state.http, provider, model, &key, req, |t| {
        if matches!(t, Delta::Source { .. }) {
            sources += 1;
        }
        gen.map_or(true, |g| state.generation.load(Ordering::SeqCst) == g) && emit(t)
    })
    .await;
    let what = format!("{} {model}{}", provider.id(), if req.web_search { " (web search)" } else { "" });
    match &result {
        Err(e) => diag::log(&format!("request failed: {what}: {e}")),
        Ok(()) if req.web_search => diag::log(&format!("request done: {what}, {sources} sources")),
        Ok(()) => {}
    }
    result
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[tauri::command]
async fn run_translation(
    app: AppHandle,
    state: State<'_, AppState>,
    input: Input,
    task: Task,
    tutor: bool,
    // Use another provider for this request only, e.g. when the default cannot read video.
    provider: Option<Provider>,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let gen = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let (provider, model, lang, keep) = {
        let s = state.settings.lock().unwrap();
        let p = provider.unwrap_or(s.provider);
        (p, s.model_for_task(p, task).to_string(), s.target_language.clone(), s.save_history)
    };
    let media = match input.media {
        Some(m) => Some(blocking(move || Ok(imaging::prepare(provider, m))).await?),
        None => None,
    };
    let web = contextlens_core::can_search(provider, &model);
    let mut req = prompt::build_task(task, tutor, &input.text, media, &lang, web);
    if task == Task::Translate {
        let to_english = state.settings.lock().unwrap().own_language_to_english;
        req.system.push_str(&prompt::language_note(&input.text, !req.media.is_empty(), &lang, to_english));
    }

    let mut full = String::new();
    let mut sources: Vec<(String, String)> = Vec::new();
    // A busy server (503, often on free keys) usually answers a moment later: try once more,
    // unless part of the answer is already showing.
    let mut retried = false;
    let mut result = loop {
        let r = stream_raw(&state, provider, &model, &req, Some(gen), |d| match d {
            Delta::Text(t) => {
                full.push_str(&t);
                on_event.send(StreamEvent::Delta { text: t }).is_ok()
            }
            Delta::Source { title, url } => {
                if sources.iter().any(|(_, u)| *u == url) {
                    return true;
                }
                sources.push((title.clone(), url.clone()));
                on_event.send(StreamEvent::Source { title, url }).is_ok()
            }
        })
        .await;
        if retried || !full.is_empty() || !r.as_ref().is_err_and(|e| e.is_transient()) {
            break r;
        }
        retried = true;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        if state.generation.load(Ordering::SeqCst) != gen {
            break r;
        }
    };
    // Web search (Gemini's Google grounding) is often unavailable on free keys and answers
    // 429. Answer from the model's own knowledge instead, and say so.
    if req.web_search && full.is_empty() && result.as_ref().is_err_and(|e| e.is_rate_limited()) {
        let _ = on_event.send(StreamEvent::Offline {
            reason: "這次無法上網搜尋（免費金鑰通常不支援搜尋，或已超過搜尋額度），以下是模型本身的知識，可能不是最新資訊。".into(),
        });
        req = prompt::build_task(task, tutor, &input.text, req.media.first().cloned(), &lang, false);
        result = stream_raw(&state, provider, &model, &req, Some(gen), |d| match d {
            Delta::Text(t) => {
                full.push_str(&t);
                on_event.send(StreamEvent::Delta { text: t }).is_ok()
            }
            Delta::Source { .. } => true,
        })
        .await;
    }
    let result = result.map_err(|e| e.to_string());
    // A newer request owns the popup now; stay silent and keep nothing.
    if state.generation.load(Ordering::SeqCst) != gen {
        return Ok(());
    }
    if result.is_ok() && !full.trim().is_empty() {
        let m = req.media.first();
        let kind = m.and_then(|m| m.kind()).map_or("text", |k| match k {
            contextlens_core::MediaKind::Image => "image",
            contextlens_core::MediaKind::Audio => "audio",
            contextlens_core::MediaKind::Video => "video",
        });
        let label = m.map(|m| m.name.clone().unwrap_or_else(|| m.kind().map_or("檔案", |k| k.label()).into()));
        let mut id = now_ms();
        if keep {
            let entry = HistoryEntry {
                at_ms: id,
                kind: kind.into(),
                source: label.clone().unwrap_or_else(|| input.text.trim().chars().take(4000).collect()),
                thumb: m.and_then(imaging::thumbnail),
                result: full.clone(),
                tutor,
                provider,
                model: model.clone(),
                sources: sources.clone(),
                thread: vec![],
                task: Some(task),
            };
            let mut h = state.history.lock().unwrap();
            id = h.push_id(entry);
            let _ = store::save_history(&app, &h);
            drop(h);
            let _ = app.emit_to("main", "history-changed", ());
        }
        remember_convo(
            &state,
            Conversation {
                id,
                task,
                tutor,
                text: input.text.clone(),
                media: req.media.first().cloned(),
                label: label.unwrap_or_default(),
                provider,
                model,
                answer: full,
                sources,
                thread: vec![],
                media_lost: false,
            },
        );
        let _ = on_event.send(StreamEvent::Conversation { id });
    }
    let _ = on_event.send(match result {
        Ok(()) => StreamEvent::Done,
        Err(message) => StreamEvent::Error { message },
    });
    Ok(())
}

// ---------- conversations ----------

/// How many conversations keep their media in memory.
const CONVOS_KEPT: usize = 20;

fn remember_convo(state: &AppState, c: Conversation) {
    let mut list = state.convos.lock().unwrap();
    list.retain(|x| x.id != c.id);
    list.push_back(c);
    while list.len() > CONVOS_KEPT {
        list.pop_front();
    }
}

/// A conversation from memory, or rebuilt from history (without its media) when it is older.
fn find_convo(state: &AppState, id: u64) -> Option<Conversation> {
    if let Some(c) = state.convos.lock().unwrap().iter().find(|c| c.id == id) {
        return Some(c.clone());
    }
    let h = state.history.lock().unwrap();
    let e = h.get(id)?;
    let media_kind = e.kind != "text" && e.kind != "live";
    Some(Conversation {
        id,
        task: e.task.unwrap_or_default(),
        tutor: e.tutor,
        text: if media_kind { String::new() } else { e.source.clone() },
        media: None,
        label: if media_kind { e.source.clone() } else { String::new() },
        provider: e.provider,
        model: e.model.clone(),
        answer: e.result.clone(),
        sources: e.sources.clone(),
        thread: e.thread.clone(),
        media_lost: media_kind,
    })
}

#[tauri::command]
fn convo_get(state: State<AppState>, id: u64) -> Option<Conversation> {
    find_convo(&state, id)
}

/// Asks a follow-up question about a conversation and streams the answer. `action` with
/// `phrase` builds the question for a phrase selected in an answer.
#[tauri::command]
async fn follow_up(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    question: Option<String>,
    action: Option<String>,
    phrase: Option<String>,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let convo = find_convo(&state, id).ok_or("找不到這段對話，可能已從歷史紀錄刪除。")?;
    let question = match (question, action, phrase) {
        (Some(q), _, _) if !q.trim().is_empty() => q.trim().to_string(),
        (_, Some(a), Some(p)) if !p.trim().is_empty() => conversation::about_selection(&a, p.trim()),
        _ => return Err("問題是空的".into()),
    };
    let lang = state.settings.lock().unwrap().target_language.clone();
    let web = contextlens_core::can_search(convo.provider, &convo.model);
    let req = convo.follow_up(&question, &lang, web);
    let _ = on_event.send(StreamEvent::Question { text: question.clone() });

    let mut answer = String::new();
    // A busy server usually answers a moment later: one more try before any text arrived.
    let mut retried = false;
    let result = loop {
        let r = stream_raw(&state, convo.provider, &convo.model, &req, None, |d| match d {
            Delta::Text(t) => {
                answer.push_str(&t);
                on_event.send(StreamEvent::Delta { text: t }).is_ok()
            }
            Delta::Source { title, url } => on_event.send(StreamEvent::Source { title, url }).is_ok(),
        })
        .await;
        if retried || !answer.is_empty() || !r.as_ref().is_err_and(|e| e.is_transient()) {
            break r.map_err(|e| e.to_string());
        }
        retried = true;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    };
    if result.is_ok() && !answer.trim().is_empty() {
        let mut updated = convo;
        updated.thread.push((question.clone(), answer.clone()));
        remember_convo(&state, updated);
        let mut h = state.history.lock().unwrap();
        if h.append_thread(id, question, answer) {
            let _ = store::save_history(&app, &h);
            drop(h);
            let _ = app.emit_to("main", "history-changed", ());
        }
        // Both the popup and the conversation window may show this conversation.
        let _ = app.emit("convo-updated", id);
    }
    let _ = on_event.send(match result {
        Ok(()) => StreamEvent::Done,
        Err(message) => StreamEvent::Error { message },
    });
    Ok(())
}

fn open_chat_window(app: &AppHandle, id: u64) {
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    *app.state::<AppState>().chat_pending.lock().unwrap() = Some(id);
    let _ = app.emit_to("chat", "chat-open", id);
}

#[tauri::command]
fn open_chat(app: AppHandle, id: u64) {
    open_chat_window(&app, id);
}

/// The conversation the chat window should show when it first loads.
#[tauri::command]
fn chat_pending(state: State<AppState>) -> Option<u64> {
    *state.chat_pending.lock().unwrap()
}

/// Starts a conversation about one Live caption line and opens it in the chat window.
/// Live keeps running.
#[tauri::command]
fn live_follow_up(app: AppHandle, state: State<AppState>, input: String, output: String) -> Result<(), String> {
    let (provider, model, keep) = {
        let s = state.settings.lock().unwrap();
        (s.provider, s.model_for(s.provider).to_string(), s.save_history)
    };
    let mut id = now_ms();
    if keep {
        let mut h = state.history.lock().unwrap();
        id = h.push_id(live_line_entry(&input, &output, provider, &model));
        let _ = store::save_history(&app, &h);
        drop(h);
        let _ = app.emit_to("main", "history-changed", ());
    }
    remember_convo(
        &state,
        Conversation {
            id,
            task: Task::Translate,
            tutor: false,
            text: input,
            media: None,
            label: String::new(),
            provider,
            model,
            answer: output,
            sources: vec![],
            thread: vec![],
            media_lost: false,
        },
    );
    open_chat_window(&app, id);
    Ok(())
}

fn live_line_entry(input: &str, output: &str, provider: Provider, model: &str) -> HistoryEntry {
    HistoryEntry {
        at_ms: now_ms(),
        kind: "live".into(),
        source: input.trim().chars().take(4000).collect(),
        thumb: None,
        result: output.trim().to_string(),
        tutor: false,
        provider,
        model: model.into(),
        sources: vec![],
        thread: vec![],
        task: Some(Task::Translate),
    }
}

/// Keeps one Live caption line in history right away.
#[tauri::command]
fn live_star(app: AppHandle, state: State<AppState>, input: String, output: String) -> Result<u64, String> {
    let (provider, model) = {
        let s = state.settings.lock().unwrap();
        (s.provider, s.model_for(s.provider).to_string())
    };
    let mut h = state.history.lock().unwrap();
    let id = h.push_id(live_line_entry(&input, &output, provider, &model));
    store::save_history(&app, &h)?;
    drop(h);
    let _ = app.emit_to("main", "history-changed", ());
    Ok(id)
}

/// Opens a real Live session with the provider's real-time model and waits for one reply.
#[tauri::command]
async fn test_live(app: AppHandle, state: State<'_, AppState>, args: KeyArgs) -> Result<String, String> {
    let (model, target) = {
        let s = state.settings.lock().unwrap();
        (s.live_model_for(args.provider).map(String::from), s.target_language.clone())
    };
    let backend = contextlens_core::live::Backend::for_provider(args.provider)
        .ok_or_else(|| format!("{} 沒有即時 API。", args.provider.display_name()))?;
    let model = model.filter(|m| !m.trim().is_empty()).ok_or("先選一個即時模型。")?;
    let key = secrets::get(args.provider).ok_or("先貼上並儲存 API 金鑰。")?;
    live::probe(&app, backend, &model, &key, &target).await?;
    let interpreter = state.settings.lock().unwrap().models.gemini_translate.trim().to_string();
    if args.provider != Provider::Gemini || interpreter.is_empty() {
        return Ok(model);
    }
    if live::language_code(&target).is_none() {
        return Ok(format!("{model}；口譯模型不支援「{target}」，Live 會一律用 {model}"));
    }
    match live::probe(&app, contextlens_core::live::Backend::GeminiTranslate, &interpreter, &key, &target).await {
        Ok(()) => Ok(format!("{model}，口譯 {interpreter}")),
        Err(e) => Err(format!("即時模型 {model} 可以用，但口譯模型 {interpreter} 無法使用，Live 會改用 {model}。原因：{e}")),
    }
}

/// Sends a tiny request with the stored key so the settings page can say "works" or why not.
#[tauri::command]
async fn test_provider(state: State<'_, AppState>, args: KeyArgs) -> Result<String, String> {
    let model = state.settings.lock().unwrap().model_for(args.provider).to_string();
    let req = ChatRequest {
        system: "Reply with the single word OK.".into(),
        user: "ping".into(),
        quick: true,
        ..Default::default()
    };
    stream(&state, args.provider, &model, &req, None, |_| true).await?;
    Ok(model)
}

// ---------- commands: live ----------

#[derive(Serialize)]
struct LiveOption {
    provider: Provider,
    /// Has a key and a real-time model, so it can listen.
    audio: bool,
    /// Has a key and a model that reads images, so it can watch the screen.
    screen: bool,
}

#[derive(Serialize)]
struct LiveInfo {
    running: bool,
    sources: Vec<live::LiveSource>,
    tutor: bool,
    /// The running session's provider and model.
    label: Option<String>,
    /// The provider chosen in the caption bar; None means automatic.
    chosen: Option<Provider>,
    options: Vec<LiveOption>,
    system_audio: bool,
    /// What the screen source watches, if one was picked.
    screen: Option<String>,
}

#[tauri::command]
async fn live_info(app: AppHandle) -> LiveInfo {
    // Reads the keychain, which can block, so this runs off the main thread.
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let options = Provider::ALL
        .iter()
        .filter(|p| secrets::get(**p).is_some())
        .map(|&p| LiveOption {
            provider: p,
            audio: settings.live_model_for(p).is_some_and(|m| caps::is_live_model(p, m)),
            screen: caps::for_model(p, settings.model_for(p)).image,
        })
        .collect();
    let running = state.live.lock().unwrap().as_ref().map(|h| (h.sources.clone(), h.tutor, h.label.lock().unwrap().clone()));
    let last_sources = state.live_sources.lock().unwrap().clone();
    let screen = state.live_screen.lock().unwrap().as_ref().map(|t| t.label.clone());
    LiveInfo {
        running: running.is_some(),
        sources: running.as_ref().map_or(last_sources, |r| r.0.clone()),
        tutor: running.as_ref().is_some_and(|r| r.1),
        label: running.map(|r| r.2),
        chosen: settings.live_provider,
        options,
        system_audio: live::SYSTEM_AUDIO_SUPPORTED,
        screen,
    }
}

/// Picks the provider for Live (None = automatic); a running session restarts with it.
#[tauri::command]
async fn live_set_provider(app: AppHandle, provider: Option<Provider>) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.live_provider = provider;
        store::save(&app, &s)?;
    }
    // The settings page and the caption bar both show this choice.
    let _ = app.emit("settings-changed", ());
    let _ = app.emit_to("live", "live-changed", ());
    let running = state.live.lock().unwrap().as_ref().map(|h| (h.sources.clone(), h.tutor));
    if let Some((sources, tutor)) = running {
        let app2 = app.clone();
        blocking(move || start_live(&app2, sources, tutor)).await?;
    }
    Ok(())
}

#[tauri::command]
async fn live_start(app: AppHandle, sources: Vec<live::LiveSource>, tutor: bool) -> Result<(), String> {
    // Opening audio devices can block for a moment.
    blocking(move || start_live(&app, sources, tutor)).await
}

#[tauri::command]
fn live_stop(app: AppHandle) {
    stop_live(&app);
}

#[tauri::command]
fn open_live(app: AppHandle) {
    open_live_window(&app);
}

/// Opens the snipping overlay to choose what Live watches.
#[tauri::command]
async fn live_pick_screen(app: AppHandle) -> Result<(), String> {
    let s = blocking(move || snip::take(false, Task::Translate, snip::Purpose::LiveTarget)).await?;
    start_snip(&app, s)
}

/// Closing the caption bar ends the session; Live is billed while it runs.
#[tauri::command]
fn close_live(app: AppHandle) {
    stop_live(&app);
    if let Some(w) = app.get_webview_window("live") {
        let _ = w.hide();
    }
}

// ---------- commands: history ----------

#[tauri::command]
fn history_list(state: State<AppState>, query: String) -> Vec<HistoryEntry> {
    state.history.lock().unwrap().search(&query, 300)
}

#[tauri::command]
fn history_delete(app: AppHandle, state: State<AppState>, at_ms: u64) -> Result<(), String> {
    let mut h = state.history.lock().unwrap();
    if h.remove(at_ms) {
        store::save_history(&app, &h)?;
    }
    Ok(())
}

#[tauri::command]
fn history_clear(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let mut h = state.history.lock().unwrap();
    h.clear();
    store::save_history(&app, &h)
}

// ---------- app ----------

pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch hands over to the running app and exits.
        // A second launch shows the running one, or with `--hotkey N` presses hotkey N: on
        // Wayland, where apps cannot grab global keys, a desktop shortcut can run that command.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _| match hotkey_arg(&argv) {
            Some(n) => {
                let hk = app.state::<AppState>().settings.lock().unwrap().hotkeys.get(n).cloned();
                match hk {
                    Some(hk) => trigger(app.clone(), hk),
                    None => open_settings(app),
                }
            }
            None => open_settings(app),
        }))
        // Started by the system at sign-in: stay in the tray.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![START_IN_TRAY_ARG]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            autostart_get,
            autostart_set,
            platform_info,
            test_live,
            save_settings,
            set_api_key,
            delete_api_key,
            key_status,
            provider_info,
            take_pending_capture,
            hide_popup,
            set_popup_pinned,
            show_settings,
            snip_view,
            finish_snip,
            cancel_snip,
            run_translation,
            test_provider,
            history_list,
            history_delete,
            history_clear,
            live_info,
            live_start,
            live_stop,
            open_live,
            close_live,
            live_pick_screen,
            live_set_provider,
            convo_get,
            follow_up,
            open_chat,
            chat_pending,
            live_follow_up,
            live_star,
            diagnostics,
            reveal_log,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            if let Ok(dir) = handle.path().app_log_dir() {
                diag::init(dir);
            }
            let settings = store::load(&handle);
            app.manage(AppState {
                settings: Mutex::new(settings.clone()),
                pending: Mutex::new(None),
                snip: Mutex::new(None),
                history: Mutex::new(store::load_history(&handle)),
                generation: AtomicU64::new(0),
                popup_pinned: AtomicBool::new(settings.popup_pinned),
                live: Mutex::new(None),
                chat_pending: Mutex::new(None),
                convos: Mutex::new(VecDeque::new()),
                live_sources: Mutex::new(if settings.live_sources.is_empty() {
                    vec![live::LiveSource::Mic]
                } else {
                    settings.live_sources.clone()
                }),
                live_screen: Mutex::new(None),
                http: contextlens_core::http_client(),
            });
            if let Err(e) = register_hotkeys(&handle, &settings) {
                diag::log(&format!("hotkeys: {e}"));
            }

            // Windows are declared with `create: false` and built only now, after the state
            // above is managed; otherwise a webview can call a command before it exists.
            for cfg in app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(&handle, &cfg)?.build()?;
            }
            if let (Some([w, h]), Some(win)) = (settings.popup_size, popup(&handle)) {
                let _ = win.set_size(LogicalSize::new(w as f64, h as f64));
            }
            let in_tray = settings.start_in_tray || std::env::args().any(|a| a == START_IN_TRAY_ARG);
            if !in_tray {
                open_settings(&handle);
            }

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "translator", "開啟翻譯視窗", true, None::<&str>)?,
                    &MenuItem::with_id(app, "live", "Live 即時翻譯", true, None::<&str>)?,
                    &MenuItem::with_id(app, "settings", "開啟 ContextLens", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "結束 ContextLens", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("window icon"))
                .tooltip("ContextLens")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "translator" => show_popup(app, Capture::default()),
                    "live" => open_live_window(app),
                    "settings" => open_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = e
                    {
                        toggle_settings(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Dragging a pinned translation window moves where it stays.
            if let WindowEvent::Moved(p) = event {
                if window.label() == "popup" {
                    let state = window.app_handle().state::<AppState>();
                    if state.popup_pinned.load(Ordering::SeqCst) {
                        state.settings.lock().unwrap().popup_pos = Some([p.x, p.y]);
                    }
                }
            }
            // Minimizing the settings window can tuck it into the tray instead of the taskbar.
            if let WindowEvent::Resized(_) = event {
                let app = window.app_handle();
                if window.label() == "main"
                    && window.is_minimized().unwrap_or(false)
                    && app.state::<AppState>().settings.lock().unwrap().minimize_to_tray
                {
                    let _ = window.hide();
                }
            }
            // Closing a window keeps ContextLens alive in the tray so hotkeys still work,
            // unless the user chose to quit when the settings window closes.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if window.label() == "main" && !app.state::<AppState>().settings.lock().unwrap().close_to_tray {
                    stop_live(app);
                    app.exit(0);
                    return;
                }
                api.prevent_close();
                if window.label() == "popup" {
                    remember_popup_size(window.app_handle());
                }
                // Closing the caption bar ends the session; Live is billed while it runs.
                if window.label() == "live" {
                    stop_live(window.app_handle());
                }
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running ContextLens");
}
