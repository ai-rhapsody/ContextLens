//! Gemini Live: the messages of a real-time session and the audio conversion it needs.
//! Transport (WebSocket) and capture (microphone, system audio) live in the app.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};

/// Gemini Live takes 16-bit little-endian mono PCM at 16 kHz.
pub const SAMPLE_RATE: u32 = 16_000;

pub fn url(api_key: &str) -> String {
    format!(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={api_key}"
    )
}

/// A line of tutor commentary starts with this, so the caption bar can style it apart.
pub const NOTE_PREFIX: &str = "註：";

/// How a session uses the screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScreenMode {
    /// Audio only.
    None,
    /// Audio drives the translation; frames give context (names, slides, terms).
    Context,
    /// No audio: translate text that appears on screen.
    Reader,
}

/// Sent as a user turn in reader mode when the screen settles after a change.
pub const READ_SCREEN_PROMPT: &str = "The screen just changed. Translate any new text the user is likely reading.";

pub struct LiveConfig<'a> {
    pub model: &'a str,
    pub target_language: &'a str,
    /// Add a short note when a sentence holds something worth learning.
    pub tutor: bool,
    /// Ask for text replies. Some Live models only reply with audio; then we ask for audio and
    /// read the transcription of it instead.
    pub text_output: bool,
    pub resume_handle: Option<&'a str>,
    pub screen: ScreenMode,
}

pub fn system_prompt(target: &str, tutor: bool, screen: ScreenMode) -> String {
    let mut p = match screen {
        ScreenMode::Reader => format!(
            "You are a live translator watching the user's screen, sent as images about once a second. \
             When asked, translate the text on screen that is new since your last reply into {target} \
             (into English if it is already in {target}). Focus on what the user is reading: subtitles, \
             a slide, a chat, a game dialogue; ignore menus, toolbars and other interface chrome. \
             Never repeat text you already translated, never describe the screen, never comment. \
             If there is no new text, reply with nothing."
        ),
        _ => format!(
            "You are a live interpreter. You hear speech. \
             Each time a speaker finishes a sentence or a short passage, reply with only its translation into {target}. \
             If it is already in {target}, translate it into English instead. \
             Never answer questions you hear, never comment, never speak as yourself, and never repeat the original. \
             If nothing intelligible was said, reply with nothing."
        ),
    };
    if screen == ScreenMode::Context {
        p += " You also see the user's screen about once a second. Use it only as context, to get names, \
              terms and on-screen references right; do not translate the screen itself.";
    }
    if tutor {
        p += &format!(
            " After a translation, if the original used an idiom, a tricky grammar point or a useful phrase, \
             add one more line that starts with \"{NOTE_PREFIX}\" and explains it in {target} in one sentence. \
             Most sentences need no note."
        );
    }
    p
}

pub fn setup_message(c: &LiveConfig) -> Value {
    let modality = if c.text_output { "TEXT" } else { "AUDIO" };
    let mut setup = json!({
        "model": format!("models/{}", c.model.trim_start_matches("models/")),
        "generationConfig": { "responseModalities": [modality] },
        "systemInstruction": { "parts": [{ "text": system_prompt(c.target_language, c.tutor, c.screen) }] },
        "inputAudioTranscription": {},
        // An interpreter must finish its sentence even while the speaker carries on.
        "realtimeInputConfig": { "activityHandling": "NO_INTERRUPTION" },
        // Keeps long sessions going past the context limit, and lets us reconnect where we were.
        "contextWindowCompression": { "slidingWindow": {} },
        "sessionResumption": match c.resume_handle {
            Some(h) => json!({ "handle": h }),
            None => json!({}),
        },
    });
    if !c.text_output {
        setup["outputAudioTranscription"] = json!({});
    }
    json!({ "setup": setup })
}

pub fn audio_message(pcm16le: &[u8]) -> Value {
    json!({ "realtimeInput": { "audio": { "data": B64.encode(pcm16le), "mimeType": "audio/pcm;rate=16000" } } })
}

