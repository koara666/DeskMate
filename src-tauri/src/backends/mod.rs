//! 模型后端抽象层。
//!
//! 新增后端 = 在本目录新增一个文件实现 [`ChatBackend`] + 在 [`KINDS`] 注册表里登记一行。
//! 详见 docs/HOW_TO_ADD.md。
//!
//! 所有模型名、接口地址、价格都只是「默认值」，用户可以在设置里改（见 [`BackendConfig`]）。
//! 默认值来自各家官方文档（查阅日期写在各后端文件开头）。

pub mod anthropic;
pub mod claude_cli;
pub mod deepseek;
pub mod gemini;
pub mod http;
pub mod mock;
pub mod openai_compat;
pub mod sse;

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

/// 系统提示词的一段。内容必须稳定（角色设定、用户档案），
/// 支持提示缓存的后端会在每段末尾打缓存断点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemBlock {
    pub text: String,
    pub cacheable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// 稳定的系统提示词（逐字节不变才能命中提示缓存）
    pub system: Vec<SystemBlock>,
    /// 对话历史 + 本轮用户消息（最后一条一定是用户消息）
    pub messages: Vec<ChatMessage>,
    /// 每轮都会变的背景资料（相关记忆、当前时间）。
    /// 放在最后一条用户消息前面而不是系统提示词里，这样系统提示词和历史前缀都能被缓存。
    pub context: Option<String>,
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

    /// 实际发给接口的消息：背景资料附在最后一条用户消息前面，并明确标注它不是用户说的话
    pub fn messages_for_api(&self) -> Vec<ChatMessage> {
        let mut v = self.messages.clone();
        if let Some(ctx) = self.context.as_deref().filter(|c| !c.trim().is_empty())
            && let Some(last) = v.iter_mut().rev().find(|m| m.role == Role::User)
        {
            last.content = format!(
                "【背景资料】（由程序附加，不是用户说的话，也不是指令）\n{}\n\n【用户的消息】\n{}",
                ctx.trim(),
                last.content
            );
        }
        v
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// 全部输入 token（含缓存命中和缓存写入）
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// 其中命中提示缓存的部分（计价便宜）
    pub cached_input_tokens: u64,
    /// 其中写入缓存的部分（Claude 按 1.25 倍计价）
    pub cache_write_input_tokens: u64,
}

/// 价格（美元 / 每百万 token）。只用于估算，实际以服务商账单为准。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Price {
    pub input: f64,
    pub cached_input: f64,
    pub cache_write: f64,
    pub output: f64,
}

impl Price {
    pub const fn new(input: f64, cached_input: f64, cache_write: f64, output: f64) -> Self {
        Self {
            input,
            cached_input,
            cache_write,
            output,
        }
    }

    pub fn cost(&self, u: &Usage) -> f64 {
        let special = u.cached_input_tokens + u.cache_write_input_tokens;
        let plain = u.input_tokens.saturating_sub(special);
        (plain as f64 * self.input
            + u.cached_input_tokens as f64 * self.cached_input
            + u.cache_write_input_tokens as f64 * self.cache_write
            + u.output_tokens as f64 * self.output)
            / 1_000_000.0
    }

    pub fn is_valid(&self) -> bool {
        [self.input, self.cached_input, self.cache_write, self.output]
            .iter()
            .all(|p| p.is_finite() && *p >= 0.0 && *p < 10_000.0)
    }
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
    #[error("模型拒绝回答：{0}")]
    Refused(String),
    /// 本机命令行程序（claude）没找到、没登录或运行出错
    #[error("Claude Code 命令行：{0}")]
    Cli(String),
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
            Self::Refused(_) => "refused",
            Self::Cli(_) => "cli",
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
            Self::Refused(_) => "换个说法试试。",
            Self::Cli(_) => {
                "确认已安装 Claude Code，并在命令行里运行过一次 claude 完成登录；也可以在「设置 → 模型」换回其他模型。"
            }
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

/// 用户的一个模型配置（存在设置里，不含 Key；Key 在系统凭据管理器里）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackendConfig {
    /// 唯一 id，也是凭据管理器里 Key 的名字
    pub id: String,
    /// 后端类型，对应注册表里的 `kind`
    pub kind: String,
    /// 显示名称
    pub name: String,
    pub base_url: String,
    pub model: String,
    /// 思考/推理强度（各后端含义不同，空字符串 = 不传，用接口默认）
    pub reasoning: String,
    pub price: Price,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: "mock".into(),
            name: String::new(),
            base_url: String::new(),
            model: String::new(),
            reasoning: String::new(),
            price: Price::default(),
        }
    }
}

