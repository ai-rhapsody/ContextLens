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
}

/// What a hotkey does when it fires. Image and video capture are planned
/// (see docs/ROADMAP.md) and will be added here as new variants.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Translate the selected text; tutor mode follows the global toggle.
    TranslateSelection,
    /// Translate the selected text and always explain it.
    TutorSelection,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub id: String,
    /// Accelerator string, e.g. `CommandOrControl+Shift+T`.
    pub accelerator: String,
    pub action: Action,
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
    /// When on, `TranslateSelection` also explains structure, usage and background.
    pub tutor_mode: bool,
    pub hotkeys: Vec<Hotkey>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target_language: "繁體中文".into(),
            provider: Provider::Gemini,
            models: Models::default(),
            tutor_mode: false,
            hotkeys: vec![
                Hotkey {
                    id: "default-translate".into(),
                    accelerator: "CommandOrControl+Shift+T".into(),
                    action: Action::TranslateSelection,
                },
                Hotkey {
                    id: "default-tutor".into(),
                    accelerator: "CommandOrControl+Shift+E".into(),
                    action: Action::TutorSelection,
                },
            ],
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
                return Err(format!("{} 的模型名稱不能是空的", p.id()));
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
        assert!(s.validate().is_ok());
    }

    #[test]
    fn partial_json_fills_defaults() {
        let s: Settings = serde_json::from_str(r#"{"tutor_mode":true}"#).unwrap();
        assert!(s.tutor_mode);
        assert_eq!(s.hotkeys.len(), 2);
        assert_eq!(s.models.anthropic, "claude-haiku-5-5");
    }

    #[test]
    fn provider_and_action_wire_names() {
        assert_eq!(serde_json::to_string(&Provider::OpenAi).unwrap(), "\"openai\"");
        assert_eq!(
            serde_json::to_string(&Action::TutorSelection).unwrap(),
            "\"tutor_selection\""
        );
    }

    #[test]
    fn duplicate_hotkeys_are_rejected_case_insensitively() {
        let mut s = Settings::default();
        s.hotkeys[1].accelerator = "commandorcontrol+shift+t".into();
        assert!(s.validate().unwrap_err().contains("重複"));
    }
}