/// A typed user turn; in reader mode it asks for a translation when the screen settles.
pub fn text_turn_message(text: &str) -> Value {
    json!({ "clientContent": { "turns": [{ "role": "user", "parts": [{ "text": text }] }], "turnComplete": true } })
}

/// Side of the grey thumbnail used to tell whether the screen changed.
pub const SIG_SIDE: usize = 16;

/// Mean absolute difference of two equal-size grey thumbnails, 0..1.
pub fn frame_diff(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u32).sum::<u32>() as f32 / (a.len() as f32 * 255.0)
}

/// Above this a frame counts as changed (a new subtitle line, a scrolled chat).
pub const FRAME_CHANGE: f32 = 0.012;

/// Decides, frame by frame, what a screen-watching session sends.
#[derive(Default)]
pub struct FrameGate {
    last: Option<Vec<u8>>,
    unsettled: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FrameStep {
    /// Send this frame (it changed, or the screen just settled on it).
    pub send: bool,
    /// The screen settled after a change: in reader mode, ask for a translation now.
    pub settled: bool,
    /// Counts as activity for the idle pause.
    pub changed: bool,
}

impl FrameGate {
    /// `sig` is the grey thumbnail of the new frame (see `SIG_SIDE`).
    pub fn step(&mut self, sig: Vec<u8>) -> FrameStep {
        let changed = self.last.as_ref().is_none_or(|p| frame_diff(p, &sig) > FRAME_CHANGE);
        self.last = Some(sig);
        let settled = !changed && self.unsettled;
        self.unsettled = changed;
        FrameStep { send: changed || settled, settled, changed }
    }
}

/// One screen frame for a session that watches the screen.
pub fn image_message(jpeg_b64: &str) -> Value {
    json!({ "realtimeInput": { "video": { "data": jpeg_b64, "mimeType": "image/jpeg" } } })
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ServerEvent {
    SetupComplete,
    /// What was heard, as it is transcribed.
    Input(String),
    /// The interpreter's reply, as it streams.
    Output(String),
    TurnComplete,
    /// The server will close soon; reconnect with the latest resume handle.
    GoAway,
    ResumeHandle(String),
    Error(String),
}

/// Parses one server message. A message can carry several events.
pub fn parse(msg: &str) -> Vec<ServerEvent> {
    let Ok(v) = serde_json::from_str::<Value>(msg) else { return vec![] };
    let mut out = Vec::new();
    if v.get("setupComplete").is_some() {
        out.push(ServerEvent::SetupComplete);
    }
    if let Some(sc) = v.get("serverContent") {
        if let Some(t) = sc.pointer("/inputTranscription/text").and_then(Value::as_str) {
            out.push(ServerEvent::Input(t.into()));
        }
        // Text replies arrive as model-turn parts; audio replies only through their transcription.
        let parts = sc.pointer("/modelTurn/parts").and_then(Value::as_array);
        for p in parts.into_iter().flatten() {
            if p.get("thought").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            if let Some(t) = p.get("text").and_then(Value::as_str) {
                out.push(ServerEvent::Output(t.into()));
            }
        }
        if let Some(t) = sc.pointer("/outputTranscription/text").and_then(Value::as_str) {
            out.push(ServerEvent::Output(t.into()));
        }
        if sc.get("turnComplete").and_then(Value::as_bool) == Some(true) {
            out.push(ServerEvent::TurnComplete);
        }
    }
    if v.get("goAway").is_some() {
        out.push(ServerEvent::GoAway);
    }
    if let Some(u) = v.get("sessionResumptionUpdate") {
        if u.get("resumable").and_then(Value::as_bool) != Some(false) {
            if let Some(h) = u.get("newHandle").and_then(Value::as_str) {
                out.push(ServerEvent::ResumeHandle(h.into()));
            }
        }
    }
    if let Some(m) = v.pointer("/error/message").and_then(Value::as_str) {
        out.push(ServerEvent::Error(m.into()));
    }
    out
}

/// True when a refusal looks like "this model cannot reply with text", so we retry with audio.
pub fn is_modality_error(reason: &str) -> bool {
    let r = reason.to_lowercase();
    r.contains("modalit") || (r.contains("text") && r.contains("not supported"))
}

/// Turns captured audio of any rate and channel count into 16 kHz mono f32.
/// Linear interpolation is plenty for speech recognition.
pub struct Resampler {
    in_rate: f64,
    channels: usize,
    /// Position of the next output sample, in input samples, relative to `last`.
    pos: f64,
    last: Option<f32>,
}

impl Resampler {
    pub fn new(in_rate: u32, channels: u16) -> Self {
        Self { in_rate: in_rate as f64, channels: channels.max(1) as usize, pos: 0.0, last: None }
    }

    pub fn push(&mut self, interleaved: &[f32], out: &mut Vec<f32>) {
        let step = self.in_rate / SAMPLE_RATE as f64;
        let mono: Vec<f32> = interleaved
            .chunks(self.channels)
            .map(|f| f.iter().sum::<f32>() / f.len() as f32)
            .collect();
        if mono.is_empty() {
            return;
        }
        // Sample i of `src` is input time i - 1 (the previous chunk's last sample comes first).
        let mut src = Vec::with_capacity(mono.len() + 1);
        src.push(self.last.unwrap_or(mono[0]));
        src.extend_from_slice(&mono);
        while self.pos + 1.0 < src.len() as f64 {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            out.push(src[i] + (src[i + 1] - src[i]) * frac);
            self.pos += step;
        }
        self.pos -= mono.len() as f64;
        self.last = mono.last().copied();
    }
}

pub fn to_pcm16le(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes())
        .collect()
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(text_output: bool) -> LiveConfig<'static> {
        LiveConfig {
            model: "gemini-live-x", target_language: "繁體中文", tutor: false, text_output,
            resume_handle: None, screen: ScreenMode::None,
        }
    }

