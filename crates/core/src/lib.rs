//! Everything ContextLens does that does not need a window: settings, prompts,
//! and streaming completions from Gemini, OpenAI and Anthropic.

pub mod prompt;
pub mod provider;
pub mod settings;
pub mod sse;

pub use reqwest;
pub use provider::{stream_completion, ChatRequest, ProviderError};
pub use settings::{Action, Hotkey, Provider, Settings};

/// One shared HTTP client. No total timeout, because streams can run long;
/// instead a stalled connection fails after 60 s without data.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("http client")
}
