use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gemini,
    OpenAi,
    Anthropic,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Gemini, Provider::OpenAi, Provider::Anthropic];

    /// Stable id, used as the keychain account name.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Gemini => "gemini",
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Provider::Gemini => "Gemini",
            Provider::OpenAi => "ChatGPT",
            Provider::Anthropic => "Claude",
        }
    }
}

/// Where a hotkey takes its content from.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The text selected in any app.
    Selection,
    /// A region, window or screen picked on a screenshot overlay.
    Screen,
    /// The image on the clipboard (e.g. after Win+Shift+S).
    ClipboardImage,
    /// Starts or stops a Live session.
    Live,
}

/// What to do with the captured content.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    /// Read the words and translate them (optionally with tutor notes).
    #[default]
    Translate,
    /// Explain what it means: a chart's finding, a diagram, a meme, code, a maths problem.
    Explain,
    /// Identify it and look it up on the web.
    Search,
}

/// What a Live session listens to or watches.
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
    pub fn is_audio(self) -> bool {
        self != LiveSource::Screen
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(from = "HotkeyWire")]
pub struct Hotkey {
    pub id: String,
    /// Accelerator string, e.g. `CommandOrControl+Shift+T`.
    pub accelerator: String,
    pub source: Source,
    pub task: Task,
    /// Explain structure, usage and background as well as translating.
    pub tutor: bool,
    /// For a Live hotkey: what to start with. Empty means whatever was used last.
    pub live_sources: Vec<LiveSource>,
}

/// Accepts the current shape and the 0.1 shape (`action: translate_selection | tutor_selection`).
#[derive(Deserialize)]
struct HotkeyWire {
    id: String,
    accelerator: String,
    source: Option<Source>,
    task: Option<Task>,
    tutor: Option<bool>,
    action: Option<String>,
    #[serde(default)]
    live_sources: Vec<LiveSource>,
}

