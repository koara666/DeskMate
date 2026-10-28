//! 角色的「原作台词库」：可选的 `quotes.md`（DECISIONS #31）。
//!
//! 内容是从原作语料里「炼化」出来的对话片段（对方说了什么、她怎么回），帮模型找准语气和反应方式：
//! - 没写「关键词」行的片段是**常驻**的：每轮都带，放进可缓存的系统提示词（总共约 1200 token 以内）；
//! - 写了「关键词」行的片段和往事（`canon.rs`）一样：用户这句话提到关键词时才带，每轮最多 2 段、约 300 token。
//!   只被角色上一句回复命中的不带（语气参考跟着用户的话走就够了）。
//!
//! 台词原文只放在用户数据目录（`%APPDATA%\DeskMate\characters\<id>\quotes.md`），不进公开仓库
//! （`.gitignore` 排除了 `characters/*/quotes.md`）。只读：程序没有任何写入台词库的接口。
//!
//! 文件格式和 canon.md 相同（第一个 `## ` 之前是给人看的说明，程序忽略）：
//! ```text
//! ## 场合
//! 关键词：词1、词2      ← 可选；不写就是常驻片段
//! 对方：……
//! 她：……
//! ```

use crate::backends::estimate_tokens;
use crate::canon::{self, CanonEntry, MAX_ENTRIES, RecallLimits};

/// 常驻片段最多占用的 token（在可缓存的系统提示词里，每轮都带）
const FIXED_TOKENS: usize = 1200;

/// 按关键词取用的上限
const KEYED_LIMITS: RecallLimits = RecallLimits {
    max: 2,
    max_reply_only: 0,
    tokens: 300,
};

#[derive(Debug, Clone, Default)]
pub struct Quotes {
    /// 常驻片段，已排好版、已按预算截取（按文件顺序，放不下的丢掉）
    pub fixed: String,
    pub fixed_count: usize,
    /// 超出常驻预算、没用上的片段数（加载时记日志提醒）
    pub fixed_dropped: usize,
    /// 按关键词取用的片段
    pub keyed: Vec<CanonEntry>,
}

fn render(e: &CanonEntry) -> String {
    format!("◇ {}\n{}", e.title, e.text)
}

pub fn parse(src: &str) -> Quotes {
    let mut q = Quotes::default();
    let mut left = FIXED_TOKENS;
    for e in canon::parse_all(src) {
        if !e.keywords.is_empty() {
            if q.keyed.len() < MAX_ENTRIES {
                q.keyed.push(e);
            }
            continue;
        }
        let block = render(&e);
        let cost = estimate_tokens(&block);
        // 预算用完后后面的都不要，保证「文件里越靠前越优先」
        if q.fixed_dropped > 0 || cost > left {
            q.fixed_dropped += 1;
            continue;
        }
        left -= cost;
        if !q.fixed.is_empty() {
            q.fixed.push_str("\n\n");
        }
        q.fixed.push_str(&block);
        q.fixed_count += 1;
    }
    q
}

