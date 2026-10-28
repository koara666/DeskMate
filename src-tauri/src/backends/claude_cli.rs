//! 「Claude Code（订阅额度）」聊天后端：通过本机官方 `claude` 命令行聊天，用的是用户自己的 Claude 订阅。
//!
//! 规定（需求第 6 节）：订阅额度只能通过官方命令行使用；DeskMate 不读取、不转发它的登录信息。
//! 每条回复启动一次 `claude -p`（比 API 慢几秒），消耗订阅里的 Agent SDK 额度。
//!
//! 只聊天、不干活：
//! - `--tools ""` 关掉全部工具（不能读写文件、执行命令、联网）；
//! - `--restricted`、`--strict-mcp-config`、`--disable-slash-commands`：不读用户/项目设置，不加载 MCP 和技能；
//! - `--no-session-persistence`：不在本机留会话记录（聊天记录由 DeskMate 自己的记忆系统管理）；
//! - `--system-prompt`：用 DeskMate 组装的角色设定替换 Claude Code 自带的编程助手提示词；
//! - 在一个空的临时目录里运行，避免读到任何项目里的 CLAUDE.md。
//!
//! 参数以本机 `claude --help`（2.1.296）为准。

use super::*;
use crate::tasks::{self, Executor, RunEnd};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

pub fn kind() -> BackendKind {
    BackendKind {
        kind: "claude_code",
        display_name: "Claude Code（用订阅额度）",
        needs_key: false,
        default_base_url: "",
        default_model: "sonnet",
        default_reasoning: "low",
        // 订阅不按 token 收费；这里按 API 价格估算消耗，用于花费上限保护订阅额度
        default_price: Price::new(2.0, 0.2, 2.5, 10.0),
        key_url: "",
        note: "用你自己的 Claude 订阅聊天：需要先在命令行里运行一次 claude 完成登录。每条回复都会启动一次 claude，比 API 慢几秒；消耗订阅里单独的 Agent SDK 额度（花费按 API 价格估算，计入花费上限）。只聊天，不能读写文件或执行命令。",
        capabilities: Capabilities {
            streaming: true,
            tool_calls: false,
            agent: false,
            prompt_cache: true,
        },
        create: |cfg, _key, _client| Box::new(ClaudeCli::new(cfg)),
    }
}

pub struct ClaudeCli {
    /// 手动指定的 claude 程序位置（空 = 自动查找）
    program: String,
    model: String,
    effort: String,
    /// 测试用：直接指定要启动的程序
    pub override_program: Option<tasks::Program>,
}

impl ClaudeCli {
    pub fn new(cfg: &BackendConfig) -> Self {
        Self {
            program: cfg.base_url.trim().to_string(),
            model: cfg.model.trim().to_string(),
            effort: cfg.reasoning.trim().to_string(),
            override_program: None,
        }
    }
}

/// 纯聊天模式的参数
pub fn build_args(system: &str, model: &str, effort: &str) -> Vec<String> {
    let mut v: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--tools",
        "",
        "--restricted",
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--no-session-persistence",
        "--system-prompt",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.push(format!(
        "{system}\n\n你现在在纯聊天模式下运行：没有任何工具，不能读写文件、执行命令或联网。"
    ));
    if !model.is_empty() {
        v.push("--model".into());
        v.push(model.into());
    }
    if !effort.is_empty() {
        v.push("--effort".into());
        v.push(effort.into());
    }
    v
}

/// `claude -p` 只接受一段输入：把历史对话和本轮消息拼成一段文本（从 stdin 传入）
pub fn build_prompt(req: &ChatRequest) -> String {
    let msgs = req.messages_for_api();
    if msgs.len() <= 1 {
        return msgs.last().map(|m| m.content.clone()).unwrap_or_default();
    }
    let mut s = String::from("下面是你和用户之前的对话（按时间顺序，只是背景）：\n");
    for m in &msgs[..msgs.len() - 1] {
        let who = match m.role {
            Role::User => "用户",
            Role::Assistant => "你",
        };
        s.push_str(&format!("{who}：{}\n", m.content.trim()));
    }
    s.push_str("\n用户现在说：\n");
    s.push_str(&msgs[msgs.len() - 1].content);
    s
}

/// Windows 命令行总长度上限约 32000 字符：系统提示词太长时，改为放在输入内容的开头
const MAX_SYSTEM_ARG_CHARS: usize = 16000;

pub fn split_system(system: &str, prompt: String) -> (String, String) {
    if system.chars().count() <= MAX_SYSTEM_ARG_CHARS {
        return (system.to_string(), prompt);
    }
    (
        "请严格按照输入开头【角色设定】里的要求扮演角色、和用户聊天。".to_string(),
        format!("【角色设定】\n{system}\n【角色设定结束】\n\n{prompt}"),
    )
}

