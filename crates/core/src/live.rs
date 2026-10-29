//! Real-time sessions (Gemini Live, OpenAI Realtime): their messages, and the audio conversion
//! they need. Transport (WebSocket) and capture (microphone, system audio) live in the app.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};

use crate::settings::Provider;

/// Gemini Live takes 16-bit little-endian mono PCM at 16 kHz.
pub const SAMPLE_RATE: u32 = 16_000;

/// The real-time API a session talks to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    GeminiLive,
    /// Gemini's interpreter model: translates continuously as people speak, audio in only,
    /// no instructions. Its replies are read from the transcription of its translated speech.
    GeminiTranslate,
    OpenAiRealtime,
}

/// Transcription model OpenAI uses for what the user said (low-latency streaming).
const OPENAI_TRANSCRIBE: &str = "gpt-live-transcribe";

impl Backend {
    pub fn for_provider(p: Provider) -> Option<Self> {
        match p {
            Provider::Gemini => Some(Self::GeminiLive),
            Provider::OpenAi => Some(Self::OpenAiRealtime),
            Provider::Anthropic => None,
        }
    }

    /// Input PCM rate the API expects.
    pub fn sample_rate(self) -> u32 {
        match self {
            Self::GeminiLive | Self::GeminiTranslate => 16_000,
            Self::OpenAiRealtime => 24_000,
        }
    }

    /// Whether the server hands out handles to resume a session where it left off.
    pub fn resumable(self) -> bool {
        self == Self::GeminiLive
    }

