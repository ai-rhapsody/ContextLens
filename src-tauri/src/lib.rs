mod capture;
mod imaging;
mod live;
mod secrets;
mod snip;
mod store;

use contextlens_core::{
    caps, prompt, Caps, ChatRequest, History, HistoryEntry, Hotkey, Media, Provider, Settings, Source,
};
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

fn place_near_cursor(app: &AppHandle, win: &WebviewWindow) {
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
        let (w, h) = outer_size_or(win, configured, mon.scale_factor());
        let (mp, ms) = (mon.position(), mon.size());
        let max_x = mp.x as f64 + ms.width as f64 - w - 8.0;
        let max_y = mp.y as f64 + ms.height as f64 - h - 8.0;
        x = x.min(max_x).max(mp.x as f64 + 8.0);
        y = y.min(max_y).max(mp.y as f64 + 8.0);
    }
    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn show_popup(app: &AppHandle, capture: Capture) {
    let Some(win) = popup(app) else { return };
    let state = app.state::<AppState>();
    *state.pending.lock().unwrap() = Some(capture);
    let stay = state.popup_pinned.load(Ordering::SeqCst) && win.is_visible().unwrap_or(false);
    if !stay {
        place_near_cursor(app, &win);
    }
    let _ = win.show();
    let _ = win.set_focus();
    // The popup also calls take_pending_capture on mount, so a first show cannot be missed.
    let _ = app.emit_to("popup", "capture", ());
}

