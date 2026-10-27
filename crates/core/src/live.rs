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

pub struct LiveConfig<'a> {
    pub model: &'a str,
    pub target_language: &'a str,
    /// Add a short note when a sentence holds something worth learning.
    pub tutor: bool,
    /// Ask for text replies. Some Live models only reply with audio; then we ask for audio and
    /// read the transcription of it instead.
    pub text_output: bool,
    pub resume_handle: Option<&'a str>,
}

pub fn system_prompt(target: &str, tutor: bool) -> String {
    let mut p = format!(
        "You are a live interpreter. You hear speech, and sometimes see the user's screen. \
         Each time a speaker finishes a sentence or a short passage, reply with only its translation into {target}. \
         If it is already in {target}, translate it into English instead. \
         Never answer questions you hear, never comment, never speak as yourself, and never repeat the original. \
         If nothing intelligible was said, reply with nothing."
    );
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
        "systemInstruction": { "parts": [{ "text": system_prompt(c.target_language, c.tutor) }] },
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
        LiveConfig { model: "gemini-live-x", target_language: "繁體中文", tutor: false, text_output, resume_handle: None }
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
        assert!(!system_prompt("繁體中文", false).contains(NOTE_PREFIX));
        assert!(system_prompt("繁體中文", true).contains(NOTE_PREFIX));
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
