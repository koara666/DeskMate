//! 通用「OpenAI 兼容接口」后端：POST {base_url}/chat/completions，SSE 流式。
//! DeepSeek、很多国产模型、本地 Ollama / LM Studio 都能直接用它接入。
//!
//! `reasoning` 设置：
//! - 空：不传任何思考参数（用接口默认）
//! - `disabled`：传 `thinking: {type: "disabled"}`（DeepSeek 关闭思考模式的写法）
//! - 其他值（low / high / max…）：传 `reasoning_effort`

use super::http::{self, read_sse};
use super::*;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub fn kind() -> BackendKind {
    BackendKind {
        kind: "openai_compat",
        display_name: "OpenAI 兼容接口（自定义）",
        needs_key: true,
        default_base_url: "https://api.example.com/v1",
        default_model: "model-name",
        default_reasoning: "",
        default_price: Price::new(0.0, 0.0, 0.0, 0.0),
        key_url: "",
        note: "适用于兼容 OpenAI Chat Completions 格式的服务。请按服务商文档填写接口地址、模型名和价格。",
        capabilities: Capabilities {
            streaming: true,
            tool_calls: false,
            agent: false,
            prompt_cache: false,
        },
        create: |cfg, key, client| Box::new(OpenAiCompat::new(cfg, key, client)),
    }
}

pub struct OpenAiCompat {
    pub url: String,
    pub model: String,
    pub reasoning: String,
    key: Option<String>,
    client: reqwest::Client,
    pub classify: fn(u16, String) -> BackendError,
}

impl OpenAiCompat {
    pub fn new(cfg: &BackendConfig, key: Option<String>, client: reqwest::Client) -> Self {
        Self {
            url: format!(
                "{}/chat/completions",
                cfg.base_url.trim().trim_end_matches('/')
            ),
            model: cfg.model.trim().to_string(),
            reasoning: cfg.reasoning.clone(),
            key,
            client,
            classify: BackendError::from_status,
        }
    }

    pub fn body(&self, req: &ChatRequest) -> Value {
        let mut messages = Vec::new();
        let system = req.system_text();
        if !system.is_empty() {
            messages.push(json!({"role": "system", "content": system}));
        }
        for m in req.messages_for_api() {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            messages.push(json!({"role": role, "content": m.content}));
        }
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": req.max_tokens,
        });
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        match self.reasoning.as_str() {
            "" => {}
            "disabled" => body["thinking"] = json!({"type": "disabled"}),
            effort => body["reasoning_effort"] = json!(effort),
        }
        body
    }
}

/// 解析一个流式数据块，返回 (文本增量, 结束原因, 用量)
pub fn parse_chunk(v: &Value) -> (Option<String>, Option<FinishReason>, Option<Usage>) {
    let choice = v.get("choices").and_then(|c| c.get(0));
    let text = choice
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let finish = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .map(|f| match f {
            "length" => FinishReason::Length,
            _ => FinishReason::Stop,
        });
    let usage = v.get("usage").filter(|u| u.is_object()).map(|u| {
        let n = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        // DeepSeek 用 prompt_cache_hit_tokens；OpenAI 用 prompt_tokens_details.cached_tokens
        let cached = u
            .get("prompt_cache_hit_tokens")
            .and_then(|x| x.as_u64())
            .or_else(|| {
                u.get("prompt_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(|x| x.as_u64())
            })
            .unwrap_or(0);
        Usage {
            input_tokens: n("prompt_tokens"),
            output_tokens: n("completion_tokens"),
            cached_input_tokens: cached,
            cache_write_input_tokens: 0,
        }
    });
    (text, finish, usage)
}

impl ChatBackend for OpenAiCompat {
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
            let mut auth = HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| BackendError::InvalidKey("Key 里有不支持的字符".into()))?;
            auth.set_sensitive(true);
            headers.insert(AUTHORIZATION, auth);
            let body = self.body(&req);
            let Some(resp) = http::post_stream(
                &self.client,
                &self.url,
                headers,
                &body,
                &cancel,
                self.classify,
            )
            .await?
            else {
                return Ok(cancelled(String::new()));
            };
            let mut text = String::new();
            let mut finish = FinishReason::Stop;
            let mut usage = Usage::default();
            let was_cancelled = read_sse(resp, &cancel, |ev| {
                let data = ev.data.trim();
                if data == "[DONE]" {
                    return Ok(false);
                }
                if data.is_empty() {
                    return Ok(true);
                }
                let v: Value = serde_json::from_str(data)
                    .map_err(|e| BackendError::Protocol(format!("无法解析流式数据：{e}")))?;
                if let Some(err) = v.get("error") {
                    return Err(BackendError::Server {
                        status: 200,
                        detail: http::error_message(&json!({"error": err}).to_string()),
                    });
                }
                let (delta, f, u) = parse_chunk(&v);
                if let Some(d) = delta {
                    on_delta(&d);
                    text.push_str(&d);
                }
                if let Some(f) = f {
                    finish = f;
                }
                if let Some(u) = u {
                    usage = u;
                }
                Ok(true)
            })
            .await?;
            if was_cancelled {
                finish = FinishReason::Cancelled;
            }
            Ok(ChatOutcome {
                text,
                usage,
                finish,
            })
        })
    }
}