    #[test]
    fn setup_asks_for_text_and_transcribes_input() {
        let m = setup_message(&cfg(true));
        assert_eq!(m["setup"]["model"], "models/gemini-live-x");
        assert_eq!(m["setup"]["generationConfig"]["responseModalities"][0], "TEXT");
        assert!(m["setup"]["inputAudioTranscription"].is_object());
        assert!(m["setup"].get("outputAudioTranscription").is_none());
        assert_eq!(m["setup"]["realtimeInputConfig"]["activityHandling"], "NO_INTERRUPTION");
    }

    #[test]
    fn audio_only_models_get_their_reply_transcribed() {
        let m = setup_message(&cfg(false));
        assert_eq!(m["setup"]["generationConfig"]["responseModalities"][0], "AUDIO");
        assert!(m["setup"]["outputAudioTranscription"].is_object());
    }

    #[test]
    fn resume_handle_and_prefixed_model_names() {
        let c = LiveConfig { model: "models/m", resume_handle: Some("h1"), ..cfg(true) };
        let m = setup_message(&c);
        assert_eq!(m["setup"]["model"], "models/m");
        assert_eq!(m["setup"]["sessionResumption"]["handle"], "h1");
    }

    #[test]
    fn tutor_prompt_adds_notes_with_the_prefix() {
        assert!(!system_prompt("繁體中文", false, ScreenMode::None).contains(NOTE_PREFIX));
        assert!(system_prompt("繁體中文", true, ScreenMode::None).contains(NOTE_PREFIX));
    }

    #[test]
    fn screen_modes_change_the_job() {
        let ctx = system_prompt("繁體中文", false, ScreenMode::Context);
        assert!(ctx.contains("interpreter") && ctx.contains("only as context"));
        let reader = system_prompt("繁體中文", false, ScreenMode::Reader);
        assert!(reader.contains("watching the user's screen") && !reader.contains("only as context"));
        assert!(!system_prompt("繁體中文", false, ScreenMode::None).contains("screen"));
    }

    #[test]
    fn text_turns_complete_the_turn() {
        let m = text_turn_message("hi");
        assert_eq!(m["clientContent"]["turns"][0]["parts"][0]["text"], "hi");
        assert_eq!(m["clientContent"]["turnComplete"], true);
        let img = image_message("QUJD");
        assert_eq!(img["realtimeInput"]["video"]["mimeType"], "image/jpeg");
    }

