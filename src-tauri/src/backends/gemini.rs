//! Google Gemini 官方 API（generateContent 流式接口）。
//!
//! 默认值查阅自官方文档（2026-10-27）：
//! - https://ai.google.dev/gemini-api/docs/models ：高频聊天推荐 `gemini-3.5-flash-lite`（最快、最省）。
//! - https://ai.google.dev/gemini-api/docs/pricing ：gemini-3.5-flash-lite 付费价（美元/百万 token）
//!   输入 0.30、输出 2.50、缓存 0.03；有免费额度。
//! - https://ai.google.dev/api/generate-content ：
//!   POST {base}/v1beta/models/{model}:streamGenerateContent?alt=sse
//!   Key 放在请求头 `x-goog-api-key`（**不放在网址里**，避免出现在日志和错误信息中）。
//!   响应：candidates[].content.parts[].text、finishReason、usageMetadata
//!   （promptTokenCount / candidatesTokenCount / cachedContentTokenCount / thoughtsTokenCount）。
//!
//! `reasoning` 设置：非空时作为 generationConfig.thinkingConfig.thinkingLevel 传入（如 low），
//! 降低思考开销；空字符串则用接口默认。

use super::http::{self, read_sse};
use super::*;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub fn kind() -> BackendKind {
    BackendKind {
        kind: "gemini",
        display_name: "Google Gemini",
        needs_key: true,
        default_base_url: "https://generativelanguage.googleapis.com",
        default_model: "gemini-3.5-flash-lite",
        default_reasoning: "low",
        default_price: Price::new(0.30, 0.03, 0.30, 2.50),
        key_url: "https://aistudio.google.com/apikey",
        note: "有免费额度。国内网络通常需要开启代理（DeskMate 会使用系统代理设置）。",
        capabilities: Capabilities {
            streaming: true,
            tool_calls: true,
            agent: false,
            prompt_cache: true,
        },
        create: |cfg, key, client| Box::new(Gemini::new(cfg, key, client)),
    }
}

pub struct Gemini {
    url: String,
    reasoning: String,
    key: Option<String>,
    client: reqwest::Client,
}

impl Gemini {
    pub fn new(cfg: &BackendConfig, key: Option<String>, client: reqwest::Client) -> Self {
        let base = cfg.base_url.trim().trim_end_matches('/');
        Self {
            url: format!(
                "{base}/v1beta/models/{}:streamGenerateContent?alt=sse",
                cfg.model.trim()
            ),
            reasoning: cfg.reasoning.clone(),
            key,
            client,
        }
    }

    pub fn body(&self, req: &ChatRequest) -> Value {
        let contents: Vec<Value> = req
            .messages_for_api()
            .into_iter()
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "model",
                };
                json!({"role": role, "parts": [{"text": m.content}]})
            })
            .collect();
        let mut gen_cfg = json!({"maxOutputTokens": req.max_tokens});
        if let Some(t) = req.temperature {
            gen_cfg["temperature"] = json!(t);
        }
        if !self.reasoning.is_empty() {
            gen_cfg["thinkingConfig"] = json!({"thinkingLevel": self.reasoning});
        }
        let mut body = json!({"contents": contents, "generationConfig": gen_cfg});
        let system = req.system_text();
        if !system.is_empty() {
            body["systemInstruction"] = json!({"parts": [{"text": system}]});
        }
        body
    }
}

/// Gemini 的 Key 错误返回 400（INVALID_ARGUMENT，信息里含 API key），要单独识别
fn classify(status: u16, detail: String) -> BackendError {
    let lower = detail.to_ascii_lowercase();
    if status == 400 && (lower.contains("api key") || lower.contains("api_key")) {
        BackendError::InvalidKey(detail)
    } else {
        BackendError::from_status(status, detail)
    }
}