    /// WebSocket URL and headers.
    pub fn endpoint(self, api_key: &str, model: &str) -> (String, Vec<(&'static str, String)>) {
        match self {
            Self::GeminiLive | Self::GeminiTranslate => (url(api_key), vec![]),
            Self::OpenAiRealtime => (
                format!("wss://api.openai.com/v1/realtime?model={}", model.trim()),
                vec![("Authorization", format!("Bearer {api_key}"))],
            ),
        }
    }

    /// Messages sent right after connecting.
    pub fn opening(self, c: &LiveConfig) -> Vec<Value> {
        match self {
            Self::GeminiLive => vec![setup_message(c)],
            Self::GeminiTranslate => vec![translate_setup_message(c)],
            Self::OpenAiRealtime => vec![json!({
                "type": "session.update",
                "session": {
                    "type": "realtime",
                    "model": c.model,
                    "output_modalities": ["text"],
                    "instructions": system_prompt(c.target_language, c.tutor, c.screen) + c.glossary,
                    "audio": { "input": {
                        "format": { "type": "audio/pcm", "rate": 24_000 },
                        "transcription": { "model": OPENAI_TRANSCRIBE },
                        // Reply after each pause, and finish a reply even if the speaker carries on.
                        "turn_detection": { "type": "server_vad", "silence_duration_ms": 500,
                                            "create_response": true, "interrupt_response": false },
                    } },
                },
            })],
        }
    }

    /// Whether the app marks speech start and end itself (Gemini) instead of the server's VAD.
    pub fn manual_activity(self) -> bool {
        self == Self::GeminiLive
    }

    pub fn activity(self, a: Activity) -> Vec<Value> {
        match (self, a) {
            (Self::GeminiLive, Activity::Start) => vec![json!({ "realtimeInput": { "activityStart": {} } })],
            (Self::GeminiLive, Activity::End) => vec![json!({ "realtimeInput": { "activityEnd": {} } })],
            (Self::GeminiTranslate | Self::OpenAiRealtime, _) => vec![],
        }
    }

    pub fn audio(self, pcm16le: &[u8]) -> Vec<Value> {
        match self {
            Self::GeminiLive | Self::GeminiTranslate => vec![audio_message(pcm16le)],
            Self::OpenAiRealtime => vec![json!({ "type": "input_audio_buffer.append", "audio": B64.encode(pcm16le) })],
        }
    }

    pub fn frame(self, jpeg_b64: &str) -> Vec<Value> {
        match self {
            Self::GeminiLive => vec![image_message(jpeg_b64)],
            Self::GeminiTranslate => vec![],
            Self::OpenAiRealtime => vec![json!({
                "type": "conversation.item.create",
                "item": { "type": "message", "role": "user", "content": [
                    { "type": "input_image", "image_url": format!("data:image/jpeg;base64,{jpeg_b64}") }
                ] },
            })],
        }
    }

    pub fn turn(self, text: &str) -> Vec<Value> {
        match self {
            Self::GeminiLive => vec![text_turn_message(text)],
            Self::GeminiTranslate => vec![],
            Self::OpenAiRealtime => vec![
                json!({ "type": "conversation.item.create", "item": { "type": "message", "role": "user",
                        "content": [{ "type": "input_text", "text": text }] } }),
                json!({ "type": "response.create" }),
            ],
        }
    }

    pub fn parse(self, msg: &str) -> Vec<ServerEvent> {
        match self {
            Self::GeminiLive | Self::GeminiTranslate => parse(msg),
            Self::OpenAiRealtime => parse_openai(msg),
        }
    }
}

/// OpenAI Realtime server events. Accepts both the GA and the older beta event names.
pub fn parse_openai(msg: &str) -> Vec<ServerEvent> {
    let Ok(v) = serde_json::from_str::<Value>(msg) else { return vec![] };
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    match v.get("type").and_then(Value::as_str).unwrap_or_default() {
        "session.updated" => vec![ServerEvent::SetupComplete],
        "conversation.item.input_audio_transcription.completed" => {
            let t = s("transcript");
            if t.trim().is_empty() { vec![] } else { vec![ServerEvent::Input(t)] }
        }
        "response.output_text.delta" | "response.text.delta" => vec![ServerEvent::Output(s("delta"))],
        "response.done" => {
            let failed = v.pointer("/response/status").and_then(Value::as_str) == Some("failed");
            match v.pointer("/response/status_details/error/message").and_then(Value::as_str) {
                Some(m) if failed => vec![ServerEvent::Error(m.into())],
                _ => vec![ServerEvent::TurnComplete],
            }
        }
        "error" => vec![ServerEvent::Error(
            v.pointer("/error/message").and_then(Value::as_str).map_or_else(|| crate::t!("OpenAI Realtime 錯誤"), String::from),
        )],
        _ => vec![],
    }
}

pub fn url(api_key: &str) -> String {
    format!(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={api_key}"
    )
}

/// A line of tutor commentary starts with this, so the caption bar can style it apart. It is
/// in the interface language; the caption bar knows every language's version.
pub fn note_prefix() -> String {
    crate::t!("註：")
}

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
    /// The app marks speech start and end (`Segmenter`) instead of the server's detection.
    pub manual_activity: bool,
    /// The user's fixed renderings (`prompt::glossary_note`), added to the instructions.
    /// The interpreter model takes no instructions, so it goes without.
    pub glossary: &'a str,
}

pub fn system_prompt(target: &str, tutor: bool, screen: ScreenMode) -> String {
    let mut p = match screen {
        ScreenMode::Reader => format!(
            "You are a live translator watching the user's screen, sent as images about once a second. \
             When asked, translate the text on screen that is new since your last reply into {target}. \
             Text already in {target} needs no translation; skip it. Focus on what the user is reading: subtitles, \
             a slide, a chat, a game dialogue; ignore menus, toolbars and other interface chrome. \
             Never repeat text you already translated, never describe the screen, never comment. \
             If there is no new text, reply with nothing."
        ),
        _ => format!(
            "You are a live interpreter. You hear speech. \
             Each time a speaker finishes a sentence or a short passage, reply with only its translation into {target}. \
             Translate every sentence of the passage, not just the last one. \
             Keep names and titles (people, groups, songs, albums, shows) as they are said, in their \
             original spelling, instead of guessing a translation. \
             Speech mixing in English words or abbreviations is still translated into {target}; \
             speech that is already in {target} needs no reply. \
             Write plain text without Markdown. \
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
             add one more line that starts with \"{prefix}\" and explains it in {target} in one sentence. \
             Most sentences need no note.",
            prefix = note_prefix()
        );
    }
    p
}

