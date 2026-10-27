//! Live sessions: capture audio and the screen, stream them to a real-time API (Gemini Live or
//! OpenAI Realtime) or, for the screen alone, to a regular vision model, and relay what comes
//! back to the caption window. Capture runs on plain threads (audio streams cannot move between
//! threads on every platform); the network side runs on the async runtime.

use crate::{imaging, store, AppState};
use contextlens_core::live::{self, Backend, LiveConfig, Resampler, ScreenMode, ServerEvent};
use contextlens_core::{ChatRequest, Delta, HistoryEntry, Media, Provider, Task};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LiveSource {
    Mic,
    /// What the speakers play (WASAPI loopback). Windows only for now.
    System,
    /// A region, window or monitor, sent as frames.
    Screen,
}

impl LiveSource {
    fn is_audio(self) -> bool {
        self != LiveSource::Screen
    }
}

/// The part of the screen a Live session watches, in physical pixels relative to its monitor.
#[derive(Serialize, Clone, Debug)]
pub struct ScreenTarget {
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Shown in the caption bar: a window title, "全螢幕" or the region size.
    pub label: String,
}

/// What the session sends.
pub enum Outgoing {
    Audio(Vec<u8>),
    /// A JPEG frame, base64.
    Frame(String),
    /// A typed user turn.
    Turn(&'static str),
}

/// Milliseconds since `epoch` of the last sound or screen change; drives the idle pause.
fn touch(activity: &AtomicU64, epoch: Instant) {
    activity.store(epoch.elapsed().as_millis() as u64, Ordering::SeqCst);
}

pub const SYSTEM_AUDIO_SUPPORTED: bool = cfg!(target_os = "windows");

/// Events for the caption window.
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveUi {
    /// `connecting`, `listening`, `reconnecting`, `paused`, `stopped` or `error`.
    Status { state: &'static str, message: Option<String> },
    Input { text: String },
    Output { text: String },
    TurnComplete,
    /// Loudness of the last 100 ms, 0..1, for the level meter.
    Level { value: f32 },
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Gemini models that refused text replies; later sessions ask them for audio straight away.
static AUDIO_ONLY: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn audio_only(model: &str) -> bool {
    AUDIO_ONLY.lock().unwrap().iter().any(|m| m == model)
}

fn mark_audio_only(model: &str) {
    let mut list = AUDIO_ONLY.lock().unwrap();
    if !list.iter().any(|m| m == model) {
        list.push(model.to_string());
    }
}

pub struct LiveHandle {
    /// Tells this session apart from a newer one that replaced it.
    pub id: u64,
    stop: watch::Sender<bool>,
    capture_stop: Arc<AtomicBool>,
    pub sources: Vec<LiveSource>,
    pub tutor: bool,
    /// "Gemini · gemini-live-…", shown in the caption bar.
    pub label: String,
}

impl LiveHandle {
    pub fn stop(&self) {
        self.capture_stop.store(true, Ordering::SeqCst);
        let _ = self.stop.send(true);
    }
}

/// Where a session's content goes.
pub enum Engine {
    /// A real-time API, needed for audio.
    Realtime { backend: Backend, model: String, api_key: String },
    /// Screen only: each settled screen goes to a regular vision model. Works with any provider
    /// whose model reads images, and costs only when the screen changes.
    Reader { provider: Provider, model: String, api_key: String },
}

impl Engine {
    fn provider(&self) -> Provider {
        match self {
            Engine::Realtime { backend: Backend::GeminiLive, .. } => Provider::Gemini,
            Engine::Realtime { backend: Backend::OpenAiRealtime, .. } => Provider::OpenAi,
            Engine::Reader { provider, .. } => *provider,
        }
    }

    fn model(&self) -> &str {
        match self {
            Engine::Realtime { model, .. } | Engine::Reader { model, .. } => model,
        }
    }
}

pub struct StartArgs {
    pub sources: Vec<LiveSource>,
    pub tutor: bool,
    pub engine: Engine,
    pub target_language: String,
    pub idle_minutes: u32,
    pub screen: Option<ScreenTarget>,
}

fn emit(app: &AppHandle, e: LiveUi) {
    let _ = app.emit_to("live", "live", e);
}

fn status(app: &AppHandle, state: &'static str, message: Option<String>) {
    emit(app, LiveUi::Status { state, message });
}

// ---------- capture ----------

/// Builds an input stream that hands mono f32 chunks at `rate` to `tx`, tagged with `idx`.
fn build_stream(
    source: LiveSource,
    idx: usize,
    rate: u32,
    tx: std_mpsc::Sender<(usize, Vec<f32>)>,
) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let (device, config) = match source {
        LiveSource::Mic => {
            let d = host.default_input_device().ok_or("找不到麥克風。")?;
            let c = d.default_input_config().map_err(|e| format!("無法開啟麥克風：{e}"))?;
            (d, c)
        }
        LiveSource::System => {
            if !SYSTEM_AUDIO_SUPPORTED {
                return Err("系統聲音目前只支援 Windows。".into());
            }
            // On WASAPI an input stream on an output device records what it plays.
            let d = host.default_output_device().ok_or("找不到播放裝置。")?;
            let c = d.default_output_config().map_err(|e| format!("無法錄製系統聲音：{e}"))?;
            (d, c)
        }
        LiveSource::Screen => return Err("螢幕不是聲音來源。".into()),
    };
    let mut rs = Resampler::to_rate(config.sample_rate(), config.channels(), rate);
    let err_fn = |e| eprintln!("live capture: {e}");
    let mut send = move |samples: &[f32]| {
        let mut out = Vec::with_capacity(samples.len() / 2);
        rs.push(samples, &mut out);
        if !out.is_empty() {
            let _ = tx.send((idx, out));
        }
    };
    let cfg = config.config();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(cfg, move |d: &[f32], _: &_| send(d), err_fn, None),
        cpal::SampleFormat::I16 => device.build_input_stream(
            cfg,
            move |d: &[i16], _: &_| send(&d.iter().map(|s| *s as f32 / 32768.0).collect::<Vec<_>>()),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream(
            cfg,
            move |d: &[i32], _: &_| send(&d.iter().map(|s| *s as f32 / 2147483648.0).collect::<Vec<_>>()),
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            cfg,
            move |d: &[u16], _: &_| send(&d.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect::<Vec<_>>()),
            err_fn,
            None,
        ),
        other => return Err(format!("不支援的音訊格式：{other:?}")),
    }
    .map_err(|e| format!("無法開始錄音：{e}"))?;
    stream.play().map_err(|e| format!("無法開始錄音：{e}"))?;
    Ok(stream)
}

/// One thread per source keeps its stream alive until `stop`. Reports whether it started.
fn spawn_capture(
    source: LiveSource,
    idx: usize,
    rate: u32,
    tx: std_mpsc::Sender<(usize, Vec<f32>)>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let (ready_tx, ready_rx) = std_mpsc::channel();
    std::thread::spawn(move || match build_stream(source, idx, rate, tx) {
        Ok(stream) => {
            let _ = ready_tx.send(Ok(()));
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }
            drop(stream);
        }
        Err(e) => {
            let _ = ready_tx.send(Err(e));
        }
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "音訊裝置沒有回應。".to_string())?
}


/// Below this loudness a chunk counts as silence.
const SILENCE_RMS: f32 = 0.01;

/// Sums the audio sources into 100 ms PCM chunks for the session and reports levels.
fn spawn_mixer(
    sources: usize,
    rate: u32,
    rx: std_mpsc::Receiver<(usize, Vec<f32>)>,
    out: mpsc::Sender<Outgoing>,
    stop: Arc<AtomicBool>,
    activity: Arc<AtomicU64>,
    epoch: Instant,
    app: AppHandle,
) {
    // 100 ms per chunk.
    let chunk = (rate / 10) as usize;
    std::thread::spawn(move || {
        let mut bufs: Vec<VecDeque<f32>> = vec![VecDeque::new(); sources];
        let mut last_level = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok((i, s)) => bufs[i].extend(s),
                Err(std_mpsc::RecvTimeoutError::Timeout) => {}
                Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
            }
            // A source that plays nothing (loopback during silence) delivers no data at all,
            // so mix whenever any source has a full chunk.
            while bufs.iter().any(|b| b.len() >= chunk) {
                let mut mix = vec![0f32; chunk];
                for b in bufs.iter_mut() {
                    let n = b.len().min(chunk);
                    for (m, s) in mix.iter_mut().zip(b.drain(..n)) {
                        *m += s;
                    }
                }
                let level = live::rms(&mix);
                if level > SILENCE_RMS {
                    touch(&activity, epoch);
                }
                if last_level.elapsed() >= Duration::from_millis(150) {
                    emit(&app, LiveUi::Level { value: (level * 4.0).min(1.0) });
                    last_level = Instant::now();
                }
                if out.blocking_send(Outgoing::Audio(live::to_pcm16le(&mix))).is_err() {
                    return;
                }
            }
        }
    });
}

