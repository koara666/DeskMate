//! 角色的「原作往事」：角色包里可选的 `canon.md`（DECISIONS #30）。
//!
//! 省 token 的分工：
//! - `persona.md` 每轮都放进可缓存的系统提示词，只写身份、性格和最核心的经历；
//! - `canon.md` 写细节往事，每条带关键词。本轮用户的话（或角色上一句回复）提到关键词时，
//!   才把那几条放进本轮背景资料；不聊到就一个 token 都不占。
//!
//! 和用户记忆完全分开：只读、不进数据库、不参与记忆整理和导出，记忆管理界面里也看不到。
//!
//! 文件格式（第一个 `## ` 之前是给人看的说明，程序忽略）：
//! ```text
//! ## 标题
//! 关键词：词1、词2、词3
//! 正文（可以多行）
//! ```

use crate::backends::estimate_tokens;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonEntry {
    pub title: String,
    /// 已转成小写；每个至少 2 个字符（单字太容易误命中）
    pub keywords: Vec<String>,
    pub text: String,
}

/// 最多读取的条目数（防止异常大的角色包拖慢每轮匹配）
const MAX_ENTRIES: usize = 300;
/// 每轮最多放几条
const MAX_RECALL: usize = 3;
/// 只被角色上一句回复命中（用户这句没提到）的条目最多放几条：接得上话就够了
const MAX_REPLY_ONLY: usize = 1;
/// 每轮往事最多占用的 token
const RECALL_TOKENS: usize = 500;

pub fn parse(src: &str) -> Vec<CanonEntry> {
    let mut out = Vec::new();
    let mut cur: Option<CanonEntry> = None;
    for line in src.lines() {
        if let Some(t) = line.strip_prefix("## ") {
            if let Some(e) = cur.take() {
                push(&mut out, e);
            }
            cur = Some(CanonEntry {
                title: t.trim().to_string(),
                keywords: Vec::new(),
                text: String::new(),
            });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        let l = line.trim();
        if e.keywords.is_empty()
            && e.text.is_empty()
            && let Some(k) = l
                .strip_prefix("关键词：")
                .or_else(|| l.strip_prefix("关键词:"))
        {
            e.keywords = split_keywords(k);
            continue;
        }
        if !l.is_empty() {
            if !e.text.is_empty() {
                e.text.push('\n');
            }
            e.text.push_str(l);
        }
    }
    if let Some(e) = cur.take() {
        push(&mut out, e);
    }
    out.truncate(MAX_ENTRIES);
    out
}

fn push(out: &mut Vec<CanonEntry>, e: CanonEntry) {
    // 没有关键词或没有正文的条目永远用不上
    if !e.keywords.is_empty() && !e.text.is_empty() {
        out.push(e);
    }
}

fn split_keywords(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .split(['、', ',', '，', ';', '；'])
        .map(|k| k.trim().to_lowercase())
        .filter(|k| k.chars().count() >= 2)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// 关键词是否出现在文本里。英文/数字开头或结尾的关键词要求词边界（「ai」不能命中「email」）。
fn contains_word(hay: &str, k: &str) -> bool {
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
    let first = is_word(k.chars().next());
    let last = is_word(k.chars().next_back());
    hay.match_indices(k).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + k.len()..].chars().next();
        !(first && is_word(before)) && !(last && is_word(after))
    })
}

