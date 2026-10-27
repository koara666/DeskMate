//! Claude Code 命令行适配（参数以本机 `claude --help` 为准，2.1.296 查阅）。
//!
//! 安全设计：
//! - `-p` 非交互；任务内容从 stdin 传入（不出现在命令行里）。
//! - `--restricted`：去掉会执行命令/代码的工具和网页抓取（除非 `--tools` 点名），
//!   文件工具只能在工作目录内，忽略用户/项目设置文件，拒绝 bypassPermissions。
//! - `--tools` 点名 Bash / WebFetch / WebSearch，但不自动批准：配合 `--permission-prompts none`，
//!   任何需要确认的操作都会被自动拒绝，并出现在结果的 `permission_denials` 里。
//! - `--permission-mode acceptEdits`：工作目录内的读写自动通过（这是任务本身要做的）。
//! - 用户在界面上逐条批准被拒绝的操作后，用 `--resume <会话>` 继续，并用 `--allowedTools`
//!   只放行这几条精确的命令 / 网址。
//! - `--max-budget-usd`：单个任务花费上限。

use super::{ApprovalItem, StepKind, TaskStep};
use serde_json::Value;

/// 追加给执行 agent 的规则
pub const EXECUTOR_RULES: &str = "You are running on behalf of DeskMate, a desktop companion app, in non-interactive mode. \
Work only inside the current working directory. Some actions (running shell commands, deleting files, network access, \
reading or writing outside the working directory, installing software) will be denied automatically until the user approves them. \
If an action is denied, do NOT try to work around it with other tools; finish what you can, then clearly list the denied actions you still need. \
Never read or print credentials, API keys or password files. Reply in the user's language (usually Chinese).";

pub const BASE_TOOLS: &str = "Read,Glob,Grep,Edit,Write,Bash,WebFetch,WebSearch";

pub struct ClaudeArgs<'a> {
    pub budget_usd: f64,
    pub model: &'a str,
    pub resume_session: Option<&'a str>,
    /// 用户批准的 `--allowedTools` 规则
    pub allowed: &'a [String],
    /// 用户批准访问的额外目录
    pub add_dirs: &'a [String],
}

pub fn build_args(a: &ClaudeArgs<'_>) -> Vec<String> {
    let mut v: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--restricted",
        "--tools",
        BASE_TOOLS,
        "--permission-mode",
        "acceptEdits",
        "--permission-prompts",
        "none",
        "--append-system-prompt",
        EXECUTOR_RULES,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.push("--max-budget-usd".into());
    v.push(format!("{:.2}", a.budget_usd.max(0.01)));
    if !a.model.is_empty() {
        v.push("--model".into());
        v.push(a.model.into());
    }
    if let Some(s) = a.resume_session {
        v.push("--resume".into());
        v.push(s.into());
    }
    for d in a.add_dirs {
        v.push("--add-dir".into());
        v.push(d.clone());
    }
    if !a.allowed.is_empty() {
        // 可变参数放在最后，避免吞掉后面的选项
        v.push("--allowedTools".into());
        v.extend(a.allowed.iter().cloned());
    }
    v
}

/// 会话 id 只允许 uuid 一类的安全字符
pub fn valid_session_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn short(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

/// 把一次工具调用描述成给用户看的一句话
pub fn describe_tool(name: &str, input: &Value) -> (StepKind, String) {
    let get = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    match name {
        "Read" => (StepKind::Read, format!("读取 {}", get("file_path"))),
        "Glob" | "Grep" => (
            StepKind::Read,
            format!(
                "查找 {}",
                short(&format!("{} {}", get("pattern"), get("path")), 80)
            ),
        ),
        "Edit" | "MultiEdit" => (StepKind::Edit, format!("修改 {}", get("file_path"))),
        "Write" => (StepKind::Edit, format!("写入 {}", get("file_path"))),
        "Bash" | "PowerShell" => (
            StepKind::Command,
            format!("执行命令：{}", short(get("command"), 120)),
        ),
        "WebFetch" => (StepKind::Network, format!("访问网页：{}", get("url"))),
        "WebSearch" => (StepKind::Network, format!("联网搜索：{}", get("query"))),
        other => (StepKind::Other, format!("使用工具 {other}")),
    }
}

/// 解析后的结果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClaudeResult {
    pub session_id: String,
    pub text: String,
    pub is_error: bool,
    pub subtype: String,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub denials: Vec<ApprovalItem>,
}

