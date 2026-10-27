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
        }
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
    fn round_trips_as_a_plain_array() {
        let mut h = History::default();
        h.push(e(1, "a", "b"));
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.starts_with('['));
        let back: History = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
    }
}
