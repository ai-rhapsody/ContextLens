use crate::media::{self, Media};
use crate::settings::Provider;
use crate::sse::SseDecoder;
use futures_util::StreamExt;
use serde_json::{json, Value};

/// A later message in a conversation, after the first user message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub user: bool,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct ChatRequest {
    pub system: String,
    pub user: String,
    /// Images, audio or video sent with the user turn.
    pub media: Vec<Media>,
    /// Let the model search the web (Gemini: Google Search; Claude: web search;
    /// OpenAI: only its search models).
    pub web_search: bool,
    /// A quick job (translation): ask reasoning models to think briefly, for speed.
    pub quick: bool,
    /// Follow-up messages after the first user message (answers and further questions).
    pub turns: Vec<Turn>,
}

/// Claude models of the 5 generation take `output_config.effort` and the newer web search tool;
/// older ones reject the first and lack the second.
fn claude_generation_5(model: &str) -> bool {
    ["claude-opus-5", "claude-sonnet-5", "claude-haiku-5", "claude-fable-5", "claude-mythos-5"]
        .iter()
        .any(|p| model.starts_with(p))
}

/// Web search tool version: the dynamic-filtering one where the model supports it.
fn claude_web_search_type(model: &str) -> &'static str {
    let dynamic = ["claude-opus-5", "claude-sonnet-5", "claude-opus-4-6", "claude-opus-4-7", "claude-opus-4-8", "claude-sonnet-4-6"];
    if dynamic.iter().any(|p| model.starts_with(p)) {
        "web_search_20260209"
    } else {
        "web_search_20250305"
    }
}

/// Models that offer server-side refusal fallback ("default" routing).
fn claude_has_fallbacks(model: &str) -> bool {
    ["claude-opus-5-5", "claude-sonnet-5-5", "claude-fable-5-1", "claude-opus-5"].iter().any(|p| model.starts_with(p))
}

/// OpenAI reasoning models accept `reasoning_effort`; older chat models reject it.
fn openai_reasons(model: &str) -> bool {
    ["gpt-5", "gpt-6", "o1", "o3", "o4"].iter().any(|p| model.starts_with(p)) && !model.contains("chat")
}

/// One piece of a streamed answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delta {
    Text(String),
    /// A web page the answer drew on.
    Source { title: String, url: String },
}

/// Whether a model can search the web through its provider's API.
pub fn can_search(provider: Provider, model: &str) -> bool {
    match provider {
        Provider::Gemini | Provider::Anthropic => true,
        Provider::OpenAi => model.contains("search"),
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("{}", describe_network(.0))]
    Network(#[from] reqwest::Error),
    #[error("{}", describe_api(provider, *status, message))]
    Api {
        provider: &'static str,
        status: u16,
        message: String,
    },
    #[error("{0}")]
    Stream(String),
    #[error("{0}")]
    Unsupported(String),
}

impl ProviderError {
    /// Too many requests, or a quota (often the free tier's) used up.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, ProviderError::Api { status: 429, .. })
    }

    /// The service was busy or failed on its side (503 "overloaded" is common on free keys):
    /// the same request may well work a moment later.
    pub fn is_transient(&self) -> bool {
        matches!(self, ProviderError::Api { status, .. } if (500..600).contains(status))
    }
}

/// An error sent inside the stream, as an API error when it says which kind (so it reads
/// like one, in Chinese with what to do), else as the service's own words.
fn stream_error(provider: Provider, error: &Value) -> ProviderError {
    let message = error.get("message").and_then(Value::as_str).unwrap_or_default().to_string();
    // Gemini and OpenAI put the HTTP status in `code`; Anthropic names the kind in `type`.
    let status = error.get("code").and_then(Value::as_u64).map(|c| c as u16).or_else(|| {
        match error.get("type").and_then(Value::as_str)? {
            "overloaded_error" => Some(529),
            "api_error" | "server_error" => Some(500),
            "rate_limit_error" => Some(429),
            _ => None,
        }
    });
    match status {
        Some(status) if status >= 400 => ProviderError::Api { provider: provider.id(), status, message },
        _ if message.is_empty() => ProviderError::Stream(format!("{} 串流錯誤", provider.display_name())),
        _ => ProviderError::Stream(message),
    }
}

