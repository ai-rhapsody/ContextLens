//! Sends one request with a fake key and prints the full error chain.
use contextlens_core::{prompt, settings::Task, stream_completion, Provider};
use std::error::Error;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let provider = match std::env::args().nth(1).as_deref() {
        Some("gemini") => Provider::Gemini,
        Some("openai") => Provider::OpenAi,
        _ => Provider::Anthropic,
    };
    let model = std::env::args().nth(2).unwrap_or("claude-haiku-5-5".into());
    let req = prompt::build_task(Task::Translate, false, "hello", None, "繁體中文", false);
    let r = stream_completion(&contextlens_core::http_client(), provider, &model, "sk-fake", &req, |_| true).await;
    match r {
        Ok(()) => println!("ok"),
        Err(e) => {
            println!("error: {e}");
            let mut src = e.source();
            while let Some(s) = src {
                println!("  caused by: {s}");
                src = s.source();
            }
        }
    }
}
