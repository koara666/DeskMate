//! DeepSeek 官方 API（OpenAI 兼容格式）。
//!
//! 默认值查阅自官方文档（2026-10-27）：
//! - https://api-docs.deepseek.com/quick_start/pricing ：接口地址 https://api.deepseek.com，
//!   模型 `deepseek-flash`；价格（美元/百万 token，高峰时段）：缓存命中 0.006、未命中 0.30、输出 1.20。
//!   非高峰时段价格减半——这里按高峰价估算，宁可多估不少估（花费上限更安全）。
//! - https://api-docs.deepseek.com/guides/thinking_mode ：默认开启思考模式；
//!   日常聊天用 `thinking: {type: "disabled"}` 关闭，回复更快更省。
//! - https://api-docs.deepseek.com/quick_start/error_codes ：402 = 余额不足，422 = 参数错误，503 = 服务器繁忙。

use super::openai_compat::OpenAiCompat;
use super::*;

pub fn kind() -> BackendKind {
    BackendKind {
        kind: "deepseek",
        display_name: "DeepSeek",
        needs_key: true,
        default_base_url: "https://api.deepseek.com",
        default_model: "deepseek-flash",
        default_reasoning: "disabled",
        default_price: Price::new(0.30, 0.006, 0.30, 1.20),
        key_url: "https://platform.deepseek.com/api_keys",
        note: "便宜、中文好，适合日常聊天。价格按高峰时段估算；DeepSeek 会自动缓存重复的开头部分，命中后更便宜。",
        capabilities: Capabilities {
            streaming: true,
            tool_calls: true,
            agent: false,
            prompt_cache: true,
        },
        create: |cfg, key, client| {
            let mut b = OpenAiCompat::new(cfg, key, client);
            b.classify = classify;
            Box::new(b)
        },
    }
}

/// DeepSeek 的错误码：422 是参数错误（多半是模型名填错）
fn classify(status: u16, detail: String) -> BackendError {
    match status {
        422 => BackendError::BadRequest { status, detail },
        _ => BackendError::from_status(status, detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let k = kind();
        let c = BackendConfig::from_kind(&k);
        assert_eq!(c.model, "deepseek-flash");
        assert!(c.validate().is_ok());
        assert_eq!(classify(402, "x".into()).kind(), "insufficient_balance");
        assert_eq!(classify(422, "x".into()).kind(), "bad_request");
    }
}
