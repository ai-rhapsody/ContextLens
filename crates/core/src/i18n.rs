//! Interface languages for messages built in Rust (errors, the tray menu, notices, prompts
//! that follow the interface language). Keys are the Traditional Chinese text itself, as in
//! the frontend, and the dictionaries are the frontend's own files in `src/locales`. A key
//! can start with a context, `heading::翻譯`, when the same Chinese needs different words
//! elsewhere; Chinese shows the text after `::`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Locale {
    ZhTw,
    En,
    Ja,
    Ko,
    Es,
}

impl Locale {
    const ALL: [Locale; 5] = [Locale::ZhTw, Locale::En, Locale::Ja, Locale::Ko, Locale::Es];

    /// The tag the frontend uses.
    pub fn tag(self) -> &'static str {
        match self {
            Locale::ZhTw => "zh-TW",
            Locale::En => "en",
            Locale::Ja => "ja",
            Locale::Ko => "ko",
            Locale::Es => "es",
        }
    }

    /// From a BCP 47 tag (`en-US`, `zh-Hant-TW`, `ja_JP`): any Chinese reads Traditional,
    /// anything unsupported English.
    pub fn from_tag(tag: &str) -> Locale {
        let t = tag.trim().to_lowercase();
        if t.starts_with("zh") {
            return Locale::ZhTw;
        }
        [Locale::Ja, Locale::Ko, Locale::Es].into_iter().find(|l| t.starts_with(l.tag())).unwrap_or(Locale::En)
    }

    /// The language a new user's translations go into, as the settings page names it.
    pub fn language_name(self) -> &'static str {
        match self {
            Locale::ZhTw => "繁體中文",
            Locale::En => "English",
            Locale::Ja => "日本語",
            Locale::Ko => "한국어",
            Locale::Es => "Español",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_locale(l: Locale) {
    CURRENT.store(Locale::ALL.iter().position(|x| *x == l).unwrap_or(0) as u8, Ordering::SeqCst);
}

pub fn locale() -> Locale {
    Locale::ALL[CURRENT.load(Ordering::SeqCst) as usize]
}

fn dict(l: Locale) -> Option<&'static HashMap<String, String>> {
    static DICTS: OnceLock<[HashMap<String, String>; 4]> = OnceLock::new();
    let all = DICTS.get_or_init(|| {
        let parse = |s: &str| serde_json::from_str(s).expect("locale file is valid JSON");
        [
            parse(include_str!("../../../src/locales/en.json")),
            parse(include_str!("../../../src/locales/ja.json")),
            parse(include_str!("../../../src/locales/ko.json")),
            parse(include_str!("../../../src/locales/es.json")),
        ]
    });
    match l {
        Locale::ZhTw => None,
        Locale::En => Some(&all[0]),
        Locale::Ja => Some(&all[1]),
        Locale::Ko => Some(&all[2]),
        Locale::Es => Some(&all[3]),
    }
}

/// `key` (the Chinese text) in `l`, or the Chinese text itself when there is no translation.
pub fn tr_in(l: Locale, key: &str) -> String {
    dict(l).and_then(|d| d.get(key)).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| plain(key).to_string())
}

/// The Chinese text of a key, without its context.
fn plain(key: &str) -> &str {
    match key.split_once("::") {
        Some((ctx, text)) if !ctx.is_empty() && ctx.chars().all(|c| c.is_ascii_lowercase()) => text,
        _ => key,
    }
}

/// `key` in the interface language.
pub fn tr(key: &str) -> String {
    tr_in(locale(), key)
}

/// `key` in the interface language with `{name}` placeholders filled in.
pub fn trf(key: &str, vars: &[(&str, String)]) -> String {
    let mut s = tr(key);
    for (name, value) in vars {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

/// Every language's version of `key`, to recognise text written under another language
/// (answer headings saved in history).
pub fn variants(key: &str) -> Vec<String> {
    let mut v: Vec<String> = Locale::ALL.iter().map(|l| tr_in(*l, key)).collect();
    v.dedup();
    v
}

/// `t!("…{name}…", name = value)`: the text in the interface language, placeholders filled.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::trf($key, &[$((stringify!($name), ($value).to_string())),+])
    };
}

/// Marks a constant's text for translation; `tr` it where it is used.
#[macro_export]
macro_rules! n_ {
    ($key:literal) => {
        $key
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_map_to_supported_languages() {
        assert_eq!(Locale::from_tag("zh-Hant-TW"), Locale::ZhTw);
        assert_eq!(Locale::from_tag("zh_CN"), Locale::ZhTw);
        assert_eq!(Locale::from_tag("ja-JP"), Locale::Ja);
        assert_eq!(Locale::from_tag("ko"), Locale::Ko);
        assert_eq!(Locale::from_tag("es-419"), Locale::Es);
        assert_eq!(Locale::from_tag("fr-FR"), Locale::En);
        assert_eq!(Locale::from_tag("en-GB"), Locale::En);
    }

    #[test]
    fn chinese_is_the_key_and_placeholders_fill() {
        assert_eq!(tr_in(Locale::ZhTw, "翻譯"), "翻譯");
        assert_eq!(trf("{name} 測試", &[("name", "Gemini".into())]), "Gemini 測試");
        assert_eq!(tr_in(Locale::En, "a key no file has"), "a key no file has");
        assert_eq!(tr_in(Locale::ZhTw, "heading::翻譯"), "翻譯");
        assert_eq!(tr_in(Locale::En, "heading::翻譯"), "Translation");
        assert_eq!(tr_in(Locale::En, "翻譯"), "Translate");
    }
}