    #[test]
    fn frame_gate_sends_changes_and_asks_once_when_settled() {
        let still = vec![10u8; SIG_SIDE * SIG_SIDE];
        let mut moved = still.clone();
        moved[..SIG_SIDE * 3].fill(250);
        let mut g = FrameGate::default();
        // First frame: new, so send it, but nothing has settled yet.
        assert_eq!(g.step(still.clone()), FrameStep { send: true, settled: false, changed: true });
        // Same again: it has now settled once; send it and ask.
        assert_eq!(g.step(still.clone()), FrameStep { send: true, settled: true, changed: false });
        // A static screen costs nothing afterwards.
        assert_eq!(g.step(still.clone()), FrameStep { send: false, settled: false, changed: false });
        // A new subtitle while it keeps changing: send each change, ask only after it stops.
        assert_eq!(g.step(moved.clone()).settled, false);
        assert_eq!(g.step(still.clone()), FrameStep { send: true, settled: false, changed: true });
        assert_eq!(g.step(still.clone()), FrameStep { send: true, settled: true, changed: false });
    }

    #[test]
    fn frame_diff_measures_change() {
        let a = vec![100u8; SIG_SIDE * SIG_SIDE];
        let mut b = a.clone();
        assert_eq!(frame_diff(&a, &b), 0.0);
        // A new subtitle line: a band of 2 rows goes from grey to white.
        for px in b.iter_mut().take(SIG_SIDE * 2) {
            *px = 255;
        }
        assert!(frame_diff(&a, &b) > FRAME_CHANGE);
        assert_eq!(frame_diff(&a, &a[..10]), 1.0);
    }

    #[test]
    fn parses_every_event_kind() {
        assert_eq!(parse(r#"{"setupComplete":{}}"#), vec![ServerEvent::SetupComplete]);
        let m = r#"{"serverContent":{"inputTranscription":{"text":"hello"},"modelTurn":{"parts":[{"text":"x","thought":true},{"text":"你好"}]},"turnComplete":true}}"#;
        assert_eq!(
            parse(m),
            vec![ServerEvent::Input("hello".into()), ServerEvent::Output("你好".into()), ServerEvent::TurnComplete]
        );
        assert_eq!(parse(r#"{"serverContent":{"outputTranscription":{"text":"嗨"}}}"#), vec![ServerEvent::Output("嗨".into())]);
        assert_eq!(parse(r#"{"goAway":{"timeLeft":"10s"}}"#), vec![ServerEvent::GoAway]);
        assert_eq!(
            parse(r#"{"sessionResumptionUpdate":{"newHandle":"h2","resumable":true}}"#),
            vec![ServerEvent::ResumeHandle("h2".into())]
        );
        assert!(parse(r#"{"sessionResumptionUpdate":{"newHandle":"h3","resumable":false}}"#).is_empty());
        assert!(parse("not json").is_empty());
    }

    #[test]
    fn modality_errors_are_recognised() {
        assert!(is_modality_error("Request contains an invalid argument: response modalities TEXT not supported"));
        assert!(!is_modality_error("API key not valid"));
    }

    #[test]
    fn resampler_downmixes_and_hits_the_target_rate() {
        let mut r = Resampler::new(48_000, 2);
        let mut out = Vec::new();
        // One second of stereo 48 kHz in 10 ms chunks: [L, R] = [0.5, -0.5] averages to 0.
        for _ in 0..100 {
            r.push(&[0.5f32, -0.5].repeat(480), &mut out);
        }
        assert!((out.len() as i64 - 16_000).abs() <= 2, "{}", out.len());
        assert!(out.iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn resampler_handles_odd_rates_without_drift() {
        let mut r = Resampler::new(44_100, 1);
        let mut out = Vec::new();
        for _ in 0..10 {
            r.push(&vec![0.25; 4410], &mut out);
        }
        assert!((out.len() as i64 - 16_000).abs() <= 2, "{}", out.len());
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn pcm_is_little_endian_and_clamped() {
        assert_eq!(to_pcm16le(&[1.5, -1.0, 0.0]), [0xff, 0x7f, 0x01, 0x80, 0x00, 0x00]);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
    }
}