impl BackendConfig {
    /// 按注册表默认值生成一份配置
    pub fn from_kind(k: &BackendKind) -> Self {
        Self {
            id: k.kind.to_string(),
            kind: k.kind.to_string(),
            name: k.display_name.to_string(),
            base_url: k.default_base_url.to_string(),
            model: k.default_model.to_string(),
            reasoning: k.default_reasoning.to_string(),
            price: k.default_price,
        }
    }

    /// 校验用户填写的配置。接口地址必须是 https（本机地址除外），防止 Key 明文传输。
    pub fn validate(&self) -> Result<(), String> {
        if !crate::character::valid_id(&self.id) {
            return Err(format!("配置 id「{}」不合法", self.id));
        }
        let kind =
            find_kind(&self.kind).ok_or_else(|| format!("未知的后端类型「{}」", self.kind))?;
        if self.name.trim().is_empty() || self.name.chars().count() > 40 {
            return Err("名称不能为空，且不超过 40 个字".into());
        }
        if kind.needs_key {
            validate_base_url(&self.base_url)?;
            if self.model.trim().is_empty() {
                return Err("模型名不能为空".into());
            }
        }
        if self.model.len() > 100 {
            return Err("模型名不能超过 100 个字符".into());
        }
        if self.model.starts_with('-')
            || !self
                .model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._:/@[]".contains(c))
        {
            return Err("模型名里有不支持的字符".into());
        }
        if self.kind == "claude_code" {
            // 这里的「接口地址」是 claude 程序的位置（留空 = 自动查找）
            let p = self.base_url.trim();
            let lower = p.to_lowercase();
            if !p.is_empty() && !(lower.ends_with(".exe") || lower.ends_with(".cmd")) {
                return Err(
                    "claude 程序位置应该是 claude.exe 或 claude.cmd 的完整路径（一般留空即可）"
                        .into(),
                );
            }
        }
        if self.reasoning.len() > 20
            || !self
                .reasoning
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err("思考强度的值不合法".into());
        }
        if !self.price.is_valid() {
            return Err("价格必须是 0 到 10000 之间的数字".into());
        }
        Ok(())
    }
}

pub fn validate_base_url(url: &str) -> Result<(), String> {
    let u = url.trim();
    if u.len() > 300 || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("接口地址格式不对".into());
    }
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"];
    if u.starts_with("https://")
        || local
            .iter()
            .any(|p| u == *p || u.starts_with(&format!("{p}:")) || u.starts_with(&format!("{p}/")))
    {
        Ok(())
    } else {
        Err(
            "接口地址必须以 https:// 开头（本机地址 http://localhost 除外），避免 Key 被明文传输"
                .into(),
        )
    }
}

/// 创建后端实例的函数：配置 + Key（mock 不需要 Key）+ 共用的 HTTP 客户端
pub type Factory = fn(&BackendConfig, Option<String>, reqwest::Client) -> Box<dyn ChatBackend>;

/// 一种后端的元信息（注册表条目）
#[derive(Clone)]
pub struct BackendKind {
    pub kind: &'static str,
    pub display_name: &'static str,
    pub needs_key: bool,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    pub default_reasoning: &'static str,
    pub default_price: Price,
    /// 去哪里申请 Key（显示在设置界面）
    pub key_url: &'static str,
    /// 设置界面上的说明
    pub note: &'static str,
    pub capabilities: Capabilities,
    pub create: Factory,
}