/// An API error in plain language with what to do, followed by the service's own words.
pub fn describe_api(provider: &str, status: u16, message: &str) -> String {
    let name = match provider {
        "gemini" => "Gemini",
        "openai" => "ChatGPT",
        "anthropic" => "Claude",
        other => other,
    };
    let m = message.to_lowercase();
    let what = if m.contains("api key") && (m.contains("invalid") || m.contains("not valid") || m.contains("incorrect")) || status == 401 {
        format!("{name} 的 API 金鑰無效。到「模型與金鑰」重新貼上，再按「測試」。")
    } else if status == 429 {
        format!(
            "{name} 回報請求太頻繁或額度已用完。免費金鑰的額度很低，等一分鐘再試；\
             常用的話，到 {name} 的後台確認額度和帳單，或在「模型與金鑰」改用其他服務。"
        )
    } else if status == 403 {
        format!("這組 {name} 金鑰沒有權限使用這個模型或功能（有些功能需要付費方案）。到「模型與金鑰」換一個模型，或確認帳號的方案。")
    } else if status == 404 || m.contains("not found") {
        format!("{name} 找不到這個模型。到「模型與金鑰」從清單選一個模型。")
    } else if status == 413 || m.contains("too large") || m.contains("too long") {
        "內容太大，模型收不下。截小一點的範圍，或用比較短的檔案再試。".to_string()
    } else if (500..600).contains(&status) || m.contains("overloaded") {
        format!("{name} 的伺服器暫時忙碌或出錯，稍後再試，或在「模型與金鑰」暫時改用其他服務。")
    } else if status == 400 {
        format!("{name} 不接受這個請求，可能是模型名稱不對，或模型不支援這種內容。")
    } else {
        format!("{name} 回報錯誤。")
    };
    format!("{what}（{status}：{message}）")
}

/// The innermost cause of a connection failure, with a plain-language hint. reqwest's own
/// message ("error sending request for url") says nothing about why.
pub fn describe_network(e: &reqwest::Error) -> String {
    let mut root = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        root = s.to_string();
        src = s.source();
    }
    let hint = network_hint(&root);
    if hint.is_empty() {
        format!("連線失敗：{root}")
    } else {
        format!("連線失敗：{hint}（{root}）")
    }
}

pub fn network_hint(cause: &str) -> &'static str {
    let c = cause.to_lowercase();
    if c.contains("certificate") || c.contains("unknownissuer") {
        "無法驗證伺服器的憑證。常見原因是防毒軟體的 HTTPS 掃描或公司網路攔截了加密連線；請確認系統時間正確，或在防毒軟體中排除 ContextLens"
    } else if c.contains("dns") || c.contains("resolve") || c.contains("no such host") || c.contains("lookup") {
        "找不到伺服器，請檢查網路連線"
    } else if c.contains("timed out") || c.contains("timeout") {
        "連線逾時，請檢查網路，或稍後再試"
    } else if c.contains("refused") || c.contains("reset") || c.contains("forcibly closed") {
        "連線被拒絕或中斷，可能是防火牆或 Proxy 擋住了"
    } else {
        ""
    }
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
        Provider::OpenAi => {
            let mut body = json!({
                "model": model,
                "stream": true,
                "messages": std::iter::once(json!({ "role": "system", "content": req.system }))
                    .chain(std::iter::once(json!({ "role": "user", "content": openai_content(req) })))
                    .chain(req.turns.iter().map(|t| json!({ "role": if t.user { "user" } else { "assistant" }, "content": t.text })))
                    .collect::<Vec<_>>(),
            });
            if req.web_search && can_search(provider, model) {
                body["web_search_options"] = json!({});
            }
            if req.quick && openai_reasons(model) {
                body["reasoning_effort"] = json!("low");
            }
            HttpPlan {
                url: "https://api.openai.com/v1/chat/completions".into(),
                headers: vec![("authorization", format!("Bearer {api_key}"))],
                body,
            }
        }
        Provider::Anthropic => {
            let mut body = json!({
                "model": model,
                "stream": true,
                "max_tokens": 4096,
                "system": req.system,
                "messages": std::iter::once(json!({ "role": "user", "content": anthropic_content(req) }))
                    .chain(req.turns.iter().map(|t| json!({ "role": if t.user { "user" } else { "assistant" }, "content": t.text })))
                    .collect::<Vec<_>>(),
            });
            if req.web_search {
                body["tools"] = json!([{ "type": claude_web_search_type(model), "name": "web_search", "max_uses": 3 }]);
            }
            if claude_generation_5(model) {
                body["output_config"] = json!({ "effort": if req.quick { "low" } else { "medium" } });
            }
            let mut headers = vec![("x-api-key", api_key.to_string()), ("anthropic-version", "2023-06-01".into())];
            // If the model declines, the API reruns the request on a suitable fallback model.
            if claude_has_fallbacks(model) {
                body["fallbacks"] = json!("default");
                headers.push(("anthropic-beta", "server-side-fallback-2026-07-01".into()));
            }
            HttpPlan { url: "https://api.anthropic.com/v1/messages".into(), headers, body }
        }
        Provider::Gemini => {
            let mut body = json!({
                "systemInstruction": { "parts": [{ "text": req.system }] },
                "contents": std::iter::once(json!({ "role": "user", "parts": gemini_parts(req) }))
                    .chain(req.turns.iter().map(|t| json!({ "role": if t.user { "user" } else { "model" }, "parts": [{ "text": t.text }] })))
                    .collect::<Vec<_>>(),
            });
            if req.web_search {
                body["tools"] = json!([{ "google_search": {} }]);
            }
            HttpPlan {
                url: format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse"
                ),
                headers: vec![("x-goog-api-key", api_key.to_string())],
                body,
            }
        }
    }
}

