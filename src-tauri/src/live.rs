//! Live sessions: capture audio, mix it, stream it to Gemini Live, and relay what comes back
//! to the caption window. Capture runs on plain threads (audio streams cannot move between
//! threads on every platform); the WebSocket session runs on the async runtime.

use crate::{store, AppState};
use contextlens_core::live::{self, LiveConfig, Resampler, ServerEvent};
use contextlens_core::{HistoryEntry, Provider};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LiveSource {
    Mic,
    /// What the speakers play (WASAPI loopback). Windows only for now.
    System,
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

pub struct LiveHandle {
    /// Tells this session apart from a newer one that replaced it.
    pub id: u64,
    stop: watch::Sender<bool>,
    capture_stop: Arc<AtomicBool>,
    pub sources: Vec<LiveSource>,
    pub tutor: bool,
}

impl LiveHandle {
    pub fn stop(&self) {
        self.capture_stop.store(true, Ordering::SeqCst);
        let _ = self.stop.send(true);
    }
}

pub struct StartArgs {
    pub sources: Vec<LiveSource>,
    pub tutor: bool,
    pub model: String,
    pub target_language: String,
    pub api_key: String,
    pub idle_minutes: u32,
}

fn emit(app: &AppHandle, e: LiveUi) {
    let _ = app.emit_to("live", "live", e);
}

fn status(app: &AppHandle, state: &'static str, message: Option<String>) {
    emit(app, LiveUi::Status { state, message });
}

// ---------- capture ----------

/// Builds an input stream that hands 16 kHz mono f32 chunks to `tx`, tagged with `idx`.
fn build_stream(
    source: LiveSource,
    idx: usize,
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
    };
    let mut rs = Resampler::new(config.sample_rate(), config.channels());
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
    tx: std_mpsc::Sender<(usize, Vec<f32>)>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let (ready_tx, ready_rx) = std_mpsc::channel();
    std::thread::spawn(move || match build_stream(source, idx, tx) {
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

/// 100 ms at 16 kHz.
const CHUNK: usize = 1600;
/// Below this loudness a chunk counts as silence.
const SILENCE_RMS: f32 = 0.01;

/// Sums the sources into 100 ms PCM chunks for the session, reports levels, and calls
/// `on_idle` once after `idle` without sound.
fn spawn_mixer(
    sources: usize,
    rx: std_mpsc::Receiver<(usize, Vec<f32>)>,
    out: mpsc::Sender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    idle: Option<Duration>,
    app: AppHandle,
    on_idle: impl FnOnce() + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut bufs: Vec<VecDeque<f32>> = vec![VecDeque::new(); sources];
        let mut last_sound = Instant::now();
        let mut last_level = Instant::now();
        let mut on_idle = Some(on_idle);
        while !stop.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok((i, s)) => bufs[i].extend(s),
                Err(std_mpsc::RecvTimeoutError::Timeout) => {}
                Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
            }
            // A source that plays nothing (loopback during silence) delivers no data at all,
            // so mix whenever any source has a full chunk.
            while bufs.iter().any(|b| b.len() >= CHUNK) {
                let mut mix = vec![0f32; CHUNK];
                for b in bufs.iter_mut() {
                    let n = b.len().min(CHUNK);
                    for (m, s) in mix.iter_mut().zip(b.drain(..n)) {
                        *m += s;
                    }
                }
                let level = live::rms(&mix);
                if level > SILENCE_RMS {
                    last_sound = Instant::now();
                }
                if last_level.elapsed() >= Duration::from_millis(150) {
                    emit(&app, LiveUi::Level { value: (level * 4.0).min(1.0) });
                    last_level = Instant::now();
                }
                if out.blocking_send(live::to_pcm16le(&mix)).is_err() {
                    return;
                }
            }
            if let Some(limit) = idle {
                if last_sound.elapsed() >= limit {
                    if let Some(f) = on_idle.take() {
                        f();
                    }
                }
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
    url: &str,
    setup: serde_json::Value,
    audio: &mut mpsc::Receiver<Vec<u8>>,
    stop: &mut watch::Receiver<bool>,
    on: &mut (impl FnMut(ServerEvent) + Send),
) -> Outcome {
    let (mut ws, _) = match tokio_tungstenite::connect_async(url).await {
        Ok(c) => c,
        Err(e) => return Outcome::Refused(format!("無法連線到 Gemini Live：{e}")),
    };
    if let Err(e) = ws.send(Message::text(setup.to_string())).await {
        return Outcome::Refused(e.to_string());
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
                return Outcome::Refused("Gemini Live 沒有回應。".into());
            }
            chunk = audio.recv(), if started => {
                let Some(pcm) = chunk else { return Outcome::Stopped };
                if let Err(e) = ws.send(Message::text(live::audio_message(&pcm).to_string())).await {
                    return Outcome::Failed(e.to_string());
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
                for ev in live::parse(&text) {
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

fn save(app: &AppHandle, model: &str, tutor: bool, mut t: Transcript) {
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
        provider: Provider::Gemini,
        model: model.into(),
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
    let (pcm_tx, mut pcm_rx) = mpsc::channel::<Vec<u8>>(64);
    let (cap_tx, cap_rx) = std_mpsc::channel();

    for (i, s) in args.sources.iter().enumerate() {
        if let Err(e) = spawn_capture(*s, i, cap_tx.clone(), capture_stop.clone()) {
            capture_stop.store(true, Ordering::SeqCst);
            return Err(e);
        }
    }
    drop(cap_tx);

    let idle = (args.idle_minutes > 0).then(|| Duration::from_secs(args.idle_minutes as u64 * 60));
    let idle_app = app.clone();
    let idle_stop = stop_tx.clone();
    let idle_capture = capture_stop.clone();
    spawn_mixer(args.sources.len(), cap_rx, pcm_tx, capture_stop.clone(), idle, app.clone(), move || {
        let mins = idle.map_or(0, |d| d.as_secs() / 60);
        status(&idle_app, "paused", Some(format!("{mins} 分鐘沒有聲音，已自動暫停以節省費用。")));
        idle_capture.store(true, Ordering::SeqCst);
        let _ = idle_stop.send(true);
    });

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let handle = LiveHandle {
        id,
        stop: stop_tx,
        capture_stop: capture_stop.clone(),
        sources: args.sources.clone(),
        tutor: args.tutor,
    };

    tauri::async_runtime::spawn(async move {
        let url = live::url(&args.api_key);
        let mut resume: Option<String> = None;
        let mut text_output = true;
        let mut transcript = Transcript::default();
        let mut reconnects = 0u32;
        status(&app, "connecting", None);
        loop {
            let setup = live::setup_message(&LiveConfig {
                model: &args.model,
                target_language: &args.target_language,
                tutor: args.tutor,
                text_output,
                resume_handle: resume.as_deref(),
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
                    // New speech after a finished reply starts a new turn.
                    if !transcript.output.is_empty() {
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
            match session(&url, setup, &mut pcm_rx, &mut stop_rx, &mut on).await {
                Outcome::Stopped => break,
                Outcome::Reconnect if reconnects < 20 => {
                    reconnects += 1;
                    status(&app, "reconnecting", None);
                }
                Outcome::Refused(r) if text_output && live::is_modality_error(&r) => {
                    // This model only speaks; ask for audio and read its transcription.
                    text_output = false;
                }
                Outcome::Refused(r) | Outcome::Failed(r) => {
                    status(&app, "error", Some(r));
                    break;
                }
                Outcome::Reconnect => {
                    status(&app, "error", Some("連線一直中斷，已停止。".into()));
                    break;
                }
            }
        }
        capture_stop.store(true, Ordering::SeqCst);
        save(&app, &args.model, args.tutor, transcript);
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
        })
    }

    #[tokio::test]
    async fn streams_audio_and_relays_the_reply() {
        let url = fake_server(r#"{"setupComplete":{}}"#).await;
        let (tx, mut rx) = mpsc::channel(4);
        let (stop_tx, mut stop) = watch::channel(false);
        tx.send(vec![0u8; 3200]).await.unwrap();
        let mut events = Vec::new();
        let mut on = |e: ServerEvent| {
            if e == ServerEvent::TurnComplete {
                let _ = stop_tx.send(true);
            }
            events.push(e);
        };
        let run = session(&url, setup(), &mut rx, &mut stop, &mut on);
        assert!(matches!(tokio::time::timeout(Duration::from_secs(5), run).await.unwrap(), Outcome::Stopped));
        drop(on);
        assert!(events.contains(&ServerEvent::Input("hello".into())));
        assert!(events.contains(&ServerEvent::Output("你好".into())));
    }

    #[tokio::test]
    async fn an_error_before_setup_is_a_refusal_with_the_reason() {
        let url = fake_server(r#"{"error":{"message":"API key not valid"}}"#).await;
        let (_tx, mut rx) = mpsc::channel(1);
        let (_stop_tx, mut stop) = watch::channel(false);
        let mut on = |_: ServerEvent| {};
        let run = session(&url, setup(), &mut rx, &mut stop, &mut on);
        match tokio::time::timeout(Duration::from_secs(5), run).await.unwrap() {
            Outcome::Refused(r) => assert!(r.contains("API key")),
            _ => panic!("expected a refusal"),
        }
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