/// 处理一行 stream-json 输出；返回给用户看的步骤（如有）
pub fn parse_line(line: &str, res: &mut ClaudeResult) -> Vec<TaskStep> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let mut steps = Vec::new();
    match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "system" => {
            if let Some(s) = v.get("session_id").and_then(|s| s.as_str()) {
                res.session_id = s.to_string();
            }
        }
        "assistant" => {
            if let Some(items) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                for it in items {
                    match it.get("type").and_then(|t| t.as_str()) {
                        Some("tool_use") => {
                            let name = it.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let (kind, text) =
                                describe_tool(name, it.get("input").unwrap_or(&Value::Null));
                            steps.push(TaskStep { kind, text });
                        }
                        Some("text") => {
                            let t = it.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
                            if !t.is_empty() {
                                steps.push(TaskStep {
                                    kind: StepKind::Note,
                                    text: short(t, 160),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        "result" => {
            res.subtype = v
                .get("subtype")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            res.is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
            res.text = v
                .get("result")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            res.cost_usd = v
                .get("total_cost_usd")
                .and_then(|c| c.as_f64())
                .unwrap_or(0.0);
            if let Some(s) = v.get("session_id").and_then(|s| s.as_str()) {
                res.session_id = s.to_string();
            }
            let n = |k: &str| {
                v.pointer(&format!("/usage/{k}"))
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0)
            };
            res.input_tokens =
                n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
            res.output_tokens = n("output_tokens");
            if let Some(ds) = v.get("permission_denials").and_then(|d| d.as_array()) {
                for d in ds {
                    let name = d.get("tool_name").and_then(|n| n.as_str()).unwrap_or("");
                    let input = d.get("tool_input").cloned().unwrap_or(Value::Null);
                    if let Some(item) = approval_for(name, &input)
                        && !res.denials.contains(&item)
                    {
                        res.denials.push(item);
                    }
                }
            }
        }
        _ => {}
    }
    steps
}

/// `--allowedTools` 规则里不允许出现的字符（避免规则被解析错，或匹配到预期之外的命令）
fn pattern_safe(s: &str) -> bool {
    !s.is_empty() && s.len() <= 300 && !s.chars().any(|c| c.is_control() || "()\"`".contains(c))
}

/// 被拒绝的操作 → 给用户批准的条目（带精确的放行规则）
pub fn approval_for(tool: &str, input: &Value) -> Option<ApprovalItem> {
    let get = |k: &str| {
        input
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let (kind, detail, rule) = match tool {
        "Bash" | "PowerShell" => {
            let cmd = get("command");
            let rule = pattern_safe(&cmd).then(|| format!("{tool}({cmd})"));
            (StepKind::Command, format!("执行命令：{cmd}"), rule)
        }
        "WebFetch" => {
            let url = get("url");
            let host = url
                .split("://")
                .nth(1)
                .and_then(|r| r.split(['/', '?', '#']).next())
                .unwrap_or("")
                .to_string();
            let ok = !host.is_empty()
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c));
            (
                StepKind::Network,
                format!("访问网页：{url}"),
                ok.then(|| format!("WebFetch(domain:{host})")),
            )
        }
        "WebSearch" => (
            StepKind::Network,
            format!("联网搜索：{}", get("query")),
            Some("WebSearch".to_string()),
        ),
        "Read" | "Edit" | "Write" | "Glob" | "Grep" => {
            let path = [get("file_path"), get("path")]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_default();
            (
                StepKind::Outside,
                format!("访问工作目录以外的文件：{path}"),
                None,
            )
        }
        other => (StepKind::Other, format!("使用工具 {other}"), None),
    };
    Some(ApprovalItem {
        kind,
        detail: short(&detail, 300),
        rule,
        path: match tool {
            "Read" | "Edit" | "Write" | "Glob" | "Grep" => {
                let p = [get("file_path"), get("path")]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or_default();
                (!p.is_empty()).then_some(p)
            }
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_are_locked_down() {
        let a = build_args(&ClaudeArgs {
            budget_usd: 0.5,
            model: "",
            resume_session: None,
            allowed: &[],
            add_dirs: &[],
        });
        let s = a.join(" ");
        assert!(a.contains(&"--restricted".to_string()));
        assert!(s.contains("--permission-prompts none"));
        assert!(s.contains("--permission-mode acceptEdits"));
        assert!(s.contains("--max-budget-usd 0.50"));
        assert!(!s.contains("dangerously"));
        assert!(!s.contains("bypassPermissions"));
        assert!(!a.contains(&"--allowedTools".to_string()));
    }

    #[test]
    fn resume_with_exact_approvals_last() {
        let allowed = vec!["Bash(rm old.log)".to_string()];
        let a = build_args(&ClaudeArgs {
            budget_usd: 0.2,
            model: "sonnet",
            resume_session: Some("abc-123"),
            allowed: &allowed,
            add_dirs: &[],
        });
        let n = a.len();
        assert_eq!(
            &a[n - 2..],
            &["--allowedTools".to_string(), "Bash(rm old.log)".to_string()]
        );
        let s = a.join(" ");
        assert!(s.contains("--resume abc-123"));
        assert!(s.contains("--model sonnet"));
    }

    #[test]
    fn parses_stream_json() {
        let mut r = ClaudeResult::default();
        parse_line(
            r#"{"type":"system","subtype":"init","session_id":"s-1"}"#,
            &mut r,
        );
        let steps = parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"我先看看文件"},{"type":"tool_use","name":"Bash","input":{"command":"rm old.log"}}]}}"#,
            &mut r,
        );
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].kind, StepKind::Command);
        assert!(steps[1].text.contains("rm old.log"));
        parse_line(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"整理好了","total_cost_usd":0.0123,"session_id":"s-1","usage":{"input_tokens":100,"cache_read_input_tokens":50,"output_tokens":20},"permission_denials":[{"tool_name":"Bash","tool_use_id":"t1","tool_input":{"command":"rm old.log"}},{"tool_name":"WebFetch","tool_input":{"url":"https://example.com/a?b"}}]}"#,
            &mut r,
        );
        assert_eq!(r.session_id, "s-1");
        assert_eq!(r.text, "整理好了");
        assert!((r.cost_usd - 0.0123).abs() < 1e-9);
        assert_eq!(r.input_tokens, 150);
        assert_eq!(r.denials.len(), 2);
        assert_eq!(r.denials[0].rule.as_deref(), Some("Bash(rm old.log)"));
        assert_eq!(
            r.denials[1].rule.as_deref(),
            Some("WebFetch(domain:example.com)")
        );
        // 坏行忽略
        assert!(parse_line("not json", &mut r).is_empty());
    }

    #[test]
    fn unsafe_commands_get_no_exact_rule() {
        let item = approval_for(
            "Bash",
            &serde_json::json!({"command": "echo $(cat ~/.ssh/id_rsa)"}),
        )
        .unwrap();
        assert!(item.rule.is_none(), "含括号的命令无法生成精确规则");
        let item = approval_for(
            "Read",
            &serde_json::json!({"file_path": "C:\\Users\\me\\secret.txt"}),
        )
        .unwrap();
        assert_eq!(item.kind, StepKind::Outside);
        assert_eq!(item.path.as_deref(), Some("C:\\Users\\me\\secret.txt"));
    }

    #[test]
    fn session_id_validation() {
        assert!(valid_session_id("0b8c-11ef_x"));
        assert!(!valid_session_id("a b"));
        assert!(!valid_session_id("--dangerously"));
        assert!(!valid_session_id(""));
    }
}