/// 解析一个响应块，返回 (文本增量, 结束原因, 用量, 被拦截的原因)
pub fn parse_chunk(v: &Value) -> (String, Option<FinishReason>, Option<Usage>, Option<String>) {
    let cand = v.get("candidates").and_then(|c| c.get(0));
    let mut text = String::new();
    if let Some(parts) = cand
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
    {
        for p in parts {
            // thought = true 的是思考摘要，不显示
            if p.get("thought").and_then(|t| t.as_bool()) == Some(true) {
                continue;
            }
            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
            }
        }
    }
    let mut blocked = v
        .get("promptFeedback")
        .and_then(|f| f.get("blockReason"))
        .and_then(|b| b.as_str())
        .map(str::to_string);
    let finish = cand
        .and_then(|c| c.get("finishReason"))
        .and_then(|f| f.as_str())
        .map(|f| match f {
            "MAX_TOKENS" => FinishReason::Length,
            "SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII" | "RECITATION" => {
                blocked.get_or_insert_with(|| f.to_string());
                FinishReason::Stop
            }
            _ => FinishReason::Stop,
        });
    let usage = v.get("usageMetadata").map(|u| {
        let n = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        Usage {
            input_tokens: n("promptTokenCount"),
            // 思考 token 也按输出计费
            output_tokens: n("candidatesTokenCount") + n("thoughtsTokenCount"),
            cached_input_tokens: n("cachedContentTokenCount"),
            cache_write_input_tokens: 0,
        }
    });
    (text, finish, usage, blocked)
}

impl ChatBackend for Gemini {
    fn capabilities(&self) -> Capabilities {
        kind().capabilities
    }

