//! 模型后端抽象层。
//!
//! 新增后端 = 在本目录新增一个文件实现 [`ChatBackend`] + 在 [`KINDS`] 注册表里登记一行。
//! 详见 docs/HOW_TO_ADD.md。

pub mod mock;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn user(s: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: s.into(),
        }
    }
    pub fn assistant(s: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: s.into(),
        }
    }
}

/// 系统提示词的一段。`cacheable = true` 的段落内容稳定（角色设定、用户档案），
/// 支持提示缓存的后端会在这里打缓存断点。它们总是排在前面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemBlock {
    pub text: String,
    pub cacheable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub system: Vec<SystemBlock>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
}

impl ChatRequest {
    /// 把系统段落拼成一个字符串（不支持分段的后端使用）
    pub fn system_text(&self) -> String {
        self.system
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// 命中提示缓存的输入 token（计价更便宜）
    pub cached_input_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    /// 达到 max_tokens 被截断
    Length,
    /// 用户中途停止
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatOutcome {
    pub text: String,
    pub usage: Usage,
    pub finish: FinishReason,
}

/// 后端能力声明
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub streaming: bool,
    pub tool_calls: bool,
    /// agent 型（能自己执行任务，如 Claude Code / Codex）
    pub agent: bool,
    pub prompt_cache: bool,
}

/// 后端错误。`detail` 必须已经脱敏（不含 Key），可以直接展示给用户。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    #[error("还没有配置可用的模型")]
    NotConfigured,
    #[error("API Key 无效或没有权限：{0}")]
    InvalidKey(String),
    #[error("请求太频繁：{0}")]
    RateLimited(String),
    #[error("账户余额或额度不足：{0}")]
    InsufficientBalance(String),
    #[error("网络连接失败：{0}")]
    Network(String),
    #[error("请求超时")]
    Timeout,
    #[error("服务端错误（{status}）：{detail}")]
    Server { status: u16, detail: String },
    #[error("请求被拒绝（{status}）：{detail}")]
    BadRequest { status: u16, detail: String },
    #[error("返回数据无法解析：{0}")]
    Protocol(String),
    #[error("已达到花费上限：{0}")]
    BudgetExceeded(String),
}

impl BackendError {
    /// 给前端用的错误类别
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::InvalidKey(_) => "invalid_key",
            Self::RateLimited(_) => "rate_limited",
            Self::InsufficientBalance(_) => "insufficient_balance",
            Self::Network(_) => "network",
            Self::Timeout => "timeout",
            Self::Server { .. } => "server",
            Self::BadRequest { .. } => "bad_request",
            Self::Protocol(_) => "protocol",
            Self::BudgetExceeded(_) => "budget_exceeded",
        }
    }

    /// 告诉用户该怎么办
    pub fn hint(&self) -> &'static str {
        match self {
            Self::NotConfigured => "打开「设置 → 模型」填写 API Key，或先用 mock 模式体验。",
            Self::InvalidKey(_) => {
                "检查「设置 → 模型」里的 Key 是否填对、是否过期，可以点「测试连接」。"
            }
            Self::RateLimited(_) => "稍等一会儿再试；如果经常出现，可以换一个模型。",
            Self::InsufficientBalance(_) => "去对应服务商的网站充值或检查套餐额度。",
            Self::Network(_) => "检查网络或代理设置，恢复后点「重试」。",
            Self::Timeout => "网络较慢或服务繁忙，稍后点「重试」。",
            Self::Server { .. } => "服务商那边出了问题，一般过一会儿就好。",
            Self::BadRequest { .. } => "可能是模型名或接口地址填错了，检查「设置 → 模型」。",
            Self::Protocol(_) => "接口返回了意料之外的数据，检查接口地址是否正确。",
            Self::BudgetExceeded(_) => "可以在「设置 → 用量」里调高上限，或者等到明天/下个月。",
        }
    }

    /// 根据 HTTP 状态码归类（各后端在此基础上可以再细分）
    pub fn from_status(status: u16, detail: String) -> Self {
        match status {
            401 | 403 => Self::InvalidKey(detail),
            402 => Self::InsufficientBalance(detail),
            429 => Self::RateLimited(detail),
            408 | 504 => Self::Timeout,
            500..=599 => Self::Server { status, detail },
            _ => Self::BadRequest { status, detail },
        }
    }
}

/// 流式增量回调：每收到一段文本调用一次
pub type DeltaSink<'a> = &'a mut (dyn FnMut(&str) + Send);

pub trait ChatBackend: Send + Sync {
    fn capabilities(&self) -> Capabilities;

    /// 流式对话。取消时应尽快返回 `FinishReason::Cancelled`，并带上已生成的部分。
    fn stream_chat<'a>(
        &'a self,
        req: ChatRequest,
        on_delta: DeltaSink<'a>,
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<ChatOutcome, BackendError>>;
}

/// 一种后端的元信息（注册表条目）
#[derive(Debug, Clone, Serialize)]
pub struct BackendKind {
    pub kind: &'static str,
    pub display_name: &'static str,
    pub needs_key: bool,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    pub capabilities: Capabilities,
}

/// 后端注册表：新增后端在这里登记一行
pub const KINDS: &[BackendKind] = &[mock::KIND];

pub fn find_kind(kind: &str) -> Option<&'static BackendKind> {
    KINDS.iter().find(|k| k.kind == kind)
}

/// 粗略估算 token 数：中日韩字符约 1 token/字，ASCII 约 4 字符/token。
/// 只用于预算控制，不用于计费（计费以后端返回的 usage 为准）。
pub fn estimate_tokens(s: &str) -> usize {
    let mut cjk = 0usize;
    let mut ascii = 0usize;
    let mut other = 0usize;
    for c in s.chars() {
        if c.is_ascii() {
            ascii += 1;
        } else if is_cjk(c) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + ascii.div_ceil(4) + other.div_ceil(2)
}

pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // 日文假名
        | 0x3400..=0x4DBF // 扩展 A
        | 0x4E00..=0x9FFF // 基本汉字
        | 0xAC00..=0xD7AF // 韩文
        | 0xF900..=0xFAFF
        | 0x20000..=0x2FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        assert_eq!(
            BackendError::from_status(401, "x".into()).kind(),
            "invalid_key"
        );
        assert_eq!(
            BackendError::from_status(402, "x".into()).kind(),
            "insufficient_balance"
        );
        assert_eq!(
            BackendError::from_status(429, "x".into()).kind(),
            "rate_limited"
        );
        assert_eq!(BackendError::from_status(503, "x".into()).kind(), "server");
        assert_eq!(
            BackendError::from_status(400, "x".into()).kind(),
            "bad_request"
        );
    }

    #[test]
    fn token_estimate() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("你好世界"), 4);
        assert_eq!(estimate_tokens("hello world!"), 3);
        assert!(estimate_tokens("你好 hello") >= 4);
    }

    #[test]
    fn registry_has_mock() {
        assert!(find_kind("mock").is_some());
        assert!(find_kind("nope").is_none());
    }
}