#[derive(Debug, Default, PartialEq)]
pub struct CliState {
    pub text: String,
    pub got_partial: bool,
    pub result_seen: bool,
    pub is_error: bool,
    pub error_text: String,
    pub usage: Usage,
}

/// 处理一行输出，返回要显示的增量
pub fn parse_line(line: &str, st: &mut CliState) -> Option<String> {
    let v: Value = serde_json::from_str(line).ok()?;
    match v.get("type").and_then(|t| t.as_str())? {
        "stream_event" => {
            let ev = v.get("event")?;
            if ev.get("type").and_then(|t| t.as_str()) == Some("content_block_delta")
                && ev.pointer("/delta/type").and_then(|t| t.as_str()) == Some("text_delta")
            {
                let t = ev
                    .pointer("/delta/text")
                    .and_then(|t| t.as_str())?
                    .to_string();
                st.got_partial = true;
                st.text.push_str(&t);
                return Some(t);
            }
            None
        }
        // 没有逐字增量时（旧版本），用完整的助手消息兜底
        "assistant" if !st.got_partial => {
            let mut out = String::new();
            for it in v.pointer("/message/content")?.as_array()? {
                if it.get("type").and_then(|t| t.as_str()) == Some("text") {
                    out.push_str(it.get("text").and_then(|t| t.as_str()).unwrap_or(""));
                }
            }
            if out.is_empty() {
                return None;
            }
            st.text.push_str(&out);
            Some(out)
        }
        "result" => {
            st.result_seen = true;
            st.is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
            if st.is_error {
                st.error_text = v
                    .get("result")
                    .and_then(|r| r.as_str())
                    .unwrap_or("")
                    .to_string();
            }
            let n = |k: &str| {
                v.pointer(&format!("/usage/{k}"))
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0)
            };
            let read = n("cache_read_input_tokens");
            let write = n("cache_creation_input_tokens");
            st.usage = Usage {
                input_tokens: n("input_tokens") + read + write,
                output_tokens: n("output_tokens"),
                cached_input_tokens: read,
                cache_write_input_tokens: write,
            };
            None
        }
        _ => None,
    }
}

fn classify_error(text: &str) -> BackendError {
    let lower = text.to_lowercase();
    let detail = crate::redact::redact_secrets(&text.chars().take(300).collect::<String>());
    if lower.contains("external api key") {
        // DeskMate 已经不传环境变量里的 Key（tasks::SCRUBBED_ENV），还出现说明 Key 写在 Claude Code 自己的设置里
        BackendError::Cli(format!(
            "Claude Code 用的是另外配置的 API Key，不是订阅登录，而且这个 Key 无效。请检查 %USERPROFILE%\\.claude\\settings.json 里的 apiKeyHelper 或 env 设置：{detail}"
        ))
    } else if lower.contains("login")
        || lower.contains("api key")
        || lower.contains("auth")
        || lower.contains("credential")
    {
        BackendError::Cli(format!("还没登录或登录已过期：{detail}"))
    } else if lower.contains("limit") || lower.contains("quota") || lower.contains("credit") {
        BackendError::RateLimited(format!("订阅额度可能用完了：{detail}"))
    } else {
        BackendError::Cli(detail)
    }
}

fn work_dir() -> PathBuf {
    let d = std::env::temp_dir().join("DeskMate-claude-chat");
    let _ = std::fs::create_dir_all(&d);
    d
}

impl ChatBackend for ClaudeCli {
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
            let prog = match &self.override_program {
                Some(p) => p.clone(),
                None => tasks::locate(Executor::Claude, &self.program)
                    .ok_or_else(|| BackendError::Cli("没有找到 claude 程序".into()))?,
            };
            let (system, prompt) = split_system(&req.system_text(), build_prompt(&req));
            let args = build_args(&system, &self.model, &self.effort);
            let mut st = CliState::default();
            let (end, stderr) = tasks::run_process(
                &prog,
                &args,
                &work_dir(),
                &prompt,
                Duration::from_secs(180),
                &cancel,
                |line| {
                    if let Some(d) = parse_line(line, &mut st) {
                        on_delta(&d);
                    }
                },
            )
            .await;
            let finish = match end {
                RunEnd::Cancelled => FinishReason::Cancelled,
                RunEnd::TimedOut => return Err(BackendError::Timeout),
                RunEnd::SpawnFailed(e) => {
                    return Err(BackendError::Cli(format!("启动失败：{e}")));
                }
                RunEnd::Exited { .. } => FinishReason::Stop,
            };
            if finish == FinishReason::Stop {
                if st.is_error {
                    return Err(classify_error(&st.error_text));
                }
                if !st.result_seen && st.text.is_empty() {
                    return Err(classify_error(&stderr));
                }
            }
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
    use super::*;