fn remember_popup_size(app: &AppHandle) {
    let Some(win) = popup(app) else { return };
    let (Ok(size), Ok(scale)) = (win.inner_size(), win.scale_factor()) else { return };
    let logical = size.to_logical::<f64>(scale);
    let new = Some([logical.width.round() as u32, logical.height.round() as u32]);
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    if s.popup_size != new {
        s.popup_size = new;
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

fn close_snip_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("snip") {
        let _ = w.destroy();
    }
}

fn open_snip_window(app: &AppHandle, s: &snip::Snip) -> Result<(), String> {
    close_snip_window(app);
    let win = WebviewWindowBuilder::new(app, "snip", WebviewUrl::App("index.html".into()))
        .title("ContextLens 擷取")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    // Physical units, so the overlay covers the captured monitor pixel for pixel.
    let _ = win.set_position(PhysicalPosition::new(s.monitor_x, s.monitor_y));
    let _ = win.set_size(PhysicalSize::new(s.image.width(), s.image.height()));
    let _ = win.show();
    let _ = win.set_focus();
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
    let _ = app.emit_to("live", "live", live::LiveUi::Status { state, message });
}

fn stop_live(app: &AppHandle) {
    if let Some(h) = app.state::<AppState>().live.lock().unwrap().take() {
        h.stop();
    }
    live_status(app, "stopped", None);
    let _ = app.emit_to("live", "live-ended", ());
}

fn start_live(app: &AppHandle, sources: Vec<live::LiveSource>, tutor: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(old) = state.live.lock().unwrap().take() {
        old.stop();
    }
    if sources.is_empty() {
        return Err("至少要開一個來源。".into());
    }
    // Remember the choice even if starting fails, so "開始" retries it once the problem is fixed.
    *state.live_sources.lock().unwrap() = sources.clone();
    let (model, target_language, idle_minutes) = {
        let s = state.settings.lock().unwrap();
        (s.models.gemini_live.clone(), s.target_language.clone(), s.live_idle_minutes)
    };
    if !caps::for_model(Provider::Gemini, &model).live {
        return Err(format!("{model} 不是 Live 模型。到設定的「Live」頁換一個名稱含 live 的 Gemini 模型。"));
    }
    let api_key = secrets::get(Provider::Gemini)
        .ok_or("Live 模式使用 Gemini，還沒設定 Gemini 的 API 金鑰。")?;
    let watches_screen = sources.contains(&live::LiveSource::Screen);
    let screen = state.live_screen.lock().unwrap().clone();
    // Keep the caption bar out of the frames, so the model never reads its own captions
    // (Windows 10 2004+ and macOS honour this).
    if let Some(w) = app.get_webview_window("live") {
        let _ = w.set_content_protected(watches_screen);
    }
    let handle = live::start(
        app.clone(),
        live::StartArgs { sources, tutor, model, target_language, api_key, idle_minutes, screen },
    )?;
    *state.live.lock().unwrap() = Some(handle);
    Ok(())
}

// ---------- hotkeys ----------

fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    for hk in &settings.hotkeys {
        let hotkey = hk.clone();
        gs.on_shortcut(hk.accelerator.as_str(), move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                trigger(app.clone(), hotkey.clone());
            }
        })
        .map_err(|e| format!("無法註冊熱鍵 {}：{e}", hk.accelerator))?;
    }
    Ok(())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn trigger(app: AppHandle, hotkey: Hotkey) {
    tauri::async_runtime::spawn(async move {
        let tutor = hotkey.tutor;
        let failed = |message: String| Capture { tutor, error: Some(message), ..Default::default() };
        match hotkey.source {
            Source::Selection => {
                // Let the user's fingers leave the hotkey before we press Copy.
                let _ = blocking(|| Ok(std::thread::sleep(std::time::Duration::from_millis(150)))).await;
                let capture = match blocking(capture::selected_text).await {
                    Ok(text) => Capture { text, tutor, ..Default::default() },
                    Err(e) => failed(e),
                };
                show_popup(&app, capture);
            }
            Source::ClipboardImage => {
                let capture = match blocking(capture::clipboard_image).await {
                    Ok(media) => Capture { media: Some(media), tutor, ..Default::default() },
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
                    let sources = app.state::<AppState>().live_sources.lock().unwrap().clone();
                    if let Err(e) = start_live(&app, sources, tutor) {
                        live_status(&app, "error", Some(e));
                    }
                }
            }
            Source::Screen => {
                let (x, y) = app.cursor_position().map(|p| (p.x as i32, p.y as i32)).unwrap_or((0, 0));
                match blocking(move || snip::take(x, y, tutor, snip::Purpose::Translate)).await {
                    Ok(s) => {
                        let opened = open_snip_window(&app, &s);
                        *app.state::<AppState>().snip.lock().unwrap() = Some(s);
                        if let Err(e) = opened {
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
    // The popup size is owned by the window, not the settings page.
    settings.popup_size = state.settings.lock().unwrap().popup_size;
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
}

/// Each provider with its configured model, what that model takes, and whether a key is stored.
#[tauri::command]
async fn provider_info(state: State<'_, AppState>) -> Result<Vec<ProviderInfo>, String> {
    let settings = state.settings.lock().unwrap().clone();
    Ok(Provider::ALL
        .iter()
        .map(|&p| {
            let model = settings.model_for(p).to_string();
            ProviderInfo { provider: p, caps: caps::for_model(p, &model), has_key: secrets::get(p).is_some(), model }
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
fn set_popup_pinned(state: State<AppState>, pinned: bool) {
    state.popup_pinned.store(pinned, Ordering::SeqCst);
}

#[tauri::command]
fn show_settings(app: AppHandle) {
    open_settings(&app);
}

// ---------- commands: snipping overlay ----------

#[tauri::command]
fn snip_view(state: State<AppState>) -> Result<snip::SnipView, String> {
    let guard = state.snip.lock().unwrap();
    snip::view(guard.as_ref().ok_or("沒有進行中的擷取")?)
}

#[tauri::command]
async fn finish_snip(app: AppHandle, rect: snip::Rect, label: Option<String>) -> Result<(), String> {
    let s = app.state::<AppState>().snip.lock().unwrap().take().ok_or("沒有進行中的擷取")?;
    close_snip_window(&app);
    if s.purpose == snip::Purpose::LiveTarget {
        let (x, y, w, h) = snip::clamp(rect, s.image.width(), s.image.height()).ok_or("選取範圍太小")?;
        let target = live::ScreenTarget {
            monitor_x: s.monitor_x,
            monitor_y: s.monitor_y,
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
    let tutor = s.tutor;
    let capture = match blocking(move || snip::crop(&s, rect)).await {
        Ok(media) => Capture { media: Some(media), tutor, ..Default::default() },
        Err(e) => Capture { tutor, error: Some(e), ..Default::default() },
    };
    show_popup(&app, capture);
    Ok(())
}

#[tauri::command]
fn cancel_snip(app: AppHandle, state: State<AppState>) {
    state.snip.lock().unwrap().take();
    close_snip_window(&app);
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
    mut emit: impl FnMut(&str) -> bool,
) -> Result<(), String> {
    let key = secrets::get(provider).ok_or_else(|| {
        format!("還沒設定 {} 的 API 金鑰。到「模型與金鑰」貼上金鑰。", provider.display_name())
    })?;
    contextlens_core::stream_completion(&state.http, provider, model, &key, req, |t| {
        gen.map_or(true, |g| state.generation.load(Ordering::SeqCst) == g) && emit(t)
    })
    .await
    .map_err(|e| e.to_string())
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
    tutor: bool,
    // Use another provider for this request only, e.g. when the default cannot read video.
    provider: Option<Provider>,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let gen = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let (provider, model, lang, keep) = {
        let s = state.settings.lock().unwrap();
        let p = provider.unwrap_or(s.provider);
        (p, s.model_for(p).to_string(), s.target_language.clone(), s.save_history)
    };
    let mode = if tutor { prompt::Mode::Tutor } else { prompt::Mode::Translate };
    let media = match input.media {
        Some(m) => Some(blocking(move || Ok(imaging::prepare(provider, m))).await?),
        None => None,
    };
    let req = prompt::build(mode, &input.text, media, &lang);

    let mut full = String::new();
    let result = stream(&state, provider, &model, &req, Some(gen), |t| {
        full.push_str(t);
        on_event.send(StreamEvent::Delta { text: t.to_string() }).is_ok()
    })
    .await;
    // A newer request owns the popup now; stay silent and keep nothing.
    if state.generation.load(Ordering::SeqCst) != gen {
        return Ok(());
    }
    if result.is_ok() && keep && !full.trim().is_empty() {
        let m = req.media.first();
        let entry = HistoryEntry {
            at_ms: now_ms(),
            kind: m.and_then(|m| m.kind()).map_or("text", |k| match k {
                contextlens_core::MediaKind::Image => "image",
                contextlens_core::MediaKind::Audio => "audio",
                contextlens_core::MediaKind::Video => "video",
            })
            .into(),
            source: match m {
                Some(m) => m.name.clone().unwrap_or_else(|| m.kind().map_or("檔案", |k| k.label()).into()),
                None => input.text.trim().chars().take(4000).collect(),
            },
            thumb: m.and_then(imaging::thumbnail),
            result: full,
            tutor,
            provider,
            model,
        };
        let mut h = state.history.lock().unwrap();
        h.push(entry);
        let _ = store::save_history(&app, &h);
        drop(h);
        let _ = app.emit_to("main", "history-changed", ());
    }
    let _ = on_event.send(match result {
        Ok(()) => StreamEvent::Done,
        Err(message) => StreamEvent::Error { message },
    });
    Ok(())
}

/// Sends a tiny request with the stored key so the settings page can say "works" or why not.
#[tauri::command]
async fn test_provider(state: State<'_, AppState>, args: KeyArgs) -> Result<String, String> {
    let model = state.settings.lock().unwrap().model_for(args.provider).to_string();
    let req = ChatRequest { system: "Reply with the single word OK.".into(), user: "ping".into(), media: vec![] };
    stream(&state, args.provider, &model, &req, None, |_| true).await?;
    Ok(model)
}

// ---------- commands: live ----------

#[derive(Serialize)]
struct LiveInfo {
    running: bool,
    sources: Vec<live::LiveSource>,
    tutor: bool,
    model: String,
    model_ok: bool,
    system_audio: bool,
    /// What the screen source watches, if one was picked.
    screen: Option<String>,
}

#[tauri::command]
fn live_info(state: State<AppState>) -> LiveInfo {
    let model = state.settings.lock().unwrap().models.gemini_live.clone();
    let current = state.live.lock().unwrap();
    LiveInfo {
        running: current.is_some(),
        sources: current.as_ref().map_or_else(|| state.live_sources.lock().unwrap().clone(), |h| h.sources.clone()),
        tutor: current.as_ref().is_some_and(|h| h.tutor),
        model_ok: caps::for_model(Provider::Gemini, &model).live,
        model,
        system_audio: live::SYSTEM_AUDIO_SUPPORTED,
        screen: state.live_screen.lock().unwrap().as_ref().map(|t| t.label.clone()),
    }
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
    let (x, y) = app.cursor_position().map(|p| (p.x as i32, p.y as i32)).unwrap_or((0, 0));
    let s = blocking(move || snip::take(x, y, false, snip::Purpose::LiveTarget)).await?;
    open_snip_window(&app, &s)?;
    *app.state::<AppState>().snip.lock().unwrap() = Some(s);
    Ok(())
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
        .plugin(tauri_plugin_single_instance::init(|app, _, _| open_settings(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
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
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = store::load(&handle);
            app.manage(AppState {
                settings: Mutex::new(settings.clone()),
                pending: Mutex::new(None),
                snip: Mutex::new(None),
                history: Mutex::new(store::load_history(&handle)),
                generation: AtomicU64::new(0),
                popup_pinned: AtomicBool::new(false),
                live: Mutex::new(None),
                live_sources: Mutex::new(vec![live::LiveSource::Mic]),
                live_screen: Mutex::new(None),
                http: contextlens_core::http_client(),
            });
            if let Err(e) = register_hotkeys(&handle, &settings) {
                eprintln!("hotkeys: {e}");
            }

            // Windows are declared with `create: false` and built only now, after the state
            // above is managed; otherwise a webview can call a command before it exists.
            for cfg in app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(&handle, &cfg)?.build()?;
            }
            if let (Some([w, h]), Some(win)) = (settings.popup_size, popup(&handle)) {
                let _ = win.set_size(LogicalSize::new(w as f64, h as f64));
            }

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "translator", "開啟翻譯視窗", true, None::<&str>)?,
                    &MenuItem::with_id(app, "live", "Live 即時翻譯", true, None::<&str>)?,
                    &MenuItem::with_id(app, "settings", "設定", true, None::<&str>)?,
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
                        open_settings(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing a window keeps ContextLens alive in the tray so hotkeys still work.
            if let WindowEvent::CloseRequested { api, .. } = event {
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