/// Longest side of a frame sent to Live; enough to read subtitles and slides.
const FRAME_SIDE: u32 = 1024;
const FRAME_EVERY: Duration = Duration::from_millis(1000);

/// Captures the target about once a second and sends only frames that changed. In reader mode,
/// once the screen settles after a change it also asks for a translation of the new text.
fn spawn_screen(
    target: ScreenTarget,
    reader: bool,
    out: mpsc::Sender<Outgoing>,
    stop: Arc<AtomicBool>,
    activity: Arc<AtomicU64>,
    epoch: Instant,
) -> Result<(), String> {
    // The monitor handle is not Send on Windows, so it is looked up inside the thread.
    let (ready_tx, ready_rx) = std_mpsc::channel();
    std::thread::spawn(move || {
        let monitor = match xcap::Monitor::from_point(target.monitor_x + 1, target.monitor_y + 1) {
            Ok(m) => {
                let _ = ready_tx.send(Ok(()));
                m
            }
            Err(e) => {
                let _ = ready_tx.send(Err(format!("找不到要觀看的螢幕：{e}")));
                return;
            }
        };
        let mut gate = live::FrameGate::default();
        while !stop.load(Ordering::SeqCst) {
            let started = Instant::now();
            if let Ok(img) = monitor.capture_region(target.x, target.y, target.w, target.h) {
                let img = image::DynamicImage::ImageRgba8(img);
                let side = live::SIG_SIDE as u32;
                let sig = img.resize_exact(side, side, image::imageops::FilterType::Triangle).to_luma8().into_raw();
                let step = gate.step(sig);
                if step.send {
                    let small = if img.width().max(img.height()) > FRAME_SIDE {
                        img.resize(FRAME_SIDE, FRAME_SIDE, image::imageops::FilterType::Triangle)
                    } else {
                        img
                    };
                    if let Ok(jpeg) = imaging::jpeg_b64(&small, 70) {
                        if out.blocking_send(Outgoing::Frame(jpeg)).is_err() {
                            return;
                        }
                    }
                }
                if step.changed {
                    touch(&activity, epoch);
                }
                if reader && step.settled && out.blocking_send(Outgoing::Turn(live::READ_SCREEN_PROMPT)).is_err() {
                    return;
                }
            }
            if let Some(rest) = FRAME_EVERY.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
        }
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "螢幕擷取沒有回應。".to_string())?
}

/// Calls `on_idle` once when nothing was heard or changed on screen for `limit`.
fn spawn_idle_watch(
    limit: Duration,
    stop: Arc<AtomicBool>,
    activity: Arc<AtomicU64>,
    epoch: Instant,
    on_idle: impl FnOnce() + Send + 'static,
) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(2));
            let quiet = epoch.elapsed().saturating_sub(Duration::from_millis(activity.load(Ordering::SeqCst)));
            if quiet >= limit {
                on_idle();
                return;
            }
        }
    });
}