    fn req() -> ChatRequest {
        ChatRequest {
            system: vec![SystemBlock {
                text: "你是奶龙".into(),
                cacheable: true,
            }],
            messages: vec![
                ChatMessage::user("在吗"),
                ChatMessage::assistant("在呀"),
                ChatMessage::user("讲个笑话"),
            ],
            context: Some("当前时间：晚上".into()),
            max_tokens: 300,
            temperature: None,
        }
    }

    #[test]
    fn args_are_chat_only() {
        let a = build_args("你是奶龙", "sonnet", "low");
        let s = a.join(" ");
        let i = a.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a[i + 1], "", "关闭全部工具");
        for f in [
            "--restricted",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
        ] {
            assert!(a.contains(&f.to_string()), "{f}");
        }
        assert!(s.contains("--model sonnet") && s.contains("--effort low"));
        assert!(!s.contains("dangerously") && !s.contains("bypass"));
        assert!(a.iter().any(|x| x.starts_with("你是奶龙")));
    }

    #[test]
    fn prompt_contains_history_and_context() {
        let p = build_prompt(&req());
        assert!(p.contains("用户：在吗"));
        assert!(p.contains("你：在呀"));
        assert!(p.contains("当前时间：晚上"));
        assert!(p.trim_end().ends_with("讲个笑话"));
    }

    #[test]
    fn long_system_prompt_moves_to_stdin() {
        let (sys, p) = split_system("短设定", "你好".into());
        assert_eq!((sys.as_str(), p.as_str()), ("短设定", "你好"));
        let long = "设".repeat(MAX_SYSTEM_ARG_CHARS + 1);
        let (sys, p) = split_system(&long, "你好".into());
        assert!(sys.chars().count() < 100);
        assert!(p.starts_with("【角色设定】") && p.ends_with("你好"));
    }

    #[test]
    fn parses_partial_stream_and_result() {
        let mut st = CliState::default();
        assert_eq!(
            parse_line(r#"{"type":"system","subtype":"init"}"#, &mut st),
            None
        );
        assert_eq!(
            parse_line(r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"从前"}}}"#, &mut st).as_deref(),
            Some("从前")
        );
        // 有逐字增量时，完整消息不再重复输出
        assert_eq!(
            parse_line(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"从前有座山"}]}}"#,
                &mut st
            ),
            None
        );
        parse_line(
            r#"{"type":"result","is_error":false,"result":"从前","usage":{"input_tokens":10,"cache_read_input_tokens":90,"output_tokens":5}}"#,
            &mut st,
        );
        assert!(st.result_seen && !st.is_error);
        assert_eq!(st.usage.input_tokens, 100);
        assert_eq!(st.text, "从前");
    }

    #[test]
    fn falls_back_to_full_message_without_partials() {
        let mut st = CliState::default();
        assert_eq!(
            parse_line(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"你好"}]}}"#,
                &mut st
            )
            .as_deref(),
            Some("你好")
        );
    }

    #[test]
    fn error_classification() {
        assert_eq!(
            classify_error("Invalid API key · Please run /login").kind(),
            "cli"
        );
        assert_eq!(
            classify_error("Claude AI usage limit reached").kind(),
            "rate_limited"
        );
        assert_eq!(classify_error("something else").kind(), "cli");
        let e = classify_error("Invalid API key · Fix external API key").to_string();
        assert!(e.contains("不是订阅登录"), "{e}");
    }

    #[tokio::test]
    async fn streams_from_fake_cli() {
        // 用 node 模拟一个 claude 命令行：读 stdin，输出 stream-json
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-claude.js");
        std::fs::write(
            &script,
            r#"let input = ''; process.stdin.on('data', d => input += d); process.stdin.on('end', () => {
  const ok = process.argv.includes('--restricted') && input.includes('讲个笑话');
  const out = (o) => console.log(JSON.stringify(o));
  out({ type: 'system', subtype: 'init' });
  for (const t of ok ? ['哈', '哈哈'] : ['参数不对']) out({ type: 'stream_event', event: { type: 'content_block_delta', delta: { type: 'text_delta', text: t } } });
  out({ type: 'result', is_error: false, result: 'x', usage: { input_tokens: 20, output_tokens: 3 } });
});"#,
        )
        .unwrap();
        let mut b = ClaudeCli::new(&BackendConfig::from_kind(&kind()));
        b.override_program = Some(tasks::Program {
            exe: PathBuf::from("node"),
            pre_args: vec![script.display().to_string()],
        });
        let mut got = String::new();
        let mut sink = |s: &str| got.push_str(s);
        let out = b
            .stream_chat(req(), &mut sink, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(got, "哈哈哈");
        assert_eq!(out.text, "哈哈哈");
        assert_eq!(out.usage.output_tokens, 3);
    }
}