pub fn setup_message(c: &LiveConfig) -> Value {
    let modality = if c.text_output { "TEXT" } else { "AUDIO" };
    let mut setup = json!({
        "model": format!("models/{}", c.model.trim_start_matches("models/")),
        "generationConfig": { "responseModalities": [modality] },
        "systemInstruction": { "parts": [{ "text": system_prompt(c.target_language, c.tutor, c.screen) + c.glossary }] },
        "inputAudioTranscription": {},
        // The app marks where each passage starts and ends (see `Segmenter`): the server only
        // ends a turn at a pause, so a speaker who never pauses would never be translated.
        // An interpreter must also finish its sentence even while the speaker carries on.
        "realtimeInputConfig": if c.manual_activity {
            json!({ "automaticActivityDetection": { "disabled": true }, "activityHandling": "NO_INTERRUPTION" })
        } else {
            // Fallback: the server's detection, ending a passage at shorter pauses than its default.
            json!({
                "automaticActivityDetection": { "endOfSpeechSensitivity": "END_SENSITIVITY_HIGH", "silenceDurationMs": 400 },
                "activityHandling": "NO_INTERRUPTION",
            })
        },
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

/// Setup for the interpreter model. It takes no system instruction: the target language and
/// whether to repeat speech already in it are all it is told.
pub fn translate_setup_message(c: &LiveConfig) -> Value {
    // The transcription switches belong at the setup level; inside generationConfig (as the
    // documentation's sample has them) the API rejects the setup.
    json!({ "setup": {
        "model": format!("models/{}", c.model.trim_start_matches("models/")),
        "inputAudioTranscription": {},
        "outputAudioTranscription": {},
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "translationConfig": {
                "targetLanguageCode": language_code(c.target_language).unwrap_or("en"),
                "echoTargetLanguage": false,
            },
        },
    } })
}

/// Whether an interpreter caption line is done and the next speech starts a new one: it ends
/// in a sentence end, or has grown long enough to read on its own.
pub fn caption_break(output: &str) -> bool {
    let t = output.trim_end();
    t.ends_with(['。', '！', '？', '.', '!', '?', '…']) || t.chars().count() >= 60
}

/// The BCP-47 code for the user's language, as the interpreter model wants it. Names the
/// settings page offers are mapped; a value that already looks like a code is used as is.
pub fn language_code(target: &str) -> Option<&str> {
    let t = target.trim();
    let named = match t {
        // Script subtags, as the documentation uses; in a test zh-TW once mistranslated a time.
        "繁體中文" | "正體中文" | "中文（繁體）" => "zh-Hant",
        "简体中文" | "簡體中文" | "中文（简体）" => "zh-Hans",
        "中文" => "zh",
        "English" | "英文" | "英語" => "en",
        "日本語" | "日文" => "ja",
        "한국어" | "韓文" => "ko",
        "Español" => "es",
        "Français" => "fr",
        "Deutsch" => "de",
        "Italiano" => "it",
        "Português" => "pt",
        "Русский" => "ru",
        "Tiếng Việt" => "vi",
        "ภาษาไทย" => "th",
        "Bahasa Indonesia" => "id",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named);
    }
    let looks_like_code = (2..=8).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    looks_like_code.then_some(t)
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

/// Whether a refusal before setup completed is worth one retry asking for audio replies.
/// Native-audio models (Gemini 3.8 Live) reply only with audio and reject a text request, not
/// always with a message that names the modality; errors that audio cannot fix are excluded.
pub fn should_retry_with_audio(reason: &str) -> bool {
    let r = reason.to_lowercase();
    let unfixable = ["not found", "api key", "permission", "quota", "billing", "unauthenticated", "location"];
    !unfixable.iter().any(|u| r.contains(u))
}

/// Whether an error names the reply modality, e.g. "The requested combination of response
/// modalities (TEXT) is not supported by the model". Gemini can send this after setup
/// completed, when the first input arrives, so it is checked on any failure.
pub fn mentions_modality(reason: &str) -> bool {
    let r = reason.to_lowercase();
    r.contains("modalit") || (r.contains("text") && r.contains("not supported"))
}

/// The model rejected the app marking speech itself; use the server's detection instead.
pub fn rejects_manual_activity(reason: &str) -> bool {
    let r = reason.to_lowercase();
    r.contains("activity") && !r.contains("modalit")
}

/// A resume handle the server no longer knows (it expired, or belonged to a session set up
/// differently). Reconnect without it.
pub fn is_stale_session(reason: &str) -> bool {
    let r = reason.to_lowercase();
    r.contains("session not found") || (r.contains("session") && r.contains("expired"))
}

/// Adds what to do to the errors a user can fix in settings.
pub fn explain_error(reason: &str, model: &str) -> String {
    let r = reason.to_lowercase();
    if r.contains("session not found") || r.contains("expired") {
        crate::t!("即時連線已過期，請再按一次「開始」。")
    } else if (r.contains("models/") && r.contains("not found")) || r.contains("is not supported for bidigeneratecontent") {
        crate::t!(
            "找不到即時模型「{model}」，或它不支援 Live。到「模型與金鑰」從清單選一個即時模型。（{reason}）",
            model = model,
            reason = reason
        )
    } else if r.contains("api key") || r.contains("unauthenticated") || r.contains("401") {
        crate::t!("API 金鑰無效，到「模型與金鑰」重新貼上。（{reason}）", reason = reason)
    } else {
        reason.to_string()
    }
}

/// Turns captured audio of any rate and channel count into mono f32 at the API's rate.
/// Linear interpolation is plenty for speech recognition.
pub struct Resampler {
    in_rate: f64,
    out_rate: f64,
    channels: usize,
    /// Position of the next output sample, in input samples, relative to `last`.
    pos: f64,
    last: Option<f32>,
}

impl Resampler {
    /// To 16 kHz (Gemini Live).
    pub fn new(in_rate: u32, channels: u16) -> Self {
        Self::to_rate(in_rate, channels, SAMPLE_RATE)
    }

    pub fn to_rate(in_rate: u32, channels: u16, out_rate: u32) -> Self {
        Self {
            in_rate: in_rate as f64,
            out_rate: out_rate as f64,
            channels: channels.max(1) as usize,
            pos: 0.0,
            last: None,
        }
    }

    pub fn push(&mut self, interleaved: &[f32], out: &mut Vec<f32>) {
        let step = self.in_rate / self.out_rate;
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Activity {
    Start,
    End,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Seg {
    Activity(Activity),
    Audio(Vec<u8>),
}

/// Splits continuous audio into passages for a session whose app marks speech itself.
/// A passage ends at a pause; one that runs on is cut at a quiet moment after `SOFT_MS`,
/// and at `HARD_MS` regardless, so long monologues and speech over music get translated
/// piece by piece instead of all at once (or not at all) at the end.
pub struct Segmenter {
    rate: u32,
    speaking: bool,
    /// Audio kept while idle, so the first syllable is not lost when speech starts.
    preroll: std::collections::VecDeque<Vec<u8>>,
    preroll_ms: u32,
    passage_ms: u32,
    silence_ms: u32,
    /// Typical loudness of the current passage, to find its quieter moments.
    loud: f32,
}

impl Segmenter {
    pub const SPEECH_RMS: f32 = 0.012;
    pub const PAUSE_MS: u32 = 500;
    pub const SOFT_MS: u32 = 5_000;
    pub const HARD_MS: u32 = 8_000;
    const PREROLL_MS: u32 = 300;

    pub fn new(rate: u32) -> Self {
        Self {
            rate, speaking: false, preroll: Default::default(), preroll_ms: 0,
            passage_ms: 0, silence_ms: 0, loud: 0.0,
        }
    }

    /// No audio has arrived for `ms` (a loopback source that went silent sends nothing):
    /// a passage in progress ends here.
    pub fn gap(&mut self, ms: u32) -> Vec<Seg> {
        if self.speaking && ms >= Self::PAUSE_MS {
            self.speaking = false;
            return vec![Seg::Activity(Activity::End)];
        }
        vec![]
    }

    /// `pcm` is 16-bit little-endian mono at the session rate.
    pub fn push(&mut self, pcm: Vec<u8>) -> Vec<Seg> {
        let ms = (pcm.len() as u64 * 1000 / (self.rate as u64 * 2)) as u32;
        let level = rms_pcm16le(&pcm);
        let mut out = Vec::new();
        if !self.speaking {
            if level < Self::SPEECH_RMS {
                self.preroll_ms += ms;
                self.preroll.push_back(pcm);
                while self.preroll_ms > Self::PREROLL_MS && self.preroll.len() > 1 {
                    let old = self.preroll.pop_front().unwrap();
                    self.preroll_ms -= (old.len() as u64 * 1000 / (self.rate as u64 * 2)) as u32;
                }
                return out;
            }
            self.speaking = true;
            self.passage_ms = 0;
            self.silence_ms = 0;
            self.loud = level;
            out.push(Seg::Activity(Activity::Start));
            out.extend(self.preroll.drain(..).map(Seg::Audio));
            self.preroll_ms = 0;
        }
        self.passage_ms += ms;
        let quiet = level < Self::SPEECH_RMS;
        self.silence_ms = if quiet { self.silence_ms + ms } else { 0 };
        if !quiet {
            self.loud = self.loud * 0.9 + level * 0.1;
        }
        out.push(Seg::Audio(pcm));
        if self.silence_ms >= Self::PAUSE_MS {
            self.speaking = false;
            out.push(Seg::Activity(Activity::End));
        } else if self.passage_ms >= Self::HARD_MS || (self.passage_ms >= Self::SOFT_MS && level < self.loud * 0.5) {
            // Cut and carry straight on with the next passage.
            out.push(Seg::Activity(Activity::End));
            out.push(Seg::Activity(Activity::Start));
            self.passage_ms = 0;
            self.silence_ms = 0;
        }
        out
    }
}

fn rms_pcm16le(pcm: &[u8]) -> f32 {
    let n = pcm.len() / 2;
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = pcm
        .chunks_exact(2)
        .map(|b| {
            let v = i16::from_le_bytes([b[0], b[1]]) as f64 / i16::MAX as f64;
            v * v
        })
        .sum();
    (sum / n as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, ms: u32, amp: f32) -> Vec<u8> {
        let n = (rate * ms / 1000) as usize;
        to_pcm16le(&(0..n).map(|i| amp * (i as f32 * 0.3).sin()).collect::<Vec<_>>())
    }

    fn acts(segs: &[Seg]) -> Vec<Activity> {
        segs.iter().filter_map(|s| match s { Seg::Activity(a) => Some(*a), _ => None }).collect()
    }

    #[test]
    fn segmenter_marks_a_sentence_between_pauses() {
        let mut s = Segmenter::new(16_000);
        let mut all = vec![];
        for _ in 0..5 { all.extend(s.push(tone(16_000, 100, 0.0))); }
        assert!(all.is_empty(), "silence alone sends nothing");
        for _ in 0..20 { all.extend(s.push(tone(16_000, 100, 0.3))); }
        for _ in 0..6 { all.extend(s.push(tone(16_000, 100, 0.0))); }
        assert_eq!(acts(&all), vec![Activity::Start, Activity::End]);
        // The quiet lead-in goes with the passage so its first syllable is kept.
        assert!(matches!(all[1], Seg::Audio(_)) && matches!(all[2], Seg::Audio(_)));
    }

    #[test]
    fn segmenter_ends_a_passage_when_the_audio_stops_arriving() {
        let mut s = Segmenter::new(16_000);
        let mut all = vec![];
        for _ in 0..20 { all.extend(s.push(tone(16_000, 100, 0.3))); }
        assert!(s.gap(200).is_empty());
        all.extend(s.gap(600));
        assert_eq!(acts(&all), vec![Activity::Start, Activity::End]);
        assert!(s.gap(2000).is_empty(), "ends once");
    }

    #[test]
    fn segmenter_cuts_speech_that_never_pauses() {
        let mut s = Segmenter::new(16_000);
        let mut all = vec![];
        for _ in 0..170 { all.extend(s.push(tone(16_000, 100, 0.3))); }
        // 17 s of unbroken speech: cut at 8 s and 16 s.
        assert_eq!(acts(&all), vec![Activity::Start, Activity::End, Activity::Start, Activity::End, Activity::Start]);
    }

    #[test]
    fn segmenter_prefers_a_quieter_moment_after_five_seconds() {
        let mut s = Segmenter::new(16_000);
        let mut all = vec![];
        for _ in 0..55 { all.extend(s.push(tone(16_000, 100, 0.3))); }
        all.extend(s.push(tone(16_000, 100, 0.05)));
        assert_eq!(acts(&all), vec![Activity::Start, Activity::End, Activity::Start]);
    }

    #[test]
    fn interpreter_captions_break_at_sentence_ends() {
        assert!(!caption_break(""));
        assert!(!caption_break("我們今天要"));
        assert!(caption_break("我們今天要談定價。"));
        assert!(caption_break(&"長".repeat(60)));
    }

    #[test]
    fn interpreter_setup_names_the_target_language_and_nothing_else() {
        let c = LiveConfig { model: "gemini-3.5-live-translate-preview", ..cfg(false) };
        let m = &Backend::GeminiTranslate.opening(&c)[0]["setup"];
        assert_eq!(m["model"], "models/gemini-3.5-live-translate-preview");
        assert_eq!(m["generationConfig"]["translationConfig"]["targetLanguageCode"], "zh-Hant");
        assert!(m.get("inputAudioTranscription").is_some() && m.get("outputAudioTranscription").is_some());
        assert!(m["generationConfig"].get("inputAudioTranscription").is_none(), "rejected inside generationConfig");
        assert_eq!(m["generationConfig"]["responseModalities"][0], "AUDIO");
        assert!(m.get("systemInstruction").is_none());
        assert!(Backend::GeminiTranslate.turn("x").is_empty() && Backend::GeminiTranslate.frame("x").is_empty());
        assert!(!Backend::GeminiTranslate.manual_activity());
        assert_eq!(language_code("简体中文"), Some("zh-Hans"));
        assert_eq!(language_code("pt-BR"), Some("pt-BR"));
        assert_eq!(language_code("台語"), None);
    }

    #[test]
    fn gemini_setup_leaves_speech_marking_to_the_app() {
        let m = setup_message(&cfg(true));
        assert_eq!(m["setup"]["realtimeInputConfig"]["automaticActivityDetection"]["disabled"], true);
        assert!(Backend::GeminiLive.manual_activity() && !Backend::OpenAiRealtime.manual_activity());
    }

    #[test]
    fn stale_sessions_are_not_called_a_missing_model() {
        let r = "session not found or expired";
        assert!(is_stale_session(r));
        assert!(!explain_error(r, "gemini-3.8-live").contains("找不到即時模型"));
        assert!(explain_error("models/gemini-x is not found for API version v1beta", "gemini-x").contains("找不到即時模型"));
    }

    fn cfg(text_output: bool) -> LiveConfig<'static> {
        LiveConfig {
            model: "gemini-3.8-live", target_language: "繁體中文", tutor: false, text_output,
            resume_handle: None, screen: ScreenMode::None, manual_activity: true, glossary: "",
        }
    }

    #[test]
    fn setup_asks_for_text_and_transcribes_input() {
        let m = setup_message(&cfg(true));
        assert_eq!(m["setup"]["model"], "models/gemini-3.8-live");
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
        assert!(!system_prompt("繁體中文", false, ScreenMode::None).contains(&note_prefix()));
        assert!(system_prompt("繁體中文", true, ScreenMode::None).contains(&note_prefix()));
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
    fn retries_with_audio_unless_audio_cannot_help() {
        assert!(should_retry_with_audio("Request contains an invalid argument."));
        assert!(should_retry_with_audio("response modalities TEXT not supported"));
        assert!(!should_retry_with_audio("API key not valid"));
        assert!(!should_retry_with_audio(
            "models/gemini-3.8.live is not found for API version v1beta, or is not supported for bidiGenerateContent."
        ));
    }

    #[test]
    fn modality_errors_after_setup_are_recognised() {
        assert!(mentions_modality(
            "The requested combination of response modalities (TEXT) is not supported by the model. models/gemini_api_beyond_live_rev25_"
        ));
        assert!(!mentions_modality("Internal error encountered."));
    }

    #[test]
    fn errors_say_what_to_do() {
        let e = explain_error("models/gemini-3.8.live is not found for API version v1beta", "gemini-3.8.live");
        assert!(e.contains("gemini-3.8.live") && e.contains("模型與金鑰"));
        assert!(explain_error("API key not valid", "m").contains("金鑰"));
        assert_eq!(explain_error("something", "m"), "something");
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
    fn resampler_targets_other_rates() {
        let mut r = Resampler::to_rate(48_000, 1, 24_000);
        let mut out = Vec::new();
        r.push(&vec![0.1; 48_000], &mut out);
        assert!((out.len() as i64 - 24_000).abs() <= 2, "{}", out.len());
    }

    #[test]
    fn openai_session_asks_for_text_at_24khz_with_transcription() {
        let c = cfg(true);
        let (url, headers) = Backend::OpenAiRealtime.endpoint("K", "gpt-realtime");
        assert!(url.ends_with("/v1/realtime?model=gpt-realtime"));
        assert_eq!(headers[0], ("Authorization", "Bearer K".to_string()));
        let m = &Backend::OpenAiRealtime.opening(&c)[0];
        assert_eq!(m["type"], "session.update");
        assert_eq!(m["session"]["output_modalities"][0], "text");
        assert_eq!(m["session"]["audio"]["input"]["format"]["rate"], 24_000);
        assert_eq!(m["session"]["audio"]["input"]["turn_detection"]["interrupt_response"], false);
        assert_eq!(Backend::OpenAiRealtime.sample_rate(), 24_000);
        assert_eq!(Backend::OpenAiRealtime.audio(&[1, 0])[0]["type"], "input_audio_buffer.append");
        let t = Backend::OpenAiRealtime.turn("hi");
        assert_eq!(t[1]["type"], "response.create");
        // Gemini keeps its key in the URL and needs no headers.
        assert!(Backend::GeminiLive.endpoint("K", "m").1.is_empty());
        assert!(Backend::for_provider(Provider::Anthropic).is_none());
    }

    #[test]
    fn openai_events_map_to_the_shared_ones() {
        assert_eq!(parse_openai(r#"{"type":"session.updated"}"#), vec![ServerEvent::SetupComplete]);
        assert_eq!(
            parse_openai(r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello"}"#),
            vec![ServerEvent::Input("hello".into())]
        );
        assert_eq!(parse_openai(r#"{"type":"response.output_text.delta","delta":"你"}"#), vec![ServerEvent::Output("你".into())]);
        assert_eq!(parse_openai(r#"{"type":"response.text.delta","delta":"好"}"#), vec![ServerEvent::Output("好".into())]);
        assert_eq!(parse_openai(r#"{"type":"response.done","response":{"status":"completed"}}"#), vec![ServerEvent::TurnComplete]);
        assert_eq!(
            parse_openai(r#"{"type":"response.done","response":{"status":"failed","status_details":{"error":{"message":"quota"}}}}"#),
            vec![ServerEvent::Error("quota".into())]
        );
        assert_eq!(parse_openai(r#"{"type":"error","error":{"message":"bad model"}}"#), vec![ServerEvent::Error("bad model".into())]);
        assert!(parse_openai(r#"{"type":"input_audio_buffer.speech_started"}"#).is_empty());
    }

    #[test]
    fn pcm_is_little_endian_and_clamped() {
        assert_eq!(to_pcm16le(&[1.5, -1.0, 0.0]), [0xff, 0x7f, 0x01, 0x80, 0x00, 0x00]);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
    }
}
