//! The list of finished translations. File I/O lives in the app; this is the pure part.

use crate::settings::Provider;
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 1000;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct HistoryEntry {
    /// Milliseconds since the Unix epoch; also the id.
    pub at_ms: u64,
    /// `text`, `image`, `audio` or `video`.
    pub kind: String,
    /// The source text, or the file name for media.
    pub source: String,
    /// A small JPEG data URL for images.
    #[serde(default)]
    pub thumb: Option<String>,
    pub result: String,
    pub tutor: bool,
    pub provider: Provider,
    pub model: String,
    /// Web pages a search drew on: (title, url).
    #[serde(default)]
    pub sources: Vec<(String, String)>,
    /// Follow-up questions and their answers, in order.
    #[serde(default)]
    pub thread: Vec<(String, String)>,
    /// What was asked for: `translate`, `explain`, `search` or `custom`.
    #[serde(default)]
    pub task: Option<crate::settings::Task>,
    /// For `custom`: the hotkey's instruction, so follow-ups keep it.
    #[serde(default)]
    pub action: Option<crate::settings::CustomAction>,
    /// Kept for studying: shown with a star, exported to a vocabulary list.
    #[serde(default)]
    pub starred: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct History {
    entries: Vec<HistoryEntry>, // newest first
}

impl History {
    pub fn push(&mut self, mut e: HistoryEntry) {
        // Two entries in the same millisecond still need distinct ids.
        if let Some(first) = self.entries.first() {
            if e.at_ms <= first.at_ms {
                e.at_ms = first.at_ms + 1;
            }
        }
        self.entries.insert(0, e);
        self.entries.truncate(MAX_ENTRIES);
    }

    pub fn get(&self, at_ms: u64) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.at_ms == at_ms)
    }

    /// Adds a follow-up exchange to an entry; false if the entry is gone.
    pub fn append_thread(&mut self, at_ms: u64, question: String, answer: String) -> bool {
        match self.entries.iter_mut().find(|e| e.at_ms == at_ms) {
            Some(e) => {
                e.thread.push((question, answer));
                true
            }
            None => false,
        }
    }

    /// Pushes an entry and returns the id it was stored under (ids are unique).
    pub fn push_id(&mut self, e: HistoryEntry) -> u64 {
        self.push(e);
        self.entries[0].at_ms
    }

    /// Stars or unstars an entry; false if the entry is gone.
    pub fn set_starred(&mut self, at_ms: u64, starred: bool) -> bool {
        match self.entries.iter_mut().find(|e| e.at_ms == at_ms) {
            Some(e) => {
                e.starred = starred;
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, at_ms: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.at_ms != at_ms);
        before != self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Case-insensitive match on source and result; an empty query returns everything.
    pub fn search(&self, query: &str, limit: usize) -> Vec<HistoryEntry> {
        let q = query.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                q.is_empty()
                    || e.source.to_lowercase().contains(&q)
                    || e.result.to_lowercase().contains(&q)
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The part of a result worth studying: the translation, explanation or identification
/// section when the answer has headings, else all of it.
pub fn lead(result: &str) -> String {
    use crate::prompt::{H_EXPLAIN, H_TRANSLATION, H_WHAT};
    for title in [H_TRANSLATION, H_EXPLAIN, H_WHAT] {
        let mut lines = result.lines().skip_while(|l| l.trim() != format!("## {title}"));
        if lines.next().is_some() {
            let body: Vec<&str> = lines.take_while(|l| !l.starts_with("## ")).collect();
            return body.join("\n").trim().to_string();
        }
    }
    result.trim().to_string()
}

/// A spreadsheet of entries: time, kind, original, result, provider and model. Starts with a
/// byte-order mark so Excel reads it as UTF-8.
pub fn to_csv(entries: &[HistoryEntry], time: impl Fn(u64) -> String) -> String {
    let cell = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("\u{FEFF}時間,類型,原文,結果,服務,模型\r\n");
    for e in entries {
        let row = [time(e.at_ms), e.kind.clone(), e.source.clone(), lead(&e.result), e.provider.display_name().into(), e.model.clone()];
        out += &row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(",");
        out += "\r\n";
    }
    out
}

/// Flashcards for Anki's text import: the original on the front, the translation on the back.
/// Pictures and recordings have no text to learn from and are left out.
pub fn to_anki(entries: &[HistoryEntry]) -> String {
    let field = |s: &str| {
        s.trim()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\t', " ")
            .replace("\r\n", "\n")
            .replace('\n', "<br>")
    };
    let mut out = String::from("#separator:tab\n#html:true\n#tags column:3\n");
    for e in entries.iter().filter(|e| e.kind == "text" || e.kind == "live") {
        out += &format!("{}\t{}\tContextLens\n", field(&e.source), field(&lead(&e.result)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(at: u64, src: &str, res: &str) -> HistoryEntry {
        HistoryEntry {
            at_ms: at,
            kind: "text".into(),
            source: src.into(),
            thumb: None,
            result: res.into(),
            tutor: false,
            provider: Provider::Gemini,
            model: "m".into(),
            sources: vec![],
            thread: vec![],
            task: None,
            action: None,
            starred: false,
        }
    }

    #[test]
    fn stars_stick_to_their_entry() {
        let mut h = History::default();
        h.push(e(1, "a", "x"));
        assert!(h.set_starred(1, true) && h.get(1).unwrap().starred);
        assert!(!h.set_starred(99, true));
    }

    #[test]
    fn exports_take_the_translation_and_escape_it() {
        let tutor = e(1, "punch above\tits weight", "## 翻譯\n表現超出\n實力\n## 結構\n- 片語");
        let live = HistoryEntry { kind: "live".into(), ..e(2, "hello", "你好") };
        let image = HistoryEntry { kind: "image".into(), ..e(3, "chart.png", "## 說明\n上升<5%>") };
        assert_eq!(lead(&tutor.result), "表現超出\n實力");
        assert_eq!(lead("plain"), "plain");
        let anki = to_anki(&[tutor.clone(), live, image.clone()]);
        assert!(anki.starts_with("#separator:tab\n"));
        assert!(anki.contains("punch above its weight\t表現超出<br>實力\tContextLens\n"));
        assert!(anki.contains("hello\t你好\t") && !anki.contains("chart.png"));
        let csv = to_csv(&[image, HistoryEntry { source: "say \"hi\"".into(), ..tutor }], |ms| ms.to_string());
        assert!(csv.starts_with('\u{FEFF}'));
        assert!(csv.contains("\"3\",\"image\",\"chart.png\",\"上升<5%>\""));
        assert!(csv.contains("\"say \"\"hi\"\"\""));
    }

    #[test]
    fn newest_first_with_unique_ids() {
        let mut h = History::default();
        h.push(e(10, "a", "甲"));
        h.push(e(10, "b", "乙"));
        let all = h.search("", 10);
        assert_eq!(all[0].source, "b");
        assert_eq!(all[0].at_ms, 11);
    }

    #[test]
    fn search_matches_source_and_result_case_insensitively() {
        let mut h = History::default();
        h.push(e(1, "Deadline", "截止日期"));
        h.push(e(2, "hello", "你好"));
        assert_eq!(h.search("deadline", 10).len(), 1);
        assert_eq!(h.search("你好", 10)[0].source, "hello");
    }

    #[test]
    fn capped_and_removable() {
        let mut h = History::default();
        for i in 0..(MAX_ENTRIES as u64 + 5) {
            h.push(e(i, "x", "y"));
        }
        assert_eq!(h.len(), MAX_ENTRIES);
        let id = h.search("", 1)[0].at_ms;
        assert!(h.remove(id));
        assert!(!h.remove(id));
    }

    #[test]
    fn threads_append_to_the_right_entry() {
        let mut h = History::default();
        let a = h.push_id(e(5, "a", "甲"));
        let b = h.push_id(e(5, "b", "乙"));
        assert_ne!(a, b);
        assert!(h.append_thread(a, "q".into(), "答".into()));
        assert_eq!(h.get(a).unwrap().thread, vec![("q".to_string(), "答".to_string())]);
        assert!(h.get(b).unwrap().thread.is_empty());
        assert!(!h.append_thread(999, "q".into(), "x".into()));
    }

    #[test]
    fn old_entries_without_threads_still_load() {
        let old = r#"[{"at_ms":1,"kind":"text","source":"a","result":"b","tutor":false,"provider":"gemini","model":"m"}]"#;
        let h: History = serde_json::from_str(old).unwrap();
        assert!(h.get(1).unwrap().thread.is_empty());
    }

    #[test]
    fn round_trips_as_a_plain_array() {
        let mut h = History::default();
        h.push(e(1, "a", "b"));
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.starts_with('['));
        let back: History = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
    }
}
