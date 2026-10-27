//! Everything ContextLens does that does not need a window: settings, prompts, media rules,
//! history, and streaming completions from Gemini, OpenAI and Anthropic.

pub mod caps;
pub mod conversation;
pub mod history;
pub mod live;
pub mod media;
pub mod prompt;
pub mod provider;
pub mod settings;
pub mod sse;

pub use caps::Caps;
pub use conversation::Conversation;
pub use history::{History, HistoryEntry};
pub use media::{Media, MediaKind};
pub use provider::{can_search, stream_completion, ChatRequest, Delta, ProviderError, Turn};
pub use reqwest;
pub use settings::{Hotkey, LiveSource, Provider, Settings, Source, Task};

/// One shared HTTP client. No total timeout, because streams can run long;
/// instead a stalled connection fails after 90 s without data (video can take a while).
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("http client")
}