fn openai_content(req: &ChatRequest) -> Value {
    if req.media.is_empty() {
        return json!(req.user);
    }
    let mut parts = vec![json!({ "type": "text", "text": req.user })];
    for m in &req.media {
        let url = format!("data:{};base64,{}", m.mime, m.data_b64);
        parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
    }
    Value::Array(parts)
}

fn anthropic_content(req: &ChatRequest) -> Value {
    if req.media.is_empty() {
        return json!(req.user);
    }
    // Anthropic recommends images before the text that refers to them.
    let mut parts: Vec<Value> = req
        .media
        .iter()
        .map(|m| json!({ "type": "image", "source": { "type": "base64", "media_type": m.mime, "data": m.data_b64 } }))
        .collect();
    parts.push(json!({ "type": "text", "text": req.user }));
    Value::Array(parts)
}

fn gemini_parts(req: &ChatRequest) -> Value {
    let mut parts: Vec<Value> = req
        .media
        .iter()
        .map(|m| json!({ "inlineData": { "mimeType": m.mime, "data": m.data_b64 } }))
        .collect();
    parts.push(json!({ "text": req.user }));
    Value::Array(parts)
}

fn source(title: Option<&str>, url: Option<&str>) -> Option<Delta> {
    let url = url?.to_string();
    let title = title.filter(|t| !t.is_empty()).map_or_else(|| url.clone(), String::from);
    Some(Delta::Source { title, url })
}

/// Reads one SSE `data:` payload and returns what it adds: text, and the sources of a web search.
pub fn extract_delta(provider: Provider, data: &str) -> Result<Vec<Delta>, ProviderError> {
    if data.trim() == "[DONE]" {
        return Ok(vec![]);
    }
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => return Ok(vec![]), // keep-alives and other non-JSON lines
    };
    let mut out = Vec::new();
    match provider {
        Provider::OpenAi => {
            if let Some(e) = v.get("error") {
                return Err(stream_error(provider, e));
            }
            let delta = v.pointer("/choices/0/delta");
            if let Some(t) = delta.and_then(|d| d.get("content")).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                out.push(Delta::Text(t.into()));
            }
            for a in delta.and_then(|d| d.get("annotations")).and_then(Value::as_array).into_iter().flatten() {
                let c = a.get("url_citation");
                out.extend(source(
                    c.and_then(|c| c.get("title")).and_then(Value::as_str),
                    c.and_then(|c| c.get("url")).and_then(Value::as_str),
                ));
            }
        }
        Provider::Anthropic => match v.get("type").and_then(Value::as_str) {
            Some("content_block_delta")
                if v.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") =>
            {
                out.extend(v.pointer("/delta/text").and_then(Value::as_str).map(|t| Delta::Text(t.into())));
            }
            Some("content_block_start")
                if v.pointer("/content_block/type").and_then(Value::as_str) == Some("web_search_tool_result") =>
            {
                for r in v.pointer("/content_block/content").and_then(Value::as_array).into_iter().flatten() {
                    out.extend(source(
                        r.get("title").and_then(Value::as_str),
                        r.get("url").and_then(Value::as_str),
                    ));
                }
            }
            // Claude cites the pages it used inside the text as well.
            Some("content_block_delta")
                if v.pointer("/delta/type").and_then(Value::as_str) == Some("citations_delta") =>
            {
                let c = v.pointer("/delta/citation");
                out.extend(source(
                    c.and_then(|c| c.get("title")).and_then(Value::as_str),
                    c.and_then(|c| c.get("url")).and_then(Value::as_str),
                ));
            }
            Some("error") => return Err(stream_error(provider, v.get("error").unwrap_or(&Value::Null))),
            Some("message_delta") if v.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("refusal") => {
                return Err(ProviderError::Stream("Claude 婉拒處理這段內容。可以改用其他服務試試。".into()));
            }
            _ => {}
        },
        Provider::Gemini => {
            if let Some(e) = v.get("error") {
                return Err(stream_error(provider, e));
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
            if !text.is_empty() {
                out.push(Delta::Text(text));
            }
            let chunks = v.pointer("/candidates/0/groundingMetadata/groundingChunks").and_then(Value::as_array);
            for c in chunks.into_iter().flatten() {
                let web = c.get("web");
                out.extend(source(
                    web.and_then(|w| w.get("title")).and_then(Value::as_str),
                    web.and_then(|w| w.get("uri")).and_then(Value::as_str),
                ));
            }
        }
    }
    Ok(out)
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

