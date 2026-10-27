//! A conversation about one capture: the first answer and any follow-up questions.
//! The app keeps recent ones in memory (with their media) and rebuilds older ones from history.

use crate::media::Media;
use crate::prompt;
use crate::provider::{ChatRequest, Turn};
use crate::settings::{Provider, Task};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Conversation {
    pub id: u64,
    pub task: Task,
    pub tutor: bool,
    /// The captured text, or a note for media.
    pub text: String,
    /// The captured image, audio or video, while it is still in memory.
    pub media: Option<Media>,
    /// Shown for media in history: the file name or kind.
    pub label: String,
    pub provider: Provider,
    pub model: String,
    pub answer: String,
    pub sources: Vec<(String, String)>,
    pub thread: Vec<(String, String)>,
    /// The media was not kept (an older conversation from history); follow-ups use text only.
    pub media_lost: bool,
}

/// Quick follow-ups for a selected phrase in an answer.
pub fn about_selection(action: &str, phrase: &str) -> String {
    match action {
        "examples" => format!("請針對「{phrase}」給三個自然的例句，附上翻譯。"),
        "rephrase" => format!("「{phrase}」還有哪些說法？說明語氣或使用情境的差別。"),
        _ => format!("請解釋「{phrase}」在這裡的意思和用法。"),
    }
}

impl Conversation {
    /// The request for the next follow-up: the original content, then the whole exchange.
    pub fn follow_up(&self, question: &str, target: &str, web: bool) -> ChatRequest {
        let media = if self.media_lost { None } else { self.media.clone() };
        let text = if self.media_lost && !self.label.is_empty() {
            format!("（原本的{}已不在，只剩下面的回答）{}", self.label, self.text)
        } else {
            self.text.clone()
        };
        let mut req = prompt::build_task(self.task, self.tutor, &text, media, target, web && self.task == Task::Search);
        req.system += &format!(
            "\n\nAfter your first answer the user may ask follow-up questions about it. Answer each one in {target}, \
             directly and briefly, in plain Markdown, without repeating the section headings unless asked."
        );
        req.quick = false;
        req.turns.push(Turn { user: false, text: self.answer.clone() });
        for (q, a) in &self.thread {
            req.turns.push(Turn { user: true, text: q.clone() });
            req.turns.push(Turn { user: false, text: a.clone() });
        }
        req.turns.push(Turn { user: true, text: question.to_string() });
        req
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convo() -> Conversation {
        Conversation {
            id: 1,
            task: Task::Translate,
            tutor: false,
            text: "punching above our weight".into(),
            media: None,
            label: String::new(),
            provider: Provider::Gemini,
            model: "m".into(),
            answer: "表現超出規模".into(),
            sources: vec![],
            thread: vec![("更口語？".into(), "超常發揮".into())],
            media_lost: false,
        }
    }

    #[test]
    fn follow_up_replays_the_whole_exchange_in_order() {
        let r = convo().follow_up("再給例句", "繁體中文", false);
        assert!(r.user.contains("punching above our weight"));
        let t: Vec<(bool, &str)> = r.turns.iter().map(|t| (t.user, t.text.as_str())).collect();
        assert_eq!(t, vec![(false, "表現超出規模"), (true, "更口語？"), (false, "超常發揮"), (true, "再給例句")]);
        assert!(r.system.contains("follow-up") && !r.quick);
    }

    #[test]
    fn web_search_only_for_search_conversations() {
        assert!(!convo().follow_up("q", "繁體中文", true).web_search);
        let s = Conversation { task: Task::Search, ..convo() };
        assert!(s.follow_up("q", "繁體中文", true).web_search);
    }

    #[test]
    fn lost_media_is_said_and_not_sent() {
        let m = Media { mime: "image/png".into(), data_b64: "QUJD".into(), name: None };
        let c = Conversation { media: Some(m), media_lost: true, label: "圖片".into(), text: String::new(), ..convo() };
        let r = c.follow_up("q", "繁體中文", false);
        assert!(r.media.is_empty() && r.user.contains("圖片已不在"));
    }

    #[test]
    fn selection_prompts_quote_the_phrase() {
        assert!(about_selection("explain", "weight").contains("「weight」"));
        assert!(about_selection("examples", "x").contains("例句"));
    }
}