fn cancelled(text: String) -> ChatOutcome {
    ChatOutcome {
        text,
        usage: Usage::default(),
        finish: FinishReason::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::super::http::fake_server;
    use super::*;

    fn cfg(base: &str) -> BackendConfig {
        BackendConfig {
            id: "t".into(),
            kind: "openai_compat".into(),
            name: "t".into(),
            base_url: base.into(),
            model: "m-1".into(),
            reasoning: "disabled".into(),
            price: Price::default(),
        }
    }

    fn req() -> ChatRequest {
        ChatRequest {
            system: vec![SystemBlock {
                text: "你是猫".into(),
                cacheable: true,
            }],
            messages: vec![ChatMessage::user("你好")],
            context: Some("现在是晚上".into()),
            max_tokens: 100,
            temperature: None,
        }
    }

    #[test]
    fn builds_request_body() {
        let b = OpenAiCompat::new(
            &cfg("https://x/v1/"),
            Some("k".into()),
            reqwest::Client::new(),
        );
        assert_eq!(b.url, "https://x/v1/chat/completions");
        let body = b.body(&req());
        assert_eq!(body["model"], "m-1");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "你是猫");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(
            body["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("现在是晚上")
        );
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn parses_chunks() {
        let v: Value = serde_json::from_str(
            r#"{"choices":[{"delta":{"content":"你"},"finish_reason":null}]}"#,
        )
        .unwrap();
        assert_eq!(parse_chunk(&v), (Some("你".into()), None, None));
        let v: Value =
            serde_json::from_str(r#"{"choices":[{"delta":{"reasoning_content":"想"}}]}"#).unwrap();
        assert_eq!(parse_chunk(&v).0, None, "思考内容不显示");
        let v: Value = serde_json::from_str(
            r#"{"choices":[{"delta":{},"finish_reason":"length"}],"usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_cache_hit_tokens":64,"prompt_cache_miss_tokens":36}}"#,
        )
        .unwrap();
        let (_, f, u) = parse_chunk(&v);
        assert_eq!(f, Some(FinishReason::Length));
        assert_eq!(
            u.unwrap(),
            Usage {
                input_tokens: 100,
                output_tokens: 20,
                cached_input_tokens: 64,
                cache_write_input_tokens: 0
            }
        );
    }

    #[tokio::test]
    async fn streams_from_fake_server() {
        let chunks = vec![
            "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n\n".to_string(),
            "data: {\"choices\":[{\"delta\":{\"content\":\"好呀\"}}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":3}}\n\n".to_string(),
            "data: [DONE]\n\n".to_string(),
        ];
        let (base, rx) = fake_server::serve(200, "text/event-stream", chunks).await;
        let key = ["sk-", "test0000000000000000"].concat();
        let b = OpenAiCompat::new(&cfg(&base), Some(key.clone()), reqwest::Client::new());
        let mut got = String::new();
        let mut sink = |s: &str| got.push_str(s);
        let out = b
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(got, "你好呀");
        assert_eq!(out.text, "你好呀");
        assert_eq!(out.finish, FinishReason::Stop);
        assert_eq!(out.usage.input_tokens, 10);
        let cap = rx.await.unwrap();
        assert!(cap.head.starts_with("POST /chat/completions"));
        assert!(
            cap.head
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {key}").to_ascii_lowercase())
        );
        assert!(cap.body.contains("\"stream\":true"));
    }

    #[tokio::test]
    async fn maps_http_errors() {
        for (status, kind) in [
            (401, "invalid_key"),
            (402, "insufficient_balance"),
            (429, "rate_limited"),
            (503, "server"),
        ] {
            let (base, _rx) = fake_server::serve(
                status,
                "application/json",
                vec![r#"{"error":{"message":"nope"}}"#.to_string()],
            )
            .await;
            let b = OpenAiCompat::new(&cfg(&base), Some("k".into()), reqwest::Client::new());
            let mut sink = |_: &str| {};
            let e = b
                .stream_chat(req(), &mut sink, CancellationToken::new())
                .await
                .unwrap_err();
            assert_eq!(e.kind(), kind, "状态码 {status}");
            assert!(e.to_string().contains("nope"));
        }
    }

    #[tokio::test]
    async fn missing_key_is_not_configured() {
        let b = OpenAiCompat::new(&cfg("https://x"), None, reqwest::Client::new());
        let mut sink = |_: &str| {};
        let e = b
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(e, BackendError::NotConfigured);
    }

    #[tokio::test]
    async fn network_failure_is_network_error() {
        // 端口 9 一般没有服务，连接会被拒绝
        let b = OpenAiCompat::new(
            &cfg("http://127.0.0.1:9"),
            Some("k".into()),
            reqwest::Client::new(),
        );
        let mut sink = |_: &str| {};
        let e = b
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(
            matches!(e, BackendError::Network(_) | BackendError::Timeout),
            "{e:?}"
        );
    }
}