/// Streams a completion, calling `on_delta` with each piece of text or source.
/// Returning `false` from `on_delta` stops the stream early (a newer request replaced this one).
pub async fn stream_completion(
    client: &reqwest::Client,
    provider: Provider,
    model: &str,
    api_key: &str,
    req: &ChatRequest,
    on_delta: impl FnMut(Delta) -> bool,
) -> Result<(), ProviderError> {
    for m in &req.media {
        media::check(provider, model, m).map_err(ProviderError::Unsupported)?;
    }
    run_plan(client, provider, plan(provider, model, api_key, req), on_delta).await
}

pub async fn run_plan(
    client: &reqwest::Client,
    provider: Provider,
    plan: HttpPlan,
    mut on_delta: impl FnMut(Delta) -> bool,
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
            for d in extract_delta(provider, &data)? {
                if !on_delta(d) {
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
        ChatRequest { system: "sys".into(), user: "usr".into(), media: vec![], web_search: false, quick: false, turns: vec![] }
    }

    fn req_with_image() -> ChatRequest {
        let media = vec![Media { mime: "image/png".into(), data_b64: "QUJD".into(), name: None }];
        ChatRequest { media, ..req() }
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
    fn follow_up_turns_use_each_providers_roles() {
        let r = ChatRequest {
            turns: vec![Turn { user: false, text: "答".into() }, Turn { user: true, text: "再問".into() }],
            ..req()
        };
        let o = plan(Provider::OpenAi, "m", "K", &r).body;
        assert_eq!(o["messages"].as_array().unwrap().len(), 4);
        assert_eq!((o["messages"][2]["role"].as_str(), o["messages"][3]["content"].as_str()), (Some("assistant"), Some("再問")));
        let a = plan(Provider::Anthropic, "m", "K", &r).body;
        assert_eq!(a["messages"][1]["role"], "assistant");
        assert_eq!(a["messages"][2]["content"], "再問");
        let g = plan(Provider::Gemini, "m", "K", &r).body;
        assert_eq!(g["contents"][1]["role"], "model");
        assert_eq!(g["contents"][2]["parts"][0]["text"], "再問");
    }

    #[test]
    fn text_only_requests_keep_plain_string_content() {
        assert_eq!(plan(Provider::OpenAi, "m", "K", &req()).body["messages"][1]["content"], "usr");
        assert_eq!(plan(Provider::Anthropic, "m", "K", &req()).body["messages"][0]["content"], "usr");
    }

    #[test]
    fn images_use_each_providers_shape() {
        let r = req_with_image();
        let o = plan(Provider::OpenAi, "m", "K", &r).body;
        assert_eq!(o["messages"][1]["content"][1]["image_url"]["url"], "data:image/png;base64,QUJD");

        let a = plan(Provider::Anthropic, "m", "K", &r).body;
        let first = &a["messages"][0]["content"][0];
        assert_eq!(first["type"], "image");
        assert_eq!(first["source"]["media_type"], "image/png");
        assert_eq!(a["messages"][0]["content"][1]["text"], "usr");

        let g = plan(Provider::Gemini, "m", "K", &r).body;
        assert_eq!(g["contents"][0]["parts"][0]["inlineData"]["mimeType"], "image/png");
        assert_eq!(g["contents"][0]["parts"][1]["text"], "usr");
    }

    #[tokio::test]
    async fn unsupported_media_fails_before_any_request() {
        let mut r = req_with_image();
        r.media[0].mime = "video/mp4".into();
        let err = stream_completion(&reqwest::Client::new(), Provider::Anthropic, "m", "K", &r, |_| true)
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Unsupported(_)));
    }

    /// The text of one payload, as the old single-text API returned it.
    fn text(provider: Provider, data: &str) -> Option<String> {
        let t: String = extract_delta(provider, data)
            .unwrap()
            .into_iter()
            .filter_map(|d| match d {
                Delta::Text(t) => Some(t),
                Delta::Source { .. } => None,
            })
            .collect();
        (!t.is_empty()).then_some(t)
    }

    #[test]
    fn claude_5_gets_effort_new_search_tool_and_fallbacks() {
        let quick = ChatRequest { quick: true, web_search: true, ..req() };
        let opus = plan(Provider::Anthropic, "claude-opus-5-5", "K", &quick);
        assert_eq!(opus.body["output_config"]["effort"], "low");
        assert_eq!(opus.body["tools"][0]["type"], "web_search_20260209");
        assert_eq!(opus.body["fallbacks"], "default");
        assert!(opus.headers.contains(&("anthropic-beta", "server-side-fallback-2026-07-01".to_string())));
        let haiku = plan(Provider::Anthropic, "claude-haiku-5-5", "K", &ChatRequest { web_search: true, ..req() });
        assert_eq!(haiku.body["output_config"]["effort"], "medium");
        assert_eq!(haiku.body["tools"][0]["type"], "web_search_20250305");
        assert!(haiku.body.get("fallbacks").is_none());
        let old = plan(Provider::Anthropic, "claude-haiku-4-5", "K", &quick);
        assert!(old.body.get("output_config").is_none());
    }

    #[test]
    fn openai_reasoning_effort_only_for_reasoning_models() {
        let quick = ChatRequest { quick: true, ..req() };
        assert_eq!(plan(Provider::OpenAi, "gpt-6-luna", "K", &quick).body["reasoning_effort"], "low");
        assert!(plan(Provider::OpenAi, "gpt-4o", "K", &quick).body.get("reasoning_effort").is_none());
        assert!(plan(Provider::OpenAi, "gpt-6-luna", "K", &req()).body.get("reasoning_effort").is_none());
    }

    #[test]
    fn claude_refusals_become_errors() {
        let d = r#"{"type":"message_delta","delta":{"stop_reason":"refusal"}}"#;
        assert!(extract_delta(Provider::Anthropic, d).is_err());
        let ok = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#;
        assert!(extract_delta(Provider::Anthropic, ok).unwrap().is_empty());
    }

    #[test]
    fn web_search_adds_each_providers_tool() {
        let r = ChatRequest { web_search: true, ..req() };
        let g = plan(Provider::Gemini, "m", "K", &r).body;
        assert!(g["tools"][0]["google_search"].is_object());
        let a = plan(Provider::Anthropic, "m", "K", &r).body;
        assert_eq!(a["tools"][0]["type"], "web_search_20250305");
        // Only OpenAI's search models take the option; others would reject it.
        assert!(plan(Provider::OpenAi, "gpt-5-mini", "K", &r).body.get("web_search_options").is_none());
        assert!(plan(Provider::OpenAi, "gpt-4o-search-preview", "K", &r).body["web_search_options"].is_object());
        assert!(plan(Provider::Gemini, "m", "K", &req()).body.get("tools").is_none());
        assert!(can_search(Provider::Anthropic, "x") && !can_search(Provider::OpenAi, "gpt-5-mini"));
    }

    #[test]
    fn sources_are_read_from_each_provider() {
        let g = r#"{"candidates":[{"content":{"parts":[{"text":"答"}]},"groundingMetadata":{"groundingChunks":[{"web":{"uri":"https://a.example","title":"A"}}]}}]}"#;
        assert_eq!(
            extract_delta(Provider::Gemini, g).unwrap(),
            vec![Delta::Text("答".into()), Delta::Source { title: "A".into(), url: "https://a.example".into() }]
        );
        let a = r#"{"type":"content_block_start","index":1,"content_block":{"type":"web_search_tool_result","content":[{"type":"web_search_result","url":"https://b.example","title":"B"}]}}"#;
        assert_eq!(extract_delta(Provider::Anthropic, a).unwrap(), vec![Delta::Source { title: "B".into(), url: "https://b.example".into() }]);
        let o = r#"{"choices":[{"delta":{"annotations":[{"type":"url_citation","url_citation":{"url":"https://c.example","title":""}}]}}]}"#;
        assert_eq!(extract_delta(Provider::OpenAi, o).unwrap(), vec![Delta::Source { title: "https://c.example".into(), url: "https://c.example".into() }]);
    }

    #[test]
    fn openai_deltas() {
        let d = r#"{"choices":[{"delta":{"content":"你好"}}]}"#;
        assert_eq!(text(Provider::OpenAi, d).as_deref(), Some("你好"));
        assert!(extract_delta(Provider::OpenAi, "[DONE]").unwrap().is_empty());
        let role_only = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert!(extract_delta(Provider::OpenAi, role_only).unwrap().is_empty());
    }

    #[test]
    fn anthropic_deltas_and_errors() {
        let d = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#;
        assert_eq!(text(Provider::Anthropic, d).as_deref(), Some("Hi"));
        let ping = r#"{"type":"ping"}"#;
        assert!(extract_delta(Provider::Anthropic, ping).unwrap().is_empty());
        let err = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(extract_delta(Provider::Anthropic, err).is_err());
    }

    #[test]
    fn gemini_deltas_skip_thoughts_and_report_blocks() {
        let d = r#"{"candidates":[{"content":{"parts":[{"text":"想","thought":true},{"text":"答"},{"text":"案"}]}}]}"#;
        assert_eq!(text(Provider::Gemini, d).as_deref(), Some("答案"));
        let blocked = r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#;
        assert!(extract_delta(Provider::Gemini, blocked).is_err());
    }

    #[test]
    fn errors_inside_the_stream_read_like_api_errors() {
        let g = r#"{"error":{"code":503,"message":"The model is overloaded. Please try again later.","status":"UNAVAILABLE"}}"#;
        let e = extract_delta(Provider::Gemini, g).unwrap_err();
        assert!(e.is_transient());
        assert!(e.to_string().starts_with("Gemini 的伺服器暫時忙碌"), "{e}");
        let a = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(extract_delta(Provider::Anthropic, a).unwrap_err().is_transient());
        let o = r#"{"error":{"message":"Something odd","type":"invalid_request_error","code":null}}"#;
        let e = extract_delta(Provider::OpenAi, o).unwrap_err();
        assert!(!e.is_transient());
        assert_eq!(e.to_string(), "Something odd");
    }

    #[test]
    fn claude_citations_are_sources() {
        let c = r#"{"type":"content_block_delta","index":2,"delta":{"type":"citations_delta","citation":{"type":"web_search_result_location","url":"https://d.example","title":"D","cited_text":"x"}}}"#;
        assert_eq!(extract_delta(Provider::Anthropic, c).unwrap(), vec![Delta::Source { title: "D".into(), url: "https://d.example".into() }]);
    }

    #[test]
    fn network_hints_name_the_likely_cause() {
        assert!(network_hint("invalid peer certificate: UnknownIssuer").contains("憑證"));
        assert!(network_hint("dns error: failed to lookup address").contains("找不到"));
        assert!(network_hint("operation timed out").contains("逾時"));
        assert_eq!(network_hint("something else"), "");
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
        run_plan(&reqwest::Client::new(), Provider::OpenAi, local_plan(url), |d| {
            if let Delta::Text(t) = d {
                got.push_str(&t);
            }
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
        run_plan(&reqwest::Client::new(), Provider::OpenAi, local_plan(url), |d| {
            if let Delta::Text(t) = d {
                got.push_str(&t);
            }
            false
        })
        .await
        .unwrap();
        assert_eq!(got, "a");
    }

    #[test]
    fn api_errors_say_what_to_do_in_chinese() {
        let e = ProviderError::Api { provider: "gemini", status: 429, message: "Resource has been exhausted".into() };
        assert!(e.is_rate_limited());
        let m = e.to_string();
        assert!(m.contains("額度") && m.contains("Resource has been exhausted"), "{m}");
        assert!(describe_api("openai", 401, "Incorrect API key provided").contains("金鑰無效"));
        assert!(describe_api("anthropic", 529, "Overloaded").contains("伺服器"));
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