    fn stream_chat<'a>(
        &'a self,
        req: ChatRequest,
        on_delta: DeltaSink<'a>,
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<ChatOutcome, BackendError>> {
        Box::pin(async move {
            let key = self.key.as_deref().ok_or(BackendError::NotConfigured)?;
            let mut headers = HeaderMap::new();
            let mut v = HeaderValue::from_str(key)
                .map_err(|_| BackendError::InvalidKey("Key 里有不支持的字符".into()))?;
            v.set_sensitive(true);
            headers.insert("x-goog-api-key", v);
            let body = self.body(&req);
            let Some(resp) =
                http::post_stream(&self.client, &self.url, headers, &body, &cancel, classify)
                    .await?
            else {
                return Ok(ChatOutcome {
                    text: String::new(),
                    usage: Usage::default(),
                    finish: FinishReason::Cancelled,
                });
            };
            let mut text = String::new();
            let mut finish = FinishReason::Stop;
            let mut usage = Usage::default();
            let mut blocked: Option<String> = None;
            let was_cancelled = read_sse(resp, &cancel, |ev| {
                let data = ev.data.trim();
                if data.is_empty() {
                    return Ok(true);
                }
                let v: Value = serde_json::from_str(data)
                    .map_err(|e| BackendError::Protocol(format!("无法解析流式数据：{e}")))?;
                if let Some(err) = v.get("error") {
                    let code = err.get("code").and_then(|c| c.as_u64()).unwrap_or(500) as u16;
                    return Err(classify(
                        code,
                        http::error_message(&json!({"error": err}).to_string()),
                    ));
                }
                let (d, f, u, b) = parse_chunk(&v);
                if !d.is_empty() {
                    on_delta(&d);
                    text.push_str(&d);
                }
                if let Some(f) = f {
                    finish = f;
                }
                if let Some(u) = u {
                    usage = u;
                }
                if b.is_some() {
                    blocked = b;
                }
                Ok(true)
            })
            .await?;
            if was_cancelled {
                finish = FinishReason::Cancelled;
            } else if text.is_empty()
                && let Some(reason) = blocked
            {
                return Err(BackendError::Refused(format!(
                    "内容被安全策略拦截（{reason}）"
                )));
            }
            Ok(ChatOutcome {
                text,
                usage,
                finish,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::fake_server;
    use super::*;

    fn cfg(base: &str) -> BackendConfig {
        let mut c = BackendConfig::from_kind(&kind());
        c.base_url = base.into();
        c
    }

    fn req() -> ChatRequest {
        ChatRequest {
            system: vec![SystemBlock {
                text: "你是猫".into(),
                cacheable: true,
            }],
            messages: vec![
                ChatMessage::user("你好"),
                ChatMessage::assistant("喵"),
                ChatMessage::user("在干嘛"),
            ],
            context: None,
            max_tokens: 100,
            temperature: None,
        }
    }

    #[test]
    fn builds_request_body_without_key_in_url() {
        let g = Gemini::new(
            &cfg("https://generativelanguage.googleapis.com/"),
            Some("k".into()),
            reqwest::Client::new(),
        );
        assert_eq!(
            g.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.5-flash-lite:streamGenerateContent?alt=sse"
        );
        assert!(!g.url.contains("key="));
        let b = g.body(&req());
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "你是猫");
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["contents"][2]["parts"][0]["text"], "在干嘛");
        assert_eq!(b["generationConfig"]["maxOutputTokens"], 100);
        assert_eq!(
            b["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "low"
        );
    }

    #[test]
    fn parses_chunks_and_skips_thoughts() {
        let v: Value = serde_json::from_str(
            r#"{"candidates":[{"content":{"parts":[{"text":"想一想","thought":true},{"text":"你好"}],"role":"model"},"finishReason":"STOP"}],
               "usageMetadata":{"promptTokenCount":50,"candidatesTokenCount":5,"thoughtsTokenCount":7,"cachedContentTokenCount":20}}"#,
        )
        .unwrap();
        let (t, f, u, b) = parse_chunk(&v);
        assert_eq!(t, "你好");
        assert_eq!(f, Some(FinishReason::Stop));
        assert_eq!(u.unwrap().output_tokens, 12);
        assert_eq!(u.unwrap().cached_input_tokens, 20);
        assert_eq!(b, None);
        let v: Value =
            serde_json::from_str(r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#).unwrap();
        assert_eq!(parse_chunk(&v).3.as_deref(), Some("SAFETY"));
    }

    #[test]
    fn bad_key_400_is_invalid_key() {
        assert_eq!(
            classify(
                400,
                "400 API key not valid. Please pass a valid API key.".into()
            )
            .kind(),
            "invalid_key"
        );
        assert_eq!(classify(400, "400 bad model".into()).kind(), "bad_request");
    }

    #[tokio::test]
    async fn streams_from_fake_server_with_key_in_header() {
        let chunks = vec![
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"喵\"}]}}]}\r\n\r\n".to_string(),
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"呜\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":9,\"candidatesTokenCount\":2}}\r\n\r\n".to_string(),
        ];
        let (base, rx) = fake_server::serve(200, "text/event-stream", chunks).await;
        let g = Gemini::new(
            &cfg(&base),
            Some("test-key-123".into()),
            reqwest::Client::new(),
        );
        let mut got = String::new();
        let mut sink = |s: &str| got.push_str(s);
        let out = g
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.text, "喵呜");
        assert_eq!(got, "喵呜");
        assert_eq!(out.usage.input_tokens, 9);
        let cap = rx.await.unwrap();
        assert!(cap.head.contains("streamGenerateContent?alt=sse"));
        assert!(!cap.head.contains("key=test-key-123"));
        assert!(
            cap.head
                .to_ascii_lowercase()
                .contains("x-goog-api-key: test-key-123")
        );
    }

    #[tokio::test]
    async fn safety_block_without_text_is_refused() {
        let chunks =
            vec!["data: {\"promptFeedback\":{\"blockReason\":\"SAFETY\"}}\n\n".to_string()];
        let (base, _rx) = fake_server::serve(200, "text/event-stream", chunks).await;
        let g = Gemini::new(&cfg(&base), Some("k".into()), reqwest::Client::new());
        let mut sink = |_: &str| {};
        let e = g
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(e.kind(), "refused");
    }
}
