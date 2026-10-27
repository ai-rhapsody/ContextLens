use crate::settings::Provider;
use crate::sse::SseDecoder;
use futures_util::StreamExt;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub system: String,
    pub user: String,
}

#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("網路錯誤：{0}")]
    Network(#[from] reqwest::Error),
    #[error("{provider} 回應 {status}：{message}")]
    Api {
        provider: &'static str,
        status: u16,
        message: String,
    },
    #[error("{0}")]
    Stream(String),
}

/// A fully built HTTP request, kept separate from sending so it can be tested.
#[derive(Debug)]
pub struct HttpPlan {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Value,
}

pub fn plan(provider: Provider, model: &str, api_key: &str, req: &ChatRequest) -> HttpPlan {
    match provider {
        Provider::OpenAi => HttpPlan {
            url: "https://api.openai.com/v1/chat/completions".into(),
            headers: vec![("authorization", format!("Bearer {api_key}"))],
            body: json!({
                "model": model,
                "stream": true,
                "messages": [
                    { "role": "system", "content": req.system },
                    { "role": "user", "content": req.user },
                ],
            }),
        },
        Provider::Anthropic => HttpPlan {
            url: "https://api.anthropic.com/v1/messages".into(),
            headers: vec![
                ("x-api-key", api_key.to_string()),
                ("anthropic-version", "2023-06-01".into()),
            ],
            body: json!({
                "model": model,
                "stream": true,
                "max_tokens": 4096,
                "system": req.system,
                "messages": [{ "role": "user", "content": req.user }],
            }),
        },
        Provider::Gemini => HttpPlan {
            url: format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse"
            ),
            headers: vec![("x-goog-api-key", api_key.to_string())],
            body: json!({
                "systemInstruction": { "parts": [{ "text": req.system }] },
                "contents": [{ "role": "user", "parts": [{ "text": req.user }] }],
            }),
        },
    }
}

/// Reads one SSE `data:` payload and returns the text it adds, if any.
pub fn extract_delta(provider: Provider, data: &str) -> Result<Option<String>, ProviderError> {
    if data.trim() == "[DONE]" {
        return Ok(None);
    }
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return Ok(None), // keep-alives and other non-JSON lines
    };
    match provider {
        Provider::OpenAi => {
            if let Some(msg) = v.pointer("/error/message").and_then(Value::as_str) {
                return Err(ProviderError::Stream(msg.into()));
            }
            Ok(v.pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(String::from))
        }
        Provider::Anthropic => match v.get("type").and_then(Value::as_str) {
            Some("content_block_delta")
                if v.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") =>
            {
                Ok(v.pointer("/delta/text").and_then(Value::as_str).map(String::from))
            }
            Some("error") => Err(ProviderError::Stream(
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Anthropic 串流錯誤")
                    .into(),
            )),
            _ => Ok(None),
        },
        Provider::Gemini => {
            if let Some(msg) = v.pointer("/error/message").and_then(Value::as_str) {
                return Err(ProviderError::Stream(msg.into()));
            }
            if let Some(reason) = v.pointer("/promptFeedback/blockReason").and_then(Value::as_str) {
                return Err(ProviderError::Stream(format!("Gemini 拒絕了這段內容（{reason}）")));
            }
            let text: String = v
                .pointer("/candidates/0/content/parts")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
                        .filter_map(|p| p.get("text").and_then(Value::as_str))
                        .collect()
                })
                .unwrap_or_default();
            Ok((!text.is_empty()).then_some(text))
        }
    }
}

fn api_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| body.chars().take(300).collect())
}

/// Streams a completion, calling `on_delta` with each piece of text.
/// Returning `false` from `on_delta` stops the stream early (a newer request replaced this one).
pub async fn stream_completion(
    client: &reqwest::Client,
    provider: Provider,
    model: &str,
    api_key: &str,
    req: &ChatRequest,
    on_delta: impl FnMut(&str) -> bool,
) -> Result<(), ProviderError> {
    run_plan(client, provider, plan(provider, model, api_key, req), on_delta).await
}

