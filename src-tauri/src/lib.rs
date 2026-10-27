mod capture;
mod secrets;
mod store;

use serde::{Deserialize, Serialize};
use contextlens_core::{prompt, Action, ChatRequest, Provider, Settings};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use tauri::{
    ipc::Channel,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// What the hotkey grabbed, waiting for the popup to pick it up.
#[derive(Clone, Serialize)]
struct Capture {
    text: String,
    tutor: bool,
    /// Set when capturing failed; the popup shows it instead of translating.
    error: Option<String>,
}

struct AppState {
    settings: Mutex<Settings>,
    pending: Mutex<Option<Capture>>,
    /// Bumped for every translation; an older stream stops when it sees a newer number.
    generation: AtomicU64,
    http: contextlens_core::reqwest::Client,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StreamEvent {
    Delta { text: String },
    Done,
    Error { message: String },
}

// ---------- popup ----------

fn popup(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("popup")
}

fn place_near_cursor(app: &AppHandle, win: &WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else { return };
    let (mut x, mut y) = (cursor.x + 16.0, cursor.y + 16.0);
    if let (Ok(Some(mon)), Ok(size)) = (app.monitor_from_point(cursor.x, cursor.y), win.outer_size()) {
        let (mp, ms) = (mon.position(), mon.size());
        let max_x = mp.x as f64 + ms.width as f64 - size.width as f64 - 8.0;
        let max_y = mp.y as f64 + ms.height as f64 - size.height as f64 - 8.0;
        x = x.min(max_x).max(mp.x as f64 + 8.0);
        y = y.min(max_y).max(mp.y as f64 + 8.0);
    }
    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn show_popup(app: &AppHandle, capture: Capture) {
    let Some(win) = popup(app) else { return };
    *app.state::<AppState>().pending.lock().unwrap() = Some(capture);
    place_near_cursor(app, &win);
    let _ = win.show();
    let _ = win.set_focus();
    // The popup also calls take_pending_capture on mount, so a first show cannot be missed.
    let _ = win.emit("capture", ());
}

fn open_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

// ---------- hotkeys ----------

fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    for hk in &settings.hotkeys {
        let action = hk.action;
        gs.on_shortcut(hk.accelerator.as_str(), move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                trigger(app.clone(), action);
            }
        })
        .map_err(|e| format!("無法註冊熱鍵 {}：{e}", hk.accelerator))?;
    }
    Ok(())
}

fn trigger(app: AppHandle, action: Action) {
    tauri::async_runtime::spawn(async move {
        // Let the user's fingers leave the hotkey before we press Copy.
        tokio_sleep(150).await;
        let tutor = match action {
            Action::TutorSelection => true,
            Action::TranslateSelection => app.state::<AppState>().settings.lock().unwrap().tutor_mode,
        };
        let grabbed = tauri::async_runtime::spawn_blocking(capture::selected_text)
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        let capture = match grabbed {
            Ok(text) => Capture { text, tutor, error: None },
            Err(message) => Capture { text: String::new(), tutor, error: Some(message) },
        };
        show_popup(&app, capture);
    });
}

async fn tokio_sleep(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms))
    })
    .await;
}

// ---------- commands ----------

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    settings.validate()?;
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

#[tauri::command]
fn set_api_key(args: KeyArgs, key: String) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("金鑰不能是空的".into());
    }
    secrets::set(args.provider, &key)
}

#[tauri::command]
fn delete_api_key(args: KeyArgs) -> Result<(), String> {
    secrets::delete(args.provider)
}

/// Which providers have a key stored. The keys themselves never leave Rust.
#[tauri::command]
fn key_status() -> Vec<(Provider, bool)> {
    Provider::ALL.iter().map(|p| (*p, secrets::get(*p).is_some())).collect()
}

#[tauri::command]
fn take_pending_capture(state: State<AppState>) -> Option<Capture> {
    state.pending.lock().unwrap().take()
}

#[tauri::command]
fn hide_popup(app: AppHandle) {
    if let Some(w) = popup(&app) {
        let _ = w.hide();
    }
}

#[tauri::command]
fn show_settings(app: AppHandle) {
    open_settings(&app);
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
    let key = secrets::get(provider)
        .ok_or_else(|| format!("還沒設定 {} 的 API 金鑰。到「模型與金鑰」貼上金鑰。", provider.id()))?;
    contextlens_core::stream_completion(&state.http, provider, model, &key, req, |t| {
        gen.map_or(true, |g| state.generation.load(Ordering::SeqCst) == g) && emit(t)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_translation(
    state: State<'_, AppState>,
    text: String,
    tutor: bool,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let gen = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let (provider, model, lang) = {
        let s = state.settings.lock().unwrap();
        (s.provider, s.model_for(s.provider).to_string(), s.target_language.clone())
    };
    let mode = if tutor { prompt::Mode::Tutor } else { prompt::Mode::Translate };
    let req = prompt::build(mode, &text, &lang);

    let result = stream(&state, provider, &model, &req, Some(gen), |t| {
        on_event.send(StreamEvent::Delta { text: t.to_string() }).is_ok()
    })
    .await;
    // A newer request owns the popup now; stay silent.
    if state.generation.load(Ordering::SeqCst) != gen {
        return Ok(());
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
    let req = ChatRequest { system: "Reply with the single word OK.".into(), user: "ping".into() };
    stream(&state, args.provider, &model, &req, None, |_| true).await?;
    Ok(model)
}

// ---------- app ----------

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            set_api_key,
            delete_api_key,
            key_status,
            take_pending_capture,
            hide_popup,
            show_settings,
            run_translation,
            test_provider,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = store::load(&handle);
            app.manage(AppState {
                settings: Mutex::new(settings.clone()),
                pending: Mutex::new(None),
                generation: AtomicU64::new(0),
                http: contextlens_core::http_client(),
            });
            if let Err(e) = register_hotkeys(&handle, &settings) {
                eprintln!("hotkeys: {e}");
            }

            let menu = Menu::with_items(
                app,
                &[
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
            // Closing the settings window keeps ContextLens alive in the tray so hotkeys still work.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running ContextLens");
}
