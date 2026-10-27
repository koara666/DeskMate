//! mock 后端：本地假数据，开发和自动测试一律用它，不花一分钱。
//!
//! 用户消息以 `/mock ` 开头时可以模拟各种情况，方便调界面：
//! - `/mock error key|balance|rate|network|timeout|server|bad` 模拟错误
//! - `/mock long` 一段很长的回复（测试滚动）
//! - `/mock slow` 很慢的回复（测试停止生成）

use super::*;
use std::time::Duration;

pub const KIND: BackendKind = BackendKind {
    kind: "mock",
    display_name: "Mock（本地测试，不联网）",
    needs_key: false,
    default_base_url: "",
    default_model: "mock-1",
    capabilities: Capabilities {
        streaming: true,
        tool_calls: false,
        agent: false,
        prompt_cache: false,
    },
};

pub struct MockBackend {
    /// 每段输出之间的间隔（测试时设为 0）
    pub chunk_delay: Duration,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self {
            chunk_delay: Duration::from_millis(35),
        }
    }
}

impl MockBackend {
    pub fn instant() -> Self {
        Self {
            chunk_delay: Duration::ZERO,
        }
    }
}

fn mock_error(arg: &str) -> Option<BackendError> {
    Some(match arg {
        "key" => BackendError::InvalidKey("401 Unauthorized（mock）".into()),
        "balance" => BackendError::InsufficientBalance("402 Insufficient Balance（mock）".into()),
        "rate" => BackendError::RateLimited("429 Too Many Requests（mock）".into()),
        "network" => BackendError::Network("无法连接到服务器（mock）".into()),
        "timeout" => BackendError::Timeout,
        "server" => BackendError::Server {
            status: 503,
            detail: "Service Unavailable（mock）".into(),
        },
        "bad" => BackendError::BadRequest {
            status: 400,
            detail: "Model Not Exist（mock）".into(),
        },
        _ => return None,
    })
}

/// 根据请求生成假回复
pub fn reply_for(req: &ChatRequest) -> Result<(String, Duration), BackendError> {
    let last = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.trim())
        .unwrap_or("");
    if let Some(cmd) = last.strip_prefix("/mock ") {
        let mut parts = cmd.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some("error"), Some(arg)) => {
                if let Some(e) = mock_error(arg) {
                    return Err(e);
                }
            }
            (Some("long"), _) => {
                let mut s = String::new();
                for i in 1..=30 {
                    s.push_str(&format!(
                        "这是第 {i} 行测试文字，用来检查聊天面板能不能正常滚动。\n"
                    ));
                }
                return Ok((s, Duration::ZERO));
            }
            (Some("slow"), _) => {
                return Ok((
                    "我会慢慢地说……一个字一个字地说……你可以随时点「停止」打断我。这句话会说很久很久很久很久很久。".into(),
                    Duration::from_millis(300),
                ));
            }
            _ => {}
        }
    }
    let short: String = last.chars().take(40).collect();
    let reply = if short.is_empty() {
        "嗯？你想说什么呀～".to_string()
    } else {
        format!(
            "（mock 模式）你刚才说的是「{short}」对吧？我现在用的是本地测试模型，不会联网也不会花钱。去「设置 → 模型」填好 Key 之后，我就能真正陪你聊天啦！"
        )
    };
    Ok((reply, Duration::ZERO))
}

impl ChatBackend for MockBackend {
    fn capabilities(&self) -> Capabilities {
        KIND.capabilities
    }

    fn stream_chat<'a>(
        &'a self,
        req: ChatRequest,
        on_delta: DeltaSink<'a>,
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<ChatOutcome, BackendError>> {
        Box::pin(async move {
            let (reply, extra_delay) = reply_for(&req)?;
            let delay = if extra_delay.is_zero() {
                self.chunk_delay
            } else if self.chunk_delay.is_zero() {
                Duration::ZERO
            } else {
                extra_delay
            };
            let input_tokens: usize = req
                .system
                .iter()
                .map(|b| estimate_tokens(&b.text))
                .chain(req.messages.iter().map(|m| estimate_tokens(&m.content)))
                .sum();
            let mut out = String::new();
            let chars: Vec<char> = reply.chars().collect();
            for chunk in chars.chunks(2) {
                if cancel.is_cancelled() {
                    return Ok(outcome(out, input_tokens, FinishReason::Cancelled));
                }
                if !delay.is_zero() {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            return Ok(outcome(out, input_tokens, FinishReason::Cancelled));
                        }
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                let piece: String = chunk.iter().collect();
                on_delta(&piece);
                out.push_str(&piece);
            }
            Ok(outcome(out, input_tokens, FinishReason::Stop))
        })
    }
}

fn outcome(text: String, input_tokens: usize, finish: FinishReason) -> ChatOutcome {
    let output_tokens = estimate_tokens(&text) as u64;
    ChatOutcome {
        text,
        usage: Usage {
            input_tokens: input_tokens as u64,
            output_tokens,
            cached_input_tokens: 0,
        },
        finish,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str) -> ChatRequest {
        ChatRequest {
            system: vec![SystemBlock {
                text: "你是测试角色".into(),
                cacheable: true,
            }],
            messages: vec![ChatMessage::user(text)],
            max_tokens: 256,
            temperature: None,
        }
    }

    #[tokio::test]
    async fn streams_full_reply() {
        let b = MockBackend::instant();
        let mut got = String::new();
        let mut n = 0;
        let mut sink = |s: &str| {
            got.push_str(s);
            n += 1;
        };
        let out = b
            .stream_chat(req("你好"), &mut sink, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out.finish, FinishReason::Stop);
        assert_eq!(out.text, got);
        assert!(got.contains("你好"));
        assert!(n > 1, "应该分多段流式输出");
        assert!(out.usage.input_tokens > 0 && out.usage.output_tokens > 0);
    }

    #[tokio::test]
    async fn cancellation_returns_partial() {
        let b = MockBackend {
            chunk_delay: Duration::from_millis(5),
        };
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        let mut count = 0;
        let mut sink = |_: &str| {
            count += 1;
            if count == 3 {
                c2.cancel();
            }
        };
        let out = b.stream_chat(req("你好"), &mut sink, cancel).await.unwrap();
        assert_eq!(out.finish, FinishReason::Cancelled);
        assert_eq!(out.text.chars().count(), 6);
    }

    #[tokio::test]
    async fn simulated_errors() {
        let b = MockBackend::instant();
        for (arg, kind) in [
            ("key", "invalid_key"),
            ("balance", "insufficient_balance"),
            ("network", "network"),
            ("timeout", "timeout"),
        ] {
            let mut sink = |_: &str| {};
            let e = b
                .stream_chat(
                    req(&format!("/mock error {arg}")),
                    &mut sink,
                    CancellationToken::new(),
                )
                .await
                .unwrap_err();
            assert_eq!(e.kind(), kind);
        }
    }
}