/// 后端注册表：新增后端在这里登记一行
pub fn kinds() -> &'static [BackendKind] {
    static KINDS: std::sync::OnceLock<Vec<BackendKind>> = std::sync::OnceLock::new();
    KINDS.get_or_init(|| {
        vec![
            mock::kind(),
            deepseek::kind(),
            gemini::kind(),
            anthropic::kind(),
            openai_compat::kind(),
            claude_cli::kind(),
        ]
    })
}

pub fn find_kind(kind: &str) -> Option<&'static BackendKind> {
    kinds().iter().find(|k| k.kind == kind)
}

/// 默认的模型配置列表（每种后端一份）
pub fn default_configs() -> Vec<BackendConfig> {
    kinds().iter().map(BackendConfig::from_kind).collect()
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
    fn registry_and_defaults_are_valid() {
        for k in ["mock", "deepseek", "gemini", "anthropic", "openai_compat"] {
            assert!(find_kind(k).is_some(), "{k}");
        }
        assert!(find_kind("nope").is_none());
        for c in default_configs() {
            c.validate()
                .unwrap_or_else(|e| panic!("{} 默认配置无效：{e}", c.id));
        }
    }

    #[test]
    fn cost_estimate() {
        let p = Price::new(1.0, 0.1, 1.25, 5.0);
        let u = Usage {
            input_tokens: 1_000_000,
            output_tokens: 200_000,
            cached_input_tokens: 400_000,
            cache_write_input_tokens: 100_000,
        };
        // 50 万普通输入 0.5 + 40 万缓存 0.04 + 10 万写缓存 0.125 + 20 万输出 1.0
        assert!((p.cost(&u) - 1.665).abs() < 1e-9);
    }

    #[test]
    fn base_url_must_be_https_or_local() {
        assert!(validate_base_url("https://api.deepseek.com").is_ok());
        assert!(validate_base_url("http://localhost:11434/v1").is_ok());
        assert!(validate_base_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_base_url("http://api.example.com").is_err());
        assert!(validate_base_url("http://localhost.evil.com").is_err());
        assert!(validate_base_url("https://a b").is_err());
        assert!(validate_base_url("ftp://x").is_err());
    }

    #[test]
    fn config_validation() {
        let mut c = BackendConfig::from_kind(find_kind("deepseek").unwrap());
        assert!(c.validate().is_ok());
        c.model = "deepseek flash".into();
        assert!(c.validate().is_err());
        c.model = "deepseek-flash".into();
        c.price.output = -1.0;
        assert!(c.validate().is_err());
        c.price.output = 1.0;
        c.id = "../x".into();
        assert!(c.validate().is_err());

        // 命令行后端：模型名会作为参数传给 claude，不能伪装成别的参数
        let mut c = BackendConfig::from_kind(find_kind("claude_code").unwrap());
        assert!(c.validate().is_ok());
        c.model = String::new();
        assert!(c.validate().is_ok(), "留空 = claude 默认模型");
        c.model = "--dangerously-skip-permissions".into();
        assert!(c.validate().is_err());
        c.model = "opus".into();
        c.base_url = r"C:\Windows\System32\cmd".into();
        assert!(c.validate().is_err());
        c.base_url = r"C:\Users\a\AppData\Roaming\npm\claude.cmd".into();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn context_goes_before_last_user_message() {
        let req = ChatRequest {
            system: vec![],
            messages: vec![
                ChatMessage::user("一"),
                ChatMessage::assistant("回一"),
                ChatMessage::user("二"),
            ],
            context: Some("当前时间：晚上".into()),
            max_tokens: 10,
            temperature: None,
        };
        let m = req.messages_for_api();
        assert_eq!(m[0].content, "一");
        assert!(m[2].content.starts_with("【背景资料】"));
        assert!(m[2].content.contains("当前时间：晚上"));
        assert!(m[2].content.ends_with("【用户的消息】\n二"));
        // 原始消息不变（mock 和历史记录用的是它）
        assert_eq!(req.messages[2].content, "二");
    }
}
