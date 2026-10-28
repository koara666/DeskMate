//! Claude API（Anthropic Messages API，原始 HTTP 调用——Rust 没有官方 SDK）。
//!
//! 默认值查阅自官方资料（2026-10-27，claude-api 参考文档）：
//! - POST {base}/v1/messages，请求头 `x-api-key`、`anthropic-version: 2023-06-01`。
//! - 日常聊天用最便宜、最快的 `claude-haiku-5-5`：输入 0.10、输出 0.50 美元/百万 token
//!   （提示不超过 10 万 token 时）；缓存读取 0.1 倍、5 分钟缓存写入 1.25 倍。
//! - Haiku 5.5 不接受非默认的 temperature（会返回 400），所以不传。
//!   思考默认开启（自适应），用 `output_config.effort` 控制，聊天用 `low`。
//! - 提示缓存：在稳定的系统段落末尾加 `cache_control: {type: "ephemeral"}`；
//!   Haiku 5.5 最短可缓存前缀 512 token。
//! - SSE 事件：message_start（含输入用量）→ content_block_start/delta/stop（text_delta 是正文，
//!   thinking 块不显示）→ message_delta（stop_reason、累计 output_tokens）→ message_stop；
//!   中途可能出现 `error` 事件（如 overloaded_error）。
//! - 错误码：401 Key 无效、402 账单问题、403 无权限、404 模型不存在、429 限流、500/529 服务端问题。
//!
//! `reasoning` 设置：非空时作为 output_config.effort 传入（low / medium / high …）。

use super::http::{self, read_sse};
use super::*;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub fn kind() -> BackendKind {
    BackendKind {
        kind: "anthropic",
        display_name: "Claude API",
        needs_key: true,
        default_base_url: "https://api.anthropic.com",
        default_model: "claude-haiku-5-5",
        default_reasoning: "low",
        default_price: Price::new(0.10, 0.01, 0.125, 0.50),
        key_url: "https://platform.claude.com/settings/keys",
        note: "使用 Anthropic API Key（按量付费，和 Claude 订阅是两回事）。国内网络通常需要开启代理。",
        capabilities: Capabilities {
            streaming: true,
            tool_calls: true,
            agent: false,
            prompt_cache: true,
        },
        create: |cfg, key, client| Box::new(Claude::new(cfg, key, client)),
    }
}

pub struct Claude {
    url: String,
    model: String,
    effort: String,
    key: Option<String>,
    client: reqwest::Client,
}

impl Claude {
    pub fn new(cfg: &BackendConfig, key: Option<String>, client: reqwest::Client) -> Self {
        Self {
            url: format!("{}/v1/messages", cfg.base_url.trim().trim_end_matches('/')),
            model: cfg.model.trim().to_string(),
            effort: cfg.reasoning.clone(),
            key,
            client,
        }
    }

    pub fn body(&self, req: &ChatRequest) -> Value {
        // 每个可缓存的系统段落末尾打一个缓存断点（最多 4 个）
        let mut breakpoints = 0;
        let system: Vec<Value> = req
            .system
            .iter()
            .map(|b| {
                let mut v = json!({"type": "text", "text": b.text});
                if b.cacheable && breakpoints < 3 {
                    breakpoints += 1;
                    v["cache_control"] = json!({"type": "ephemeral"});
                }
                v
            })
            .collect();
        let messages: Vec<Value> = req
            .messages_for_api()
            .into_iter()
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                match &m.image {
                    Some(img) => json!({"role": role, "content": [
                        {"type": "image", "source": {"type": "base64", "media_type": img.mime, "data": img.base64}},
                        {"type": "text", "text": m.content},
                    ]}),
                    None => json!({"role": role, "content": m.content}),
                }
            })
            .collect();
        let mut body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "stream": true,
            "messages": messages,
        });
        if !system.is_empty() {
            body["system"] = json!(system);
        }
        if !self.effort.is_empty() {
            body["output_config"] = json!({"effort": self.effort});
        }
        body
    }
}

fn classify(status: u16, detail: String) -> BackendError {
    match status {
        404 => BackendError::BadRequest { status, detail },
        529 => BackendError::Server { status, detail },
        _ => BackendError::from_status(status, detail),
    }
}

/// 流式状态累积器（纯逻辑，方便测试）
#[derive(Debug, Default)]
pub struct StreamState {
    pub text: String,
    pub usage: Usage,
    pub finish: Option<FinishReason>,
    pub refused: bool,
}

impl StreamState {
    /// 处理一个事件；返回 (要显示的文本增量, 是否结束)
    pub fn handle(&mut self, v: &Value) -> Result<(Option<String>, bool), BackendError> {
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "message_start" => {
                if let Some(u) = v.get("message").and_then(|m| m.get("usage")) {
                    let n = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                    let read = n("cache_read_input_tokens");
                    let write = n("cache_creation_input_tokens");
                    self.usage.cached_input_tokens = read;
                    self.usage.cache_write_input_tokens = write;
                    // input_tokens 只是未缓存部分，这里换算成全部输入
                    self.usage.input_tokens = n("input_tokens") + read + write;
                    self.usage.output_tokens = n("output_tokens");
                }
                Ok((None, false))
            }
            "content_block_delta" => {
                let d = v.get("delta");
                let is_text =
                    d.and_then(|d| d.get("type")).and_then(|t| t.as_str()) == Some("text_delta");
                let t = d
                    .and_then(|d| d.get("text"))
                    .and_then(|t| t.as_str())
                    .filter(|_| is_text);
                match t {
                    Some(t) if !t.is_empty() => {
                        self.text.push_str(t);
                        Ok((Some(t.to_string()), false))
                    }
                    _ => Ok((None, false)), // thinking_delta / signature_delta 等不显示
                }
            }
            "message_delta" => {
                if let Some(o) = v
                    .get("usage")
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|x| x.as_u64())
                {
                    self.usage.output_tokens = o;
                }
                if let Some(r) = v
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|r| r.as_str())
                {
                    self.finish = Some(match r {
                        "max_tokens" => FinishReason::Length,
                        "refusal" => {
                            self.refused = true;
                            FinishReason::Stop
                        }
                        _ => FinishReason::Stop,
                    });
                }
                Ok((None, false))
            }
            "message_stop" => Ok((None, true)),
            "error" => {
                let e = v.get("error").cloned().unwrap_or(Value::Null);
                let et = e.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let msg = http::error_message(&json!({"error": e}).to_string());
                Err(match et {
                    "overloaded_error" => BackendError::Server {
                        status: 529,
                        detail: msg,
                    },
                    "rate_limit_error" => BackendError::RateLimited(msg),
                    _ => BackendError::Server {
                        status: 500,
                        detail: msg,
                    },
                })
            }
            _ => Ok((None, false)), // ping、content_block_start/stop
        }
    }
}