// ---------- session ----------

enum Outcome {
    Stopped,
    /// The server is closing (goAway or a normal close); continue with the resume handle.
    Reconnect,
    /// Refused before the session started, e.g. a bad key or an unsupported model.
    Refused(String),
    Failed(String),
}

async fn session(
    backend: Backend,
    endpoint: (String, Vec<(&'static str, String)>),
    opening: Vec<serde_json::Value>,
    outgoing: &mut mpsc::Receiver<Outgoing>,
    stop: &mut watch::Receiver<bool>,
    on: &mut (impl FnMut(ServerEvent) + Send),
) -> Outcome {
    let (url, headers) = endpoint;
    let mut request = match url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => return Outcome::Refused(e.to_string()),
    };
    for (k, v) in headers {
        if let Ok(v) = v.parse() {
            request.headers_mut().insert(k, v);
        }
    }
    let (mut ws, _) = match tokio_tungstenite::connect_async(request).await {
        Ok(c) => c,
        Err(e) => return Outcome::Refused(format!("無法連線：{e}")),
    };
    for m in opening {
        if let Err(e) = ws.send(Message::text(m.to_string())).await {
            return Outcome::Refused(e.to_string());
        }
    }
    let mut started = false;
    let setup_deadline = tokio::time::sleep(Duration::from_secs(15));
    tokio::pin!(setup_deadline);
    loop {
        tokio::select! {
            _ = stop.changed() => {
                let _ = ws.close(None).await;
                return Outcome::Stopped;
            }
            _ = &mut setup_deadline, if !started => {
                return Outcome::Refused("即時服務沒有回應。".into());
            }
            next = outgoing.recv(), if started => {
                let msgs = match next {
                    Some(Outgoing::Audio(pcm)) => backend.audio(&pcm),
                    Some(Outgoing::Frame(jpeg)) => backend.frame(&jpeg),
                    Some(Outgoing::Turn(text)) => backend.turn(text),
                    None => return Outcome::Stopped,
                };
                for m in msgs {
                    if let Err(e) = ws.send(Message::text(m.to_string())).await {
                        return Outcome::Failed(e.to_string());
                    }
                }
            }
            msg = ws.next() => {
                let text = match msg {
                    Some(Ok(Message::Text(t))) => t.to_string(),
                    // Gemini sends its JSON in binary frames.
                    Some(Ok(Message::Binary(b))) => String::from_utf8_lossy(&b).into_owned(),
                    Some(Ok(Message::Close(frame))) => {
                        let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                        return if !started {
                            Outcome::Refused(if reason.is_empty() { "連線被拒絕。".into() } else { reason })
                        } else if reason.is_empty() {
                            Outcome::Reconnect
                        } else {
                            Outcome::Failed(reason)
                        };
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return if started { Outcome::Reconnect } else { Outcome::Refused(e.to_string()) },
                    None => return if started { Outcome::Reconnect } else { Outcome::Refused("連線中斷。".into()) },
                };
                for ev in backend.parse(&text) {
                    match ev {
                        ServerEvent::SetupComplete => {
                            started = true;
                            on(ServerEvent::SetupComplete);
                        }
                        ServerEvent::GoAway => {
                            let _ = ws.close(None).await;
                            return Outcome::Reconnect;
                        }
                        ServerEvent::Error(m) => return if started { Outcome::Failed(m) } else { Outcome::Refused(m) },
                        other => on(other),
                    }
                }
            }
        }
    }
}

/// Screen-only sessions: keeps the latest frame and, each time the screen settles, asks a
/// regular vision model for the new text. Recent replies go back in, so it does not repeat itself.
#[allow(clippy::too_many_arguments)]
async fn reader(
    app: &AppHandle,
    provider: Provider,
    model: &str,
    api_key: &str,
    target: &str,
    tutor: bool,
    outgoing: &mut mpsc::Receiver<Outgoing>,
    stop: &mut watch::Receiver<bool>,
    transcript: &mut Transcript,
) {
    let http = contextlens_core::http_client();
    let mut frame: Option<String> = None;
    let mut recent: VecDeque<String> = VecDeque::new();
    status(app, "listening", None);
    loop {
        let next = tokio::select! {
            _ = stop.changed() => return,
            n = outgoing.recv() => n,
        };
        match next {
            None => return,
            Some(Outgoing::Frame(f)) => frame = Some(f),
            Some(Outgoing::Audio(_)) => {}
            Some(Outgoing::Turn(_)) => {
                let Some(f) = frame.clone() else { continue };
                let mut system = live::system_prompt(target, tutor, ScreenMode::Reader);
                if !recent.is_empty() {
                    let done: Vec<&str> = recent.iter().map(String::as_str).collect();
                    system += &format!("\nYou already translated this; do not repeat it:\n{}", done.join("\n"));
                }
                let req = ChatRequest {
                    system,
                    user: "Here is the screen now. Translate only the new text.".into(),
                    media: vec![Media { mime: "image/jpeg".into(), data_b64: f, name: None }],
                    web_search: false,
                    quick: true,
                    turns: vec![],
                };
                let mut out = String::new();
                let stopped = stop.clone();
                let result = contextlens_core::stream_completion(&http, provider, model, api_key, &req, |d| {
                    if let Delta::Text(t) = d {
                        out.push_str(&t);
                        emit(app, LiveUi::Output { text: t });
                    }
                    !*stopped.borrow()
                })
                .await;
                match result {
                    Ok(()) if !out.trim().is_empty() => {
                        emit(app, LiveUi::TurnComplete);
                        transcript.output = out.clone();
                        transcript.end_turn();
                        recent.push_back(out);
                        if recent.len() > 4 {
                            recent.pop_front();
                        }
                    }
                    Ok(()) => {}
                    // Keep watching: one failed request (rate limit, network) should not end the session.
                    Err(e) => status(app, "listening", Some(format!("這次沒有翻譯成功：{e}"))),
                }
            }
        }
    }
}

/// What was said and how it was translated, turn by turn, for history.
#[derive(Default)]
struct Transcript {
    turns: Vec<(String, String)>,
    input: String,
    output: String,
}

impl Transcript {
    fn end_turn(&mut self) {
        let (i, o) = (self.input.trim().to_string(), self.output.trim().to_string());
        if !i.is_empty() || !o.is_empty() {
            self.turns.push((i, o));
        }
        self.input.clear();
        self.output.clear();
    }
}

fn save(app: &AppHandle, provider: Provider, model: &str, tutor: bool, mut t: Transcript) {
    t.end_turn();
    if t.turns.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    if !state.settings.lock().unwrap().save_history {
        return;
    }
    let source: String = t.turns.iter().map(|(i, _)| i.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    let original = t.turns.iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>().join("\n");
    let translation = t.turns.iter().map(|(_, o)| o.as_str()).collect::<Vec<_>>().join("\n");
    let entry = HistoryEntry {
        at_ms: crate::now_ms(),
        kind: "live".into(),
        source: source.chars().take(4000).collect(),
        thumb: None,
        result: format!("## 原文\n{original}\n## 翻譯\n{translation}\n"),
        tutor,
        provider,
        model: model.into(),
        sources: vec![],
        thread: vec![],
        task: Some(Task::Translate),
    };
    let mut h = state.history.lock().unwrap();
    h.push(entry);
    let _ = store::save_history(app, &h);
    drop(h);
    let _ = app.emit_to("main", "history-changed", ());
}

pub fn start(app: AppHandle, args: StartArgs) -> Result<LiveHandle, String> {
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let capture_stop = Arc::new(AtomicBool::new(false));
    let (out_tx, mut out_rx) = mpsc::channel::<Outgoing>(64);
    let (cap_tx, cap_rx) = std_mpsc::channel();
    let epoch = Instant::now();
    let activity = Arc::new(AtomicU64::new(0));
    let fail = |e: String| {
        capture_stop.store(true, Ordering::SeqCst);
        Err(e)
    };

    let audio: Vec<LiveSource> = args.sources.iter().copied().filter(|s| s.is_audio()).collect();
    let rate = match &args.engine {
        Engine::Realtime { backend, .. } => backend.sample_rate(),
        Engine::Reader { .. } if !audio.is_empty() => return fail("聲音需要即時模型。".into()),
        Engine::Reader { .. } => live::SAMPLE_RATE,
    };
    for (i, s) in audio.iter().enumerate() {
        if let Err(e) = spawn_capture(*s, i, rate, cap_tx.clone(), capture_stop.clone()) {
            return fail(e);
        }
    }
    drop(cap_tx);
    if !audio.is_empty() {
        spawn_mixer(audio.len(), rate, cap_rx, out_tx.clone(), capture_stop.clone(), activity.clone(), epoch, app.clone());
    }

    let screen_mode = match (args.sources.contains(&LiveSource::Screen), audio.is_empty()) {
        (false, _) => ScreenMode::None,
        (true, false) => ScreenMode::Context,
        (true, true) => ScreenMode::Reader,
    };
    if screen_mode != ScreenMode::None {
        let Some(target) = args.screen.clone() else {
            return fail("先選擇要讓 Live 看的螢幕範圍。".into());
        };
        let reader = screen_mode == ScreenMode::Reader;
        if let Err(e) = spawn_screen(target, reader, out_tx.clone(), capture_stop.clone(), activity.clone(), epoch) {
            return fail(e);
        }
    }
    drop(out_tx);

    if args.idle_minutes > 0 {
        let mins = args.idle_minutes;
        let (idle_app, idle_stop, idle_capture) = (app.clone(), stop_tx.clone(), capture_stop.clone());
        spawn_idle_watch(Duration::from_secs(mins as u64 * 60), capture_stop.clone(), activity, epoch, move || {
            let what = if audio.is_empty() { "畫面沒有變化" } else { "沒有聲音" };
            status(&idle_app, "paused", Some(format!("{mins} 分鐘{what}，已自動暫停以節省費用。")));
            idle_capture.store(true, Ordering::SeqCst);
            let _ = idle_stop.send(true);
        });
    }

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let handle = LiveHandle {
        id,
        stop: stop_tx,
        capture_stop: capture_stop.clone(),
        sources: args.sources.clone(),
        tutor: args.tutor,
        label: format!("{} · {}", args.engine.provider().display_name(), args.engine.model()),
    };

    tauri::async_runtime::spawn(async move {
        let mut transcript = Transcript::default();
        status(&app, "connecting", None);
        match &args.engine {
            Engine::Reader { provider, model, api_key } => {
                reader(&app, *provider, model, api_key, &args.target_language, args.tutor,
                       &mut out_rx, &mut stop_rx, &mut transcript).await;
            }
            Engine::Realtime { backend, model, api_key } => {
                let backend = *backend;
                let mut resume: Option<String> = None;
                let mut text_output = !audio_only(model);
                let mut reconnects = 0u32;
                loop {
                    let opening = backend.opening(&LiveConfig {
                        model,
                        target_language: &args.target_language,
                        tutor: args.tutor,
                        text_output,
                        resume_handle: resume.as_deref(),
                        screen: screen_mode,
                    });
                    let mut announced = false;
                    let ui = app.clone();
                    let mut on = |ev: ServerEvent| match ev {
                        ServerEvent::SetupComplete => {
                            if !announced {
                                announced = true;
                                status(&ui, "listening", None);
                            }
                        }
                        ServerEvent::Input(t) => {
                            // Gemini transcribes the next speech as it comes, so new input after a
                            // finished reply starts a new turn. OpenAI sends each transcript whole,
                            // sometimes after the reply has begun, so it belongs to the current turn.
                            if backend == Backend::GeminiLive && !transcript.output.is_empty() {
                                transcript.end_turn();
                                emit(&ui, LiveUi::TurnComplete);
                            }
                            transcript.input.push_str(&t);
                            emit(&ui, LiveUi::Input { text: t });
                        }
                        ServerEvent::Output(t) => {
                            transcript.output.push_str(&t);
                            emit(&ui, LiveUi::Output { text: t });
                        }
                        ServerEvent::TurnComplete => {
                            transcript.end_turn();
                            emit(&ui, LiveUi::TurnComplete);
                        }
                        ServerEvent::ResumeHandle(h) => resume = Some(h),
                        ServerEvent::GoAway | ServerEvent::Error(_) => {}
                    };
                    let endpoint = backend.endpoint(api_key, model);
                    match session(backend, endpoint, opening, &mut out_rx, &mut stop_rx, &mut on).await {
                        Outcome::Stopped => break,
                        Outcome::Reconnect if reconnects < 20 => {
                            reconnects += 1;
                            if !backend.resumable() {
                                resume = None;
                            }
                            status(&app, "reconnecting", None);
                        }
                        // Native-audio models only speak: ask for audio and read its transcription.
                        // They may refuse before setup completes or only once input arrives.
                        Outcome::Refused(r)
                            if backend == Backend::GeminiLive && text_output && live::should_retry_with_audio(&r) =>
                        {
                            text_output = false;
                            mark_audio_only(model);
                        }
                        Outcome::Failed(r)
                            if backend == Backend::GeminiLive && text_output && live::mentions_modality(&r) =>
                        {
                            text_output = false;
                            mark_audio_only(model);
                            status(&app, "reconnecting", None);
                        }
                        Outcome::Refused(r) | Outcome::Failed(r) => {
                            status(&app, "error", Some(live::explain_error(&r, model)));
                            break;
                        }
                        Outcome::Reconnect => {
                            status(&app, "error", Some("連線一直中斷，已停止。".into()));
                            break;
                        }
                    }
                }
            }
        }
        capture_stop.store(true, Ordering::SeqCst);
        save(&app, args.engine.provider(), args.engine.model(), args.tutor, transcript);
        let state = app.state::<AppState>();
        let mut current = state.live.lock().unwrap();
        // A restart (new sources) may already have put a newer session in the slot.
        if current.as_ref().is_some_and(|h| h.id == id) {
            *current = None;
            drop(current);
            let _ = app.emit_to("live", "live-ended", ());
        }
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    /// A fake Gemini Live server: checks the setup, answers, reads one audio chunk, replies.
    async fn fake_server(first_reply: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(sock).await.unwrap();
            let setup = ws.next().await.unwrap().unwrap().into_text().unwrap();
            assert!(setup.contains("\"setup\""));
            ws.send(Message::binary(first_reply.as_bytes().to_vec())).await.unwrap();
            if first_reply.contains("setupComplete") {
                let audio = ws.next().await.unwrap().unwrap().into_text().unwrap();
                assert!(audio.contains("audio/pcm;rate=16000"));
                let reply = r#"{"serverContent":{"inputTranscription":{"text":"hello"},"modelTurn":{"parts":[{"text":"你好"}]},"turnComplete":true}}"#;
                ws.send(Message::binary(reply.as_bytes().to_vec())).await.unwrap();
                let _ = ws.next().await; // wait for the client to close
            }
        });
        format!("ws://{addr}/")
    }

    fn setup() -> serde_json::Value {
        live::setup_message(&LiveConfig {
            model: "m", target_language: "繁體中文", tutor: false, text_output: true, resume_handle: None,
            screen: ScreenMode::None,
        })
    }

    #[tokio::test]
    async fn streams_audio_and_relays_the_reply() {
        let url = fake_server(r#"{"setupComplete":{}}"#).await;
        let (tx, mut rx) = mpsc::channel(4);
        let (stop_tx, mut stop) = watch::channel(false);
        tx.send(Outgoing::Audio(vec![0u8; 3200])).await.unwrap();
        let mut events = Vec::new();
        let mut on = |e: ServerEvent| {
            if e == ServerEvent::TurnComplete {
                let _ = stop_tx.send(true);
            }
            events.push(e);
        };
        let run = session(Backend::GeminiLive, (url, vec![]), vec![setup()], &mut rx, &mut stop, &mut on);
        assert!(matches!(tokio::time::timeout(Duration::from_secs(5), run).await.unwrap(), Outcome::Stopped));
        drop(on);
        assert!(events.contains(&ServerEvent::Input("hello".into())));
        assert!(events.contains(&ServerEvent::Output("你好".into())));
    }

    #[tokio::test]
    async fn an_error_before_setup_is_a_refusal_with_the_reason() {
        let url = fake_server(r#"{"error":{"message":"API key not valid"}}"#).await;
        let (_tx, mut rx) = mpsc::channel::<Outgoing>(1);
        let (_stop_tx, mut stop) = watch::channel(false);
        let mut on = |_: ServerEvent| {};
        let run = session(Backend::GeminiLive, (url, vec![]), vec![setup()], &mut rx, &mut stop, &mut on);
        match tokio::time::timeout(Duration::from_secs(5), run).await.unwrap() {
            Outcome::Refused(r) => assert!(r.contains("API key")),
            _ => panic!("expected a refusal"),
        }
    }

    #[tokio::test]
    async fn a_text_refusal_after_setup_is_a_failure_that_names_the_modality() {
        use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(sock).await.unwrap();
            let _setup = ws.next().await;
            ws.send(Message::binary(br#"{"setupComplete":{}}"#.to_vec())).await.unwrap();
            let _audio = ws.next().await;
            // What Gemini 3.8 Live does with a TEXT request.
            let reason = "The requested combination of response modalities (TEXT) is not supported by the model.";
            let _ = ws.send(Message::Close(Some(CloseFrame { code: CloseCode::Policy, reason: reason.into() }))).await;
        });
        let (tx, mut rx) = mpsc::channel(4);
        let (_stop_tx, mut stop) = watch::channel(false);
        tx.send(Outgoing::Audio(vec![0u8; 320])).await.unwrap();
        let mut on = |_: ServerEvent| {};
        let run = session(Backend::GeminiLive, (url, vec![]), vec![setup()], &mut rx, &mut stop, &mut on);
        match tokio::time::timeout(Duration::from_secs(5), run).await.unwrap() {
            Outcome::Failed(r) => assert!(live::mentions_modality(&r), "{r}"),
            _ => panic!("expected a failure after setup"),
        }
    }

    #[test]
    fn audio_only_models_are_remembered() {
        assert!(!audio_only("gemini-test-live"));
        mark_audio_only("gemini-test-live");
        mark_audio_only("gemini-test-live");
        assert!(audio_only("gemini-test-live"));
        assert_eq!(AUDIO_ONLY.lock().unwrap().iter().filter(|m| *m == "gemini-test-live").count(), 1);
    }

    #[test]
    fn transcript_pairs_input_with_output() {
        let mut t = Transcript::default();
        t.input.push_str("hello ");
        t.output.push_str("你好");
        t.end_turn();
        t.end_turn(); // empty turns are dropped
        assert_eq!(t.turns, vec![("hello".to_string(), "你好".to_string())]);
    }
}