pub async fn run_plan(
    client: &reqwest::Client,
    provider: Provider,
    plan: HttpPlan,
    mut on_delta: impl FnMut(&str) -> bool,
) -> Result<(), ProviderError> {
    let mut rb = client.post(&plan.url).json(&plan.body);
    for (k, v) in &plan.headers {
        rb = rb.header(*k, v);
    }
    let resp = rb.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Api {
            provider: provider.id(),
            status: status.as_u16(),
            message: api_error_message(&body),
        });
    }

    let mut decoder = SseDecoder::default();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        for data in decoder.push(&chunk?) {
            if let Some(text) = extract_delta(provider, &data)? {
                if !on_delta(&text) {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn req() -> ChatRequest {
        ChatRequest { system: "sys".into(), user: "usr".into() }
    }

    #[test]
    fn openai_plan_uses_bearer_and_chat_messages() {
        let p = plan(Provider::OpenAi, "m", "K", &req());
        assert_eq!(p.headers[0], ("authorization", "Bearer K".to_string()));
        assert_eq!(p.body["messages"][0]["role"], "system");
        assert_eq!(p.body["stream"], true);
    }

    #[test]
    fn anthropic_plan_has_version_header_and_max_tokens() {
        let p = plan(Provider::Anthropic, "m", "K", &req());
        assert!(p.headers.contains(&("anthropic-version", "2023-06-01".to_string())));
        assert!(p.headers.contains(&("x-api-key", "K".to_string())));
        assert_eq!(p.body["system"], "sys");
        assert!(p.body["max_tokens"].as_u64().unwrap() > 0);
    }

    #[test]
    fn gemini_plan_puts_model_in_url_and_key_in_header_not_url() {
        let p = plan(Provider::Gemini, "gemini-x", "SECRET", &req());
        assert!(p.url.contains("models/gemini-x:streamGenerateContent?alt=sse"));
        assert!(!p.url.contains("SECRET"));
        assert_eq!(p.headers[0], ("x-goog-api-key", "SECRET".to_string()));
    }

    #[test]
    fn openai_deltas() {
        let d = r#"{"choices":[{"delta":{"content":"你好"}}]}"#;
        assert_eq!(extract_delta(Provider::OpenAi, d).unwrap().as_deref(), Some("你好"));
        assert_eq!(extract_delta(Provider::OpenAi, "[DONE]").unwrap(), None);
        let role_only = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(extract_delta(Provider::OpenAi, role_only).unwrap(), None);
    }

    #[test]
    fn anthropic_deltas_and_errors() {
        let d = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        assert_eq!(extract_delta(Provider::Anthropic, d).unwrap().as_deref(), Some("Hi"));
        let ping = r#"{"type":"ping"}"#;
        assert_eq!(extract_delta(Provider::Anthropic, ping).unwrap(), None);
        let err = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(extract_delta(Provider::Anthropic, err).is_err());
    }

    #[test]
    fn gemini_deltas_skip_thoughts_and_report_blocks() {
        let d = r#"{"candidates":[{"content":{"parts":[{"text":"想","thought":true},{"text":"答"},{"text":"案"}]}}]}"#;
        assert_eq!(extract_delta(Provider::Gemini, d).unwrap().as_deref(), Some("答案"));
        let blocked = r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#;
        assert!(extract_delta(Provider::Gemini, blocked).is_err());
    }

    #[test]
    fn api_error_prefers_the_json_message() {
        assert_eq!(api_error_message(r#"{"error":{"message":"bad key"}}"#), "bad key");
        assert_eq!(api_error_message("plain"), "plain");
    }

    /// Serves one canned HTTP response on localhost and returns its base URL.
    async fn serve_once(status: &'static str, body: &'static str, content_type: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let _ = sock.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
        });
        format!("http://{addr}/")
    }

    fn local_plan(url: String) -> HttpPlan {
        HttpPlan { url, headers: vec![], body: json!({}) }
    }

    #[tokio::test]
    async fn streams_deltas_end_to_end() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"好\"}}]}\n\n\
                    data: [DONE]\n\n";
        let url = serve_once("200 OK", body, "text/event-stream").await;
        let mut got = String::new();
        run_plan(&reqwest::Client::new(), Provider::OpenAi, local_plan(url), |t| {
            got.push_str(t);
            true
        })
        .await
        .unwrap();
        assert_eq!(got, "你好");
    }

    #[tokio::test]
    async fn returning_false_stops_the_stream() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n";
        let url = serve_once("200 OK", body, "text/event-stream").await;
        let mut got = String::new();
        run_plan(&reqwest::Client::new(), Provider::OpenAi, local_plan(url), |t| {
            got.push_str(t);
            false
        })
        .await
        .unwrap();
        assert_eq!(got, "a");
    }

    #[tokio::test]
    async fn http_errors_carry_status_and_message() {
        let url = serve_once("401 Unauthorized", r#"{"error":{"message":"Incorrect API key"}}"#, "application/json").await;
        let err = run_plan(&reqwest::Client::new(), Provider::OpenAi, local_plan(url), |_| true)
            .await
            .unwrap_err();
        match err {
            ProviderError::Api { status, message, .. } => {
                assert_eq!(status, 401);
                assert_eq!(message, "Incorrect API key");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