impl ChatBackend for Claude {
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
            headers.insert("x-api-key", v);
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
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
            let mut st = StreamState::default();
            let was_cancelled = read_sse(resp, &cancel, |ev| {
                let data = ev.data.trim();
                if data.is_empty() {
                    return Ok(true);
                }
                let v: Value = serde_json::from_str(data)
                    .map_err(|e| BackendError::Protocol(format!("无法解析流式数据：{e}")))?;
                let (delta, done) = st.handle(&v)?;
                if let Some(d) = delta {
                    on_delta(&d);
                }
                Ok(!done)
            })
            .await?;
            if st.refused && st.text.is_empty() {
                return Err(BackendError::Refused("模型出于安全原因没有回答".into()));
            }
            let finish = if was_cancelled {
                FinishReason::Cancelled
            } else {
                st.finish.unwrap_or(FinishReason::Stop)
            };
            Ok(ChatOutcome {
                text: st.text,
                usage: st.usage,
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
            system: vec![
                SystemBlock {
                    text: "规则和角色".into(),
                    cacheable: true,
                },
                SystemBlock {
                    text: "用户档案".into(),
                    cacheable: true,
                },
            ],
            messages: vec![ChatMessage::user("你好")],
            context: Some("当前时间：早上".into()),
            max_tokens: 300,
            temperature: Some(0.7),
        }
    }

    #[test]
    fn builds_request_body_with_cache_breakpoints() {
        let c = Claude::new(
            &cfg("https://api.anthropic.com"),
            Some("k".into()),
            reqwest::Client::new(),
        );
        assert_eq!(c.url, "https://api.anthropic.com/v1/messages");
        let b = c.body(&req());
        assert_eq!(b["model"], "claude-haiku-5-5");
        assert_eq!(b["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(b["system"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(b["output_config"]["effort"], "low");
        assert!(
            b.get("temperature").is_none(),
            "Haiku 5.5 不接受自定义 temperature"
        );
        assert!(
            b["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("当前时间：早上")
        );
    }

    #[test]
    fn stream_state_accumulates() {
        let mut st = StreamState::default();
        let ev = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        st.handle(&ev(r#"{"type":"message_start","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":500,"cache_creation_input_tokens":0,"output_tokens":1}}}"#)).unwrap();
        st.handle(&ev(r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#)).unwrap();
        assert_eq!(st.handle(&ev(r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"嗯"}}"#)).unwrap().0, None);
        assert_eq!(
            st.handle(&ev(r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"你好"}}"#)).unwrap().0.as_deref(),
            Some("你好")
        );
        st.handle(&ev(r#"{"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":42}}"#)).unwrap();
        assert!(st.handle(&ev(r#"{"type":"message_stop"}"#)).unwrap().1);
        assert_eq!(st.text, "你好");
        assert_eq!(st.finish, Some(FinishReason::Length));
        assert_eq!(
            st.usage,
            Usage {
                input_tokens: 510,
                output_tokens: 42,
                cached_input_tokens: 500,
                cache_write_input_tokens: 0
            }
        );
        let e = st
            .handle(&ev(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            ))
            .unwrap_err();
        assert_eq!(e.kind(), "server");
    }

    #[tokio::test]
    async fn streams_from_fake_server() {
        let sse = [
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n",
            "event: ping\ndata: {\"type\":\"ping\"}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"早上好\"}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ];
        let (base, rx) = fake_server::serve(
            200,
            "text/event-stream",
            sse.iter().map(|s| s.to_string()).collect(),
        )
        .await;
        let c = Claude::new(&cfg(&base), Some("test-key".into()), reqwest::Client::new());
        let mut got = String::new();
        let mut sink = |s: &str| got.push_str(s);
        let out = c
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.text, "早上好");
        assert_eq!(got, "早上好");
        assert_eq!(out.usage.output_tokens, 5);
        let cap = rx.await.unwrap();
        let head = cap.head.to_ascii_lowercase();
        assert!(head.starts_with("post /v1/messages"));
        assert!(head.contains("x-api-key: test-key"));
        assert!(head.contains("anthropic-version: 2023-06-01"));
    }

    #[tokio::test]
    async fn refusal_without_text_is_error() {
        let sse = vec![
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"},\"usage\":{\"output_tokens\":0}}\n\ndata: {\"type\":\"message_stop\"}\n\n".to_string(),
        ];
        let (base, _rx) = fake_server::serve(200, "text/event-stream", sse).await;
        let c = Claude::new(&cfg(&base), Some("k".into()), reqwest::Client::new());
        let mut sink = |_: &str| {};
        let e = c
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(e.kind(), "refused");
    }
}
