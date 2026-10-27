//! Codex 命令行适配（参数以本机 `codex exec --help` 为准，0.164 查阅）。
//!
//! Codex 的非交互模式不能逐条请求批准，所以：
//! - 默认 `--sandbox read-only`：只能读和分析，不能改任何文件；
//! - 用户在确认卡片上明确勾选「允许修改文件」才用 `workspace-write`，并且：
//!   开始前程序把工作目录完整备份一份（可以回退）、网络强制关闭、只能写工作目录；
//! - `--ignore-user-config` / `--ignore-rules`：不读用户配置，避免其中的宽松设置（比如完全访问）生效；
//! - 任务内容从 stdin 传入（参数里用 `-`）。

use super::{StepKind, TaskStep};
use serde_json::Value;

pub struct CodexArgs<'a> {
    pub workdir: &'a str,
    pub allow_write: bool,
    pub model: &'a str,
    pub last_message_file: &'a str,
}

pub fn build_args(a: &CodexArgs<'_>) -> Vec<String> {
    let mut v: Vec<String> = vec![
        "exec".into(),
        "--json".into(),
        "--skip-git-repo-check".into(),
        "--ignore-user-config".into(),
        "--ignore-rules".into(),
        "--sandbox".into(),
        if a.allow_write {
            "workspace-write"
        } else {
            "read-only"
        }
        .into(),
        "-c".into(),
        "sandbox_workspace_write.network_access=false".into(),
        "--cd".into(),
        a.workdir.into(),
        "--output-last-message".into(),
        a.last_message_file.into(),
    ];
    if !a.model.is_empty() {
        v.push("--model".into());
        v.push(a.model.into());
    }
    // 任务内容从 stdin 读取
    v.push("-".into());
    v
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexResult {
    pub thread_id: String,
    pub last_message: String,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub error: Option<String>,
    pub changed_files: Vec<String>,
}

fn short(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

/// 处理一行 `--json` 输出（JSONL 事件）；字段缺失时尽量容错
pub fn parse_line(line: &str, res: &mut CodexResult) -> Vec<TaskStep> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let mut steps = Vec::new();
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "thread.started" => {
            if let Some(id) = v.get("thread_id").and_then(|s| s.as_str()) {
                res.thread_id = id.to_string();
            }
        }
        "item.started" | "item.completed" => {
            let item = v.get("item").cloned().unwrap_or(Value::Null);
            let it = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let done = ty == "item.completed";
            match it {
                "command_execution" if !done => {
                    let cmd = item.get("command").and_then(|c| c.as_str()).unwrap_or("");
                    steps.push(TaskStep {
                        kind: StepKind::Command,
                        text: format!("执行命令：{}", short(cmd, 120)),
                    });
                }
                "file_change" if done => {
                    if let Some(changes) = item.get("changes").and_then(|c| c.as_array()) {
                        for c in changes {
                            let path = c.get("path").and_then(|p| p.as_str()).unwrap_or("");
                            let kind = c.get("kind").and_then(|k| k.as_str()).unwrap_or("update");
                            let verb = match kind {
                                "add" => "新建",
                                "delete" => "删除",
                                _ => "修改",
                            };
                            steps.push(TaskStep {
                                kind: StepKind::Edit,
                                text: format!("{verb} {path}"),
                            });
                            if !res.changed_files.iter().any(|f| f == path) {
                                res.changed_files.push(path.to_string());
                            }
                        }
                    }
                }
                "agent_message" if done => {
                    let t = item.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    res.last_message = t.to_string();
                    steps.push(TaskStep {
                        kind: StepKind::Note,
                        text: short(t.trim(), 160),
                    });
                }
                "web_search" if !done => {
                    steps.push(TaskStep {
                        kind: StepKind::Network,
                        text: "联网搜索".into(),
                    });
                }
                _ => {}
            }
        }
        "turn.completed" => {
            let n = |k: &str| {
                v.pointer(&format!("/usage/{k}"))
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0)
            };
            res.input_tokens += n("input_tokens");
            res.cached_input_tokens += n("cached_input_tokens");
            res.output_tokens += n("output_tokens");
        }
        "turn.failed" | "error" => {
            let msg = v
                .pointer("/error/message")
                .or_else(|| v.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            res.error = Some(short(msg, 300));
        }
        _ => {}
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_by_default_and_network_off() {
        let a = build_args(&CodexArgs {
            workdir: "D:\\W",
            allow_write: false,
            model: "",
            last_message_file: "o.txt",
        });
        let s = a.join(" ");
        assert!(s.contains("--sandbox read-only"));
        assert!(s.contains("--ignore-user-config"));
        assert!(s.contains("network_access=false"));
        assert!(!s.contains("danger"));
        assert_eq!(a.last().unwrap(), "-", "任务内容走 stdin");
        let a = build_args(&CodexArgs {
            workdir: "D:\\W",
            allow_write: true,
            model: "gpt-x",
            last_message_file: "o.txt",
        });
        let s = a.join(" ");
        assert!(s.contains("--sandbox workspace-write"));
        assert!(s.contains("--model gpt-x"));
    }

    #[test]
    fn parses_jsonl_events() {
        let mut r = CodexResult::default();
        parse_line(r#"{"type":"thread.started","thread_id":"th-1"}"#, &mut r);
        let s = parse_line(
            r#"{"type":"item.started","item":{"type":"command_execution","command":"ls -la"}}"#,
            &mut r,
        );
        assert_eq!(s[0].kind, StepKind::Command);
        let s = parse_line(
            r#"{"type":"item.completed","item":{"type":"file_change","changes":[{"path":"a.txt","kind":"add"},{"path":"b.txt","kind":"delete"}]}}"#,
            &mut r,
        );
        assert_eq!(s.len(), 2);
        assert!(s[1].text.starts_with("删除"));
        parse_line(
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"完成了"}}"#,
            &mut r,
        );
        parse_line(
            r#"{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":200,"output_tokens":50}}"#,
            &mut r,
        );
        assert_eq!(r.thread_id, "th-1");
        assert_eq!(r.last_message, "完成了");
        assert_eq!(r.changed_files, vec!["a.txt", "b.txt"]);
        assert_eq!((r.input_tokens, r.output_tokens), (1000, 50));
        parse_line(
            r#"{"type":"turn.failed","error":{"message":"quota exceeded"}}"#,
            &mut r,
        );
        assert_eq!(r.error.as_deref(), Some("quota exceeded"));
    }
}