/// 取出本轮要想起的往事，每条是「标题：正文」。
/// 用户这句话命中的关键词记 2 分，角色上一句回复命中的记 1 分（接着聊同一件事时也想得起来）；
/// 按得分从高到低、同分按文件顺序，在条数和 token 预算内返回。
pub fn recall(entries: &[CanonEntry], user_text: &str, last_reply: &str) -> Vec<String> {
    let user = user_text.to_lowercase();
    let reply = last_reply.to_lowercase();
    let mut scored: Vec<(usize, bool, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let mut score = 0;
            let mut by_user = false;
            for k in &e.keywords {
                if contains_word(&user, k) {
                    score += 2;
                    by_user = true;
                } else if contains_word(&reply, k) {
                    score += 1;
                }
            }
            (score > 0).then_some((score, by_user, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)));

    let mut left = RECALL_TOKENS;
    let mut reply_only = 0;
    let mut out = Vec::new();
    for (_, by_user, i) in scored {
        if out.len() >= MAX_RECALL {
            break;
        }
        if !by_user {
            if reply_only >= MAX_REPLY_ONLY {
                continue;
            }
            reply_only += 1;
        }
        let e = &entries[i];
        let line = format!("{}：{}", e.title, e.text);
        let cost = estimate_tokens(&line);
        if cost > left {
            continue;
        }
        left -= cost;
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# 说明（程序忽略）
## 不算
正文

## 父亲
关键词：父亲、爸爸、中钵、我
他偷走了论文。
后来去了俄罗斯。

## 饮料
关键词: Dr Pepper，AI
喜欢喝。

## 没有关键词
只有正文

## 研究所
关键词：研究所、冈部
在秋叶原。
";

    #[test]
    fn parses_entries_and_skips_unusable_ones() {
        let v = parse(SAMPLE);
        let titles: Vec<&str> = v.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["父亲", "饮料", "研究所"]);
        assert_eq!(
            v[0].keywords,
            vec!["中钵", "父亲", "爸爸"],
            "单字关键词被丢掉"
        );
        assert_eq!(v[0].text, "他偷走了论文。\n后来去了俄罗斯。");
        assert_eq!(
            v[1].keywords,
            vec!["ai", "dr pepper"],
            "英文转小写、支持半角冒号和中文逗号"
        );
    }

    #[test]
    fn recall_by_keywords() {
        let v = parse(SAMPLE);
        assert!(
            recall(&v, "今天天气不错", "").is_empty(),
            "没聊到就不占 token"
        );
        assert_eq!(
            recall(&v, "你爸爸是谁", ""),
            vec!["父亲：他偷走了论文。\n后来去了俄罗斯。"]
        );
        assert_eq!(
            recall(&v, "来瓶 DR PEPPER", ""),
            vec!["饮料：喜欢喝。"],
            "不区分大小写"
        );
    }

    #[test]
    fn ascii_keywords_need_word_boundaries() {
        let v = parse(SAMPLE);
        assert!(recall(&v, "send me an email", "").is_empty());
        assert_eq!(recall(&v, "你是AI吗", "").len(), 1);
        assert_eq!(recall(&v, "AI.", "").len(), 1);
    }

    #[test]
    fn user_hits_rank_first_and_reply_only_hits_are_limited() {
        let v = parse(SAMPLE);
        // 用户提到研究所，上一句回复提到了父亲和 AI：用户的排前面，只靠回复命中的只放一条
        let r = recall(&v, "研究所在哪", "我父亲……还有 AI 的事");
        assert_eq!(r.len(), 2);
        assert!(r[0].starts_with("研究所"));
        assert!(r[1].starts_with("父亲"), "同分按文件顺序");
        // 「他后来呢？」这种追问靠上一句回复接上
        assert_eq!(recall(&v, "他后来呢？", "中钵那个人啊").len(), 1);
    }

    #[test]
    fn recall_respects_limits() {
        let mut src = String::new();
        for i in 0..10 {
            src.push_str(&format!("## 条目{i}\n关键词：共同\n{}\n", "字".repeat(100)));
        }
        let v = parse(&src);
        assert_eq!(recall(&v, "共同", "").len(), MAX_RECALL);
        let big = parse(&format!(
            "## 大\n关键词：共同\n{}\n",
            "字".repeat(RECALL_TOKENS + 1)
        ));
        assert!(recall(&big, "共同", "").is_empty(), "超出预算的条目不放");
        let many: String = (0..MAX_ENTRIES + 5)
            .map(|i| format!("## {i}\n关键词：词{i}\n正文\n"))
            .collect();
        assert_eq!(parse(&many).len(), MAX_ENTRIES);
    }
}
