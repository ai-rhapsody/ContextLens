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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(from = "HotkeyWire")]
pub struct Hotkey {
    pub id: String,
    /// Accelerator string, e.g. `CommandOrControl+Shift+T`.
    pub accelerator: String,
    pub source: Source,
    /// Explain structure, usage and background as well as translating.
    pub tutor: bool,
}

/// Accepts the current shape and the 0.1 shape (`action: translate_selection | tutor_selection`).
#[derive(Deserialize)]
struct HotkeyWire {
    id: String,
    accelerator: String,
    source: Option<Source>,
    tutor: Option<bool>,
    action: Option<String>,
}

impl From<HotkeyWire> for Hotkey {
    fn from(w: HotkeyWire) -> Self {
        let legacy_tutor = w.action.as_deref() == Some("tutor_selection");
        Hotkey {
            id: w.id,
            accelerator: w.accelerator,
            source: w.source.unwrap_or(Source::Selection),
            tutor: w.tutor.unwrap_or(legacy_tutor),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct Models {
    pub gemini: String,
    pub openai: String,
    pub anthropic: String,
}

impl Default for Models {
    fn default() -> Self {
        Self {
            gemini: "gemini-2.5-flash".into(),
            openai: "gpt-5-mini".into(),
            anthropic: "claude-haiku-5-5".into(),
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
    /// Keep finished translations in the history list.
    pub save_history: bool,
    /// Last size of the translation window, in logical pixels.
    pub popup_size: Option<[u32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        let hk = |id: &str, acc: &str, source, tutor| Hotkey {
            id: id.into(),
            accelerator: acc.into(),
            source,
            tutor,
        };
        Self {
            target_language: "繁體中文".into(),
            provider: Provider::Gemini,
            models: Models::default(),
            hotkeys: vec![
                hk("default-translate", "CommandOrControl+Shift+T", Source::Selection, false),
                hk("default-tutor", "CommandOrControl+Shift+E", Source::Selection, true),
                hk("default-screen", "CommandOrControl+Alt+S", Source::Screen, false),
            ],
            save_history: true,
            popup_size: None,
        }
    }
}

impl Settings {
    pub fn model_for(&self, provider: Provider) -> &str {
        match provider {
            Provider::Gemini => &self.models.gemini,
            Provider::OpenAi => &self.models.openai,
            Provider::Anthropic => &self.models.anthropic,
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
        assert_eq!(s.hotkeys.len(), 3);
        assert_eq!(s.models.anthropic, "claude-haiku-5-5");
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
        // Written back in the new shape, without the legacy field.
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"source\":\"selection\"") && !json.contains("action"));
    }

    #[test]
    fn wire_names() {
        assert_eq!(serde_json::to_string(&Provider::OpenAi).unwrap(), "\"openai\"");
        assert_eq!(serde_json::to_string(&Source::ClipboardImage).unwrap(), "\"clipboard_image\"");
    }

    #[test]
    fn duplicate_hotkeys_are_rejected_case_insensitively() {
        let mut s = Settings::default();
        s.hotkeys[1].accelerator = "commandorcontrol+shift+t".into();
        assert!(s.validate().unwrap_err().contains("重複"));
    }
}