impl From<HotkeyWire> for Hotkey {
    fn from(w: HotkeyWire) -> Self {
        let legacy_tutor = w.action.as_deref() == Some("tutor_selection");
        Hotkey {
            id: w.id,
            accelerator: w.accelerator,
            source: w.source.unwrap_or(Source::Selection),
            task: w.task.unwrap_or_default(),
            tutor: w.tutor.unwrap_or(legacy_tutor),
            live_sources: w.live_sources,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct Models {
    pub gemini: String,
    pub openai: String,
    pub anthropic: String,
    /// Gemini model for Live sessions (Live API).
    pub gemini_live: String,
    /// OpenAI model for Live sessions (Realtime API).
    pub openai_realtime: String,
    /// Gemini's dedicated interpreter model, used when Live only listens and adds no notes.
    /// Empty: always use `gemini_live`.
    pub gemini_translate: String,
    /// Models for 解釋 and 搜尋, which need more knowledge than translating. Empty: the main model.
    pub gemini_explain: String,
    pub openai_explain: String,
    pub anthropic_explain: String,
}

impl Default for Models {
    fn default() -> Self {
        Self {
            gemini: "gemini-3.5-flash-lite".into(),
            openai: "gpt-6-luna".into(),
            anthropic: "claude-opus-5-5".into(),
            gemini_live: "gemini-3.8-live".into(),
            openai_realtime: "gpt-realtime-2.1-mini".into(),
            gemini_translate: "gemini-3.5-live-translate-preview".into(),
            // Flash-Lite translates well but gets facts wrong when explaining (a spider as an insect).
            gemini_explain: "gemini-3.6-flash".into(),
            openai_explain: String::new(),
            anthropic_explain: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct Settings {
    /// The language translations are written in (the user's own language).
    pub target_language: String,
    pub provider: Provider,
    pub models: Models,
    pub hotkeys: Vec<Hotkey>,
    /// Live sessions pause by themselves after this many minutes without sound (0 = never).
    pub live_idle_minutes: u32,
    /// Provider for Live sessions; None picks one automatically.
    pub live_provider: Option<Provider>,
    /// Keep finished translations in the history list.
    pub save_history: bool,
    /// Last size of the translation window, in logical pixels.
    pub popup_size: Option<[u32; 2]>,
    /// The translation window stays where the user put it instead of following the cursor.
    pub popup_pinned: bool,
    /// Where a pinned translation window sits, in physical pixels.
    pub popup_pos: Option<[i32; 2]>,
    /// Closing the settings window keeps ContextLens running in the system tray (else it quits).
    pub close_to_tray: bool,
    /// Minimizing the settings window hides it in the system tray instead of the taskbar.
    pub minimize_to_tray: bool,
    /// Start with only the tray icon, no settings window.
    pub start_in_tray: bool,
    /// Live models that refused text replies, so sessions ask them for audio from the start.
    pub audio_only_models: Vec<String>,
    /// Live sources last used, so the caption bar opens with them.
    pub live_sources: Vec<LiveSource>,
    /// Text already in the user's language is translated into English (else left as is).
    pub own_language_to_english: bool,
    /// The settings window's first-run card was dismissed.
    pub welcome_seen: bool,
    /// The default Live hotkey was added once; removing it later keeps it removed.
    pub live_hotkey_added: bool,
    /// `own_language_to_english` was moved to its new default once. Versions before 0.1.11
    /// saved the old default (off) without the user choosing it.
    pub own_language_upgraded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let hk = |id: &str, acc: &str, source, tutor| Hotkey {
            id: id.into(),
            accelerator: acc.into(),
            source,
            task: Task::Translate,
            tutor,
            live_sources: vec![],
        };
        Self {
            target_language: "繁體中文".into(),
            provider: Provider::Gemini,
            models: Models::default(),
            hotkeys: vec![
                hk("default-translate", "CommandOrControl+Shift+T", Source::Selection, false),
                hk("default-tutor", "CommandOrControl+Shift+E", Source::Selection, true),
                hk("default-screen", "CommandOrControl+Alt+S", Source::Screen, false),
                hk(DEFAULT_LIVE_ID, DEFAULT_LIVE_KEY, Source::Live, false),
            ],
            live_idle_minutes: 3,
            live_provider: None,
            save_history: true,
            popup_size: None,
            popup_pinned: false,
            popup_pos: None,
            close_to_tray: true,
            minimize_to_tray: true,
            start_in_tray: false,
            audio_only_models: vec![],
            live_sources: vec![LiveSource::Mic],
            own_language_to_english: true,
            welcome_seen: false,
            live_hotkey_added: false,
            own_language_upgraded: false,
        }
    }
}

/// Defaults of earlier versions. A setting that still holds one was never chosen by the user,
/// so it moves to the current default; anything the user picked stays.
const OLD_DEFAULTS: &[&str] = &[
    "gemini-2.5-flash",
    "gpt-5-mini",
    "claude-haiku-5-5",
    "gemini-live-2.5-flash-preview",
    "gpt-realtime",
];

const DEFAULT_LIVE_ID: &str = "default-live";
const DEFAULT_LIVE_KEY: &str = "CommandOrControl+Alt+L";

impl Settings {
    pub fn upgrade_defaults(&mut self) {
        // Earlier versions had no Live hotkey, leaving Live reachable only from the tray menu.
        if !self.live_hotkey_added {
            self.live_hotkey_added = true;
            let has_live = self.hotkeys.iter().any(|h| h.source == Source::Live);
            let taken = self.hotkeys.iter().any(|h| h.accelerator == DEFAULT_LIVE_KEY);
            if !has_live && !taken {
                self.hotkeys.push(Hotkey {
                    id: DEFAULT_LIVE_ID.into(),
                    accelerator: DEFAULT_LIVE_KEY.into(),
                    source: Source::Live,
                    task: Task::Translate,
                    tutor: false,
                    live_sources: vec![],
                });
            }
        }
        if !self.own_language_upgraded {
            self.own_language_upgraded = true;
            self.own_language_to_english = true;
        }
        let new = Models::default();
        let m = &mut self.models;
        for (field, fresh) in [
            (&mut m.gemini, new.gemini),
            (&mut m.openai, new.openai),
            (&mut m.anthropic, new.anthropic),
            (&mut m.gemini_live, new.gemini_live),
            (&mut m.openai_realtime, new.openai_realtime),
        ] {
            if OLD_DEFAULTS.contains(&field.as_str()) {
                *field = fresh;
            }
        }
    }

    pub fn model_for(&self, provider: Provider) -> &str {
        match provider {
            Provider::Gemini => &self.models.gemini,
            Provider::OpenAi => &self.models.openai,
            Provider::Anthropic => &self.models.anthropic,
        }
    }

    /// The model for a task: 解釋 and 搜尋 may use a stronger one than translating.
    pub fn model_for_task(&self, provider: Provider, task: Task) -> &str {
        let explain = match provider {
            Provider::Gemini => &self.models.gemini_explain,
            Provider::OpenAi => &self.models.openai_explain,
            Provider::Anthropic => &self.models.anthropic_explain,
        };
        if task != Task::Translate && !explain.trim().is_empty() {
            explain.trim()
        } else {
            self.model_for(provider)
        }
    }

    /// The model a provider uses for Live, if it has a real-time API.
    pub fn live_model_for(&self, provider: Provider) -> Option<&str> {
        match provider {
            Provider::Gemini => Some(&self.models.gemini_live),
            Provider::OpenAi => Some(&self.models.openai_realtime),
            Provider::Anthropic => None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.target_language.trim().is_empty() {
            return Err("目標語言不能是空的".into());
        }
        for p in Provider::ALL {
            if self.model_for(p).trim().is_empty() {
                return Err(format!("{} 的模型名稱不能是空的", p.display_name()));
            }
        }
        let mut seen = HashSet::new();
        for h in &self.hotkeys {
            let key = h.accelerator.trim().to_lowercase();
            if key.is_empty() {
                return Err("熱鍵不能是空的".into());
            }
            if !seen.insert(key) {
                return Err(format!("熱鍵 {} 重複了", h.accelerator));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explaining_can_use_a_stronger_model() {
        let mut s = Settings::default();
        assert_eq!(s.model_for_task(Provider::Gemini, Task::Translate), "gemini-3.5-flash-lite");
        assert_eq!(s.model_for_task(Provider::Gemini, Task::Explain), "gemini-3.6-flash");
        assert_eq!(s.model_for_task(Provider::Anthropic, Task::Search), s.models.anthropic);
        s.models.gemini_explain.clear();
        assert_eq!(s.model_for_task(Provider::Gemini, Task::Search), "gemini-3.5-flash-lite");
    }

    #[test]
    fn older_settings_get_a_live_hotkey_once() {
        let mut s: Settings = serde_json::from_str(
            r#"{"hotkeys":[{"id":"a","accelerator":"CommandOrControl+Shift+T","source":"selection","task":"translate","tutor":false}]}"#,
        )
        .unwrap();
        s.upgrade_defaults();
        assert!(s.hotkeys.iter().any(|h| h.source == Source::Live));
        s.hotkeys.retain(|h| h.source != Source::Live);
        s.upgrade_defaults();
        assert!(!s.hotkeys.iter().any(|h| h.source == Source::Live), "a removed Live hotkey stays removed");
        assert_eq!(Settings::default().hotkeys.iter().filter(|h| h.source == Source::Live).count(), 1);
    }

    #[test]
    fn the_old_own_language_default_moves_to_on_once() {
        let mut s: Settings = serde_json::from_str(r#"{"own_language_to_english":false}"#).unwrap();
        s.upgrade_defaults();
        assert!(s.own_language_to_english);
        s.own_language_to_english = false;
        s.upgrade_defaults();
        assert!(!s.own_language_to_english, "turning it off afterwards sticks");
    }

    #[test]
    fn older_settings_keep_running_in_the_tray() {
        let s: Settings = serde_json::from_str(r#"{"target_language":"English"}"#).unwrap();
        assert!(s.close_to_tray && s.minimize_to_tray && !s.start_in_tray);
    }

    #[test]
    fn defaults_are_valid_and_traditional_chinese() {
        let s = Settings::default();
        assert_eq!(s.target_language, "繁體中文");
        assert!(s.save_history);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn partial_json_fills_defaults() {
        let s: Settings = serde_json::from_str(r#"{"save_history":false}"#).unwrap();
        assert!(!s.save_history);
        assert_eq!(s.hotkeys.len(), 4);
        assert_eq!(s.models.anthropic, "claude-opus-5-5");
        assert_eq!(s.models.gemini, "gemini-3.5-flash-lite");
        assert_eq!(s.models.gemini_live, "gemini-3.8-live");
    }

    #[test]
    fn v0_1_settings_migrate() {
        let old = r#"{
            "target_language": "繁體中文", "provider": "openai", "tutor_mode": true,
            "hotkeys": [
                {"id":"a","accelerator":"CommandOrControl+Shift+T","action":"translate_selection"},
                {"id":"b","accelerator":"CommandOrControl+Shift+E","action":"tutor_selection"}
            ]
        }"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(s.provider, Provider::OpenAi);
        assert_eq!(s.hotkeys.len(), 2);
        assert_eq!((s.hotkeys[0].source, s.hotkeys[0].tutor), (Source::Selection, false));
        assert_eq!((s.hotkeys[1].source, s.hotkeys[1].tutor), (Source::Selection, true));
        assert_eq!(s.hotkeys[1].task, Task::Translate);
        // Written back in the new shape, without the legacy field.
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"source\":\"selection\"") && !json.contains("action"));
    }

    #[test]
    fn old_defaults_upgrade_but_user_choices_stay() {
        let mut s: Settings = serde_json::from_str(
            r#"{"models":{"gemini":"gemini-2.5-flash","openai":"gpt-6.1-sol","anthropic":"claude-haiku-5-5","gemini_live":"gemini-live-2.5-flash-preview"}}"#,
        )
        .unwrap();
        s.upgrade_defaults();
        assert_eq!(s.models.gemini, "gemini-3.5-flash-lite");
        assert_eq!(s.models.openai, "gpt-6.1-sol"); // chosen by the user
        assert_eq!(s.models.anthropic, "claude-opus-5-5");
        assert_eq!(s.models.gemini_live, "gemini-3.8-live");
        assert_eq!(s.models.openai_realtime, "gpt-realtime-2.1-mini");
    }

    #[test]
    fn wire_names() {
        assert_eq!(serde_json::to_string(&Provider::OpenAi).unwrap(), "\"openai\"");
        assert_eq!(serde_json::to_string(&Source::ClipboardImage).unwrap(), "\"clipboard_image\"");
        assert_eq!(serde_json::to_string(&Task::Search).unwrap(), "\"search\"");
    }

    #[test]
    fn duplicate_hotkeys_are_rejected_case_insensitively() {
        let mut s = Settings::default();
        s.hotkeys[1].accelerator = "commandorcontrol+shift+t".into();
        assert!(s.validate().unwrap_err().contains("重複"));
    }
}
