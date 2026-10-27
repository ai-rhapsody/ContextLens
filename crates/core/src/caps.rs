//! What each model can take. Provider defaults, then model-name rules on top, so a custom or
//! newer model name still gets a sensible answer. The app and the settings page both read this.

use crate::media::MediaKind;
use crate::settings::Provider;
use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Caps {
    pub image: bool,
    pub audio: bool,
    pub video: bool,
    /// Real-time streaming sessions (Gemini Live).
    pub live: bool,
}

impl Caps {
    pub fn supports(&self, kind: MediaKind) -> bool {
        match kind {
            MediaKind::Image => self.image,
            MediaKind::Audio => self.audio,
            MediaKind::Video => self.video,
        }
    }
}

const NONE: Caps = Caps { image: false, audio: false, video: false, live: false };

pub fn for_model(provider: Provider, model: &str) -> Caps {
    let m = model.trim().to_lowercase();
    match provider {
        Provider::Gemini => {
            let live = m.contains("live") || m.contains("native-audio");
            // Embedding, TTS and image-generation models do not read media in this sense.
            let text_only = ["embedding", "-tts", "imagen"].iter().any(|p| m.contains(p));
            if text_only {
                NONE
            } else {
                Caps { image: true, audio: true, video: true, live }
            }
        }
        Provider::OpenAi => {
            let blind = ["gpt-3.5", "o1-mini", "o3-mini", "davinci", "babbage"]
                .iter()
                .any(|p| m.starts_with(p));
            Caps { image: !blind, ..NONE }
        }
        Provider::Anthropic => {
            let blind = m.starts_with("claude-2") || m.starts_with("claude-instant");
            Caps { image: !blind, ..NONE }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_reads_everything_and_live_follows_the_name() {
        let c = for_model(Provider::Gemini, "gemini-2.5-flash");
        assert!(c.image && c.audio && c.video && !c.live);
        assert!(for_model(Provider::Gemini, "gemini-live-2.5-flash").live);
        assert!(for_model(Provider::Gemini, "gemini-3.8-live").live);
        assert!(!for_model(Provider::Gemini, "gemini-embedding-001").image);
    }

    #[test]
    fn openai_and_claude_see_images_except_old_models() {
        assert!(for_model(Provider::OpenAi, "gpt-5-mini").image);
        assert!(!for_model(Provider::OpenAi, "gpt-3.5-turbo").image);
        assert!(!for_model(Provider::OpenAi, "gpt-5-mini").audio);
        assert!(for_model(Provider::Anthropic, "claude-haiku-5-5").image);
        assert!(!for_model(Provider::Anthropic, "claude-2.1").image);
        assert!(!for_model(Provider::Anthropic, "claude-haiku-5-5").live);
    }
}
