//! Images, audio and video sent alongside a prompt, and what each provider accepts.

use crate::caps;
use crate::settings::Provider;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
}

impl MediaKind {
    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime.split('/').next()? {
            "image" => Some(Self::Image),
            "audio" => Some(Self::Audio),
            "video" => Some(Self::Video),
            _ => None,
        }
    }

    /// What to call it in a message, in the interface language.
    pub fn label(self) -> String {
        match self {
            Self::Image => crate::t!("圖片"),
            Self::Audio => crate::t!("音訊"),
            Self::Video => crate::t!("影片"),
        }
    }

    fn english(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio recording",
            Self::Video => "video",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Media {
    pub mime: String,
    /// Base64 of the file's bytes, without a `data:` prefix.
    pub data_b64: String,
    /// The file name, when it came from a file.
    #[serde(default)]
    pub name: Option<String>,
}

impl Media {
    pub fn kind(&self) -> Option<MediaKind> {
        MediaKind::from_mime(&self.mime)
    }

    /// Size of the decoded bytes, from the base64 length.
    pub fn byte_len(&self) -> usize {
        let pad = self.data_b64.bytes().rev().take_while(|&b| b == b'=').count();
        self.data_b64.len() / 4 * 3 - pad.min(2)
    }

    pub fn english(&self) -> &'static str {
        self.kind().map_or("file", MediaKind::english)
    }
}

const MB: usize = 1024 * 1024;

/// Accepted MIME types and the largest file sent inline, per provider and kind.
fn accepts(provider: Provider, kind: MediaKind) -> Option<(&'static [&'static str], usize)> {
    use MediaKind::*;
    use Provider::*;
    const GEMINI_IMAGE: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/heic", "image/heif"];
    const GEMINI_AUDIO: &[&str] = &[
        "audio/wav", "audio/x-wav", "audio/mp3", "audio/mpeg", "audio/aiff", "audio/aac",
        "audio/ogg", "audio/flac", "audio/mp4", "audio/x-m4a", "audio/webm",
    ];
    const GEMINI_VIDEO: &[&str] = &[
        "video/mp4", "video/mpeg", "video/quicktime", "video/x-msvideo", "video/x-flv",
        "video/mpg", "video/webm", "video/x-ms-wmv", "video/3gpp",
    ];
    const WEB_IMAGES: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];
    match (provider, kind) {
        // Gemini takes up to 20 MB per request inline; base64 adds a third.
        (Gemini, Image) => Some((GEMINI_IMAGE, 14 * MB)),
        (Gemini, Audio) => Some((GEMINI_AUDIO, 14 * MB)),
        (Gemini, Video) => Some((GEMINI_VIDEO, 14 * MB)),
        (OpenAi, Image) => Some((WEB_IMAGES, 14 * MB)),
        (Anthropic, Image) => Some((WEB_IMAGES, 5 * MB)),
        _ => None,
    }
}

/// The largest image a provider takes, so the app can shrink screenshots before sending.
pub fn image_byte_limit(provider: Provider) -> usize {
    accepts(provider, MediaKind::Image).map_or(5 * MB, |(_, max)| max)
}

pub fn check(provider: Provider, model: &str, media: &Media) -> Result<(), String> {
    let kind = media
        .kind()
        .ok_or_else(|| crate::t!("不支援這種檔案（{mime}）。", mime = media.mime))?;
    let unsupported = || {
        crate::t!(
            "{provider}（{model}）不支援{kind}。改用支援的服務，例如 Gemini。",
            provider = provider.display_name(),
            model = model,
            kind = kind.label()
        )
    };
    if !caps::for_model(provider, model).supports(kind) {
        return Err(unsupported());
    }
    let (mimes, max) = accepts(provider, kind).ok_or_else(unsupported)?;
    if !mimes.contains(&media.mime.as_str()) {
        return Err(crate::t!(
            "{provider} 不支援這種{kind}格式（{mime}）。",
            provider = provider.display_name(),
            kind = kind.label(),
            mime = media.mime
        ));
    }
    if media.byte_len() > max {
        return Err(crate::t!(
            "檔案太大：{size} MB，{provider} 的上限是 {max} MB。",
            size = format!("{:.1}", media.byte_len() as f64 / MB as f64),
            provider = provider.display_name(),
            max = max / MB
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(mime: &str, bytes: usize) -> Media {
        let n = bytes.div_ceil(3) * 4;
        Media { mime: mime.into(), data_b64: "A".repeat(n), name: None }
    }

    #[test]
    fn byte_len_accounts_for_padding() {
        let x = Media { mime: "image/png".into(), data_b64: "QUJD".into(), name: None };
        assert_eq!(x.byte_len(), 3);
        let y = Media { mime: "image/png".into(), data_b64: "QUI=".into(), name: None };
        assert_eq!(y.byte_len(), 2);
    }

    #[test]
    fn gemini_takes_all_three_kinds() {
        assert!(check(Provider::Gemini, "gemini-2.5-flash", &m("image/png", 1000)).is_ok());
        assert!(check(Provider::Gemini, "gemini-2.5-flash", &m("audio/mpeg", 1000)).is_ok());
        assert!(check(Provider::Gemini, "gemini-2.5-flash", &m("video/mp4", 1000)).is_ok());
    }

    #[test]
    fn openai_and_claude_take_images_only() {
        assert!(check(Provider::OpenAi, "gpt-5-mini", &m("image/jpeg", 1000)).is_ok());
        let e = check(Provider::Anthropic, "claude-haiku-5-5", &m("video/mp4", 1000)).unwrap_err();
        assert!(e.contains("Claude") && e.contains("影片") && e.contains("Gemini"));
    }

    #[test]
    fn the_model_matters_not_just_the_provider() {
        let e = check(Provider::OpenAi, "gpt-3.5-turbo", &m("image/png", 1000)).unwrap_err();
        assert!(e.contains("gpt-3.5-turbo"));
    }

    #[test]
    fn size_and_format_limits() {
        assert!(check(Provider::Anthropic, "claude-haiku-5-5", &m("image/png", 6 * MB)).unwrap_err().contains("太大"));
        assert!(check(Provider::Gemini, "gemini-2.5-flash", &m("image/gif", 10)).unwrap_err().contains("格式"));
        assert!(check(Provider::Gemini, "gemini-2.5-flash", &m("application/pdf", 10)).unwrap_err().contains("不支援"));
    }
}