/// 本轮用户的话提到关键词时，取出相应的片段
pub fn recall(q: &Quotes, user_text: &str) -> Vec<String> {
    canon::recall_with(&q.keyed, user_text, "", KEYED_LIMITS, render)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# 红莉栖 · 台词库（说明，程序忽略）

## 被叫外号
冈部：克里斯蒂娜！
红莉栖：不许加「蒂娜」！

## 被夸聪明
桶子：牧濑氏好聪明。
红莉栖：这是训练的结果。

## 杯面
关键词：杯面、泡面
冈部：你用叉子吃杯面？
红莉栖：有什么问题吗。

## 科学
关键词：实验、假设
红莉栖：先验证再下结论。
";

    #[test]
    fn splits_fixed_and_keyed() {
        let q = parse(SAMPLE);
        assert_eq!(q.fixed_count, 2);
        assert_eq!(q.fixed_dropped, 0);
        assert_eq!(
            q.fixed,
            "◇ 被叫外号\n冈部：克里斯蒂娜！\n红莉栖：不许加「蒂娜」！\n\n◇ 被夸聪明\n桶子：牧濑氏好聪明。\n红莉栖：这是训练的结果。"
        );
        let titles: Vec<&str> = q.keyed.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["杯面", "科学"]);
    }

    #[test]
    fn keyed_recall_follows_user_text_only() {
        let q = parse(SAMPLE);
        assert!(recall(&q, "今天好累").is_empty(), "没聊到就不占 token");
        assert_eq!(
            recall(&q, "晚饭吃泡面"),
            vec!["◇ 杯面\n冈部：你用叉子吃杯面？\n红莉栖：有什么问题吗。"]
        );
        assert_eq!(recall(&q, "做个实验验证这个假设，再吃杯面").len(), 2);
    }

    #[test]
    fn fixed_budget_keeps_file_order() {
        let mut src = String::new();
        for i in 0..40 {
            src.push_str(&format!("## 片段{i}\n红莉栖：{}\n", "字".repeat(60)));
        }
        let q = parse(&src);
        assert!(q.fixed_count > 0 && q.fixed_dropped > 0);
        assert_eq!(q.fixed_count + q.fixed_dropped, 40);
        assert!(estimate_tokens(&q.fixed) <= FIXED_TOKENS);
        assert!(q.fixed.starts_with("◇ 片段0\n"), "越靠前越优先");
    }

    /// 维护者用的体检（HOW_TO_ADD 1.5 第 8 步）：对真实台词库报告常驻用量，并用日常的话看关键词片段会不会乱触发。
    /// 台词库不在仓库里，所以默认忽略；用法：
    /// `DESKMATE_QUOTES_FILE=<quotes.md 路径> cargo test --manifest-path src-tauri/Cargo.toml real_quotes_report -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_quotes_report() {
        let path = std::env::var("DESKMATE_QUOTES_FILE").expect("请设置 DESKMATE_QUOTES_FILE");
        let q = parse(&std::fs::read_to_string(path).unwrap());
        println!(
            "常驻 {} 段，约 {} token（上限 {FIXED_TOKENS}），放不下 {} 段；关键词片段 {} 段",
            q.fixed_count,
            estimate_tokens(&q.fixed),
            q.fixed_dropped,
            q.keyed.len()
        );
        assert_eq!(q.fixed_dropped, 0, "常驻台词超出预算，后面的没有用上");
        for e in &q.keyed {
            let cost = estimate_tokens(&render(e));
            assert!(
                cost <= KEYED_LIMITS.tokens,
                "「{}」有 {cost} token，超过每轮 {} 的预算，永远取不出来",
                e.title,
                KEYED_LIMITS.tokens
            );
        }
        let daily = [
            "我好累",
            "今天吃什么",
            "你好聪明",
            "我失败了",
            "我今天心情很差",
            "你在做什么",
            "我快下班了",
            "我不知道为什么有点难过",
            "我想去睡觉了，晚安",
            "周末想去海边玩",
            "我们去约会吧",
            "你喜欢玩什么游戏",
            "我最近在玩恋爱游戏",
            "你擅长格斗游戏吗",
            "送你个礼物",
            "你喜欢看电影吗",
            "我去图书馆学习了一天",
            "我昨天去钓鱼了",
            "帮我整理一下文件夹",
            "这道题我解不出来",
        ];
        for s in daily {
            let hit: Vec<String> = recall(&q, s)
                .iter()
                .map(|r| r.lines().next().unwrap_or("").to_string())
                .collect();
            println!(
                "{s:<20} → {}",
                if hit.is_empty() {
                    "-".into()
                } else {
                    hit.join("、")
                }
            );
        }
    }

    #[test]
    fn keyed_recall_respects_limits() {
        let mut src = String::new();
        for i in 0..5 {
            src.push_str(&format!("## 片段{i}\n关键词：共同\n{}\n", "字".repeat(50)));
        }
        src.push_str(&format!("## 太长\n关键词：很长\n{}\n", "字".repeat(400)));
        let q = parse(&src);
        assert_eq!(recall(&q, "共同").len(), KEYED_LIMITS.max);
        assert!(recall(&q, "很长").is_empty(), "超出预算的片段不放");
    }
}
