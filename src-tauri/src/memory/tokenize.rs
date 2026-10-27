//! 中文友好的全文检索切分（见 DECISIONS #4）。
//!
//! SQLite FTS5 自带的 unicode61 分词器不会切中文（整句会变成一个词），
//! 所以写入索引前在这里把中日韩字符切成「单字 + 相邻二字」，拉丁字母和数字按单词切；
//! 查询时同样处理后用 OR 连接。不需要任何词典，体积为零。

use crate::backends::is_cjk;

/// 查询时忽略的常见二字词 / 英文词（太常见，没有区分度）
const STOP: &[&str] = &[
    "我们", "你们", "他们", "什么", "这个", "那个", "一个", "没有", "就是", "可以", "因为", "所以",
    "但是", "然后", "还是", "如果", "怎么", "这样", "那样", "现在", "今天", "一下", "知道", "觉得",
    "是不", "不是", "有点", "一点", "的话", "了吗", "吗吗", "呢呢", "the", "and", "for", "you",
    "are", "was", "with", "this", "that", "have", "what", "how",
];

enum Run {
    Cjk(Vec<char>),
    Word(String),
}

fn runs(text: &str) -> Vec<Run> {
    let mut out = Vec::new();
    let mut cjk: Vec<char> = Vec::new();
    let mut word = String::new();
    let flush = |cjk: &mut Vec<char>, word: &mut String, out: &mut Vec<Run>| {
        if !cjk.is_empty() {
            out.push(Run::Cjk(std::mem::take(cjk)));
        }
        if !word.is_empty() {
            out.push(Run::Word(std::mem::take(word)));
        }
    };
    for c in text.chars() {
        if is_cjk(c) {
            if !word.is_empty() {
                out.push(Run::Word(std::mem::take(&mut word)));
            }
            cjk.push(c);
        } else if c.is_alphanumeric() {
            if !cjk.is_empty() {
                out.push(Run::Cjk(std::mem::take(&mut cjk)));
            }
            word.extend(c.to_lowercase());
        } else {
            flush(&mut cjk, &mut word, &mut out);
        }
    }
    flush(&mut cjk, &mut word, &mut out);
    out
}

/// 写入索引用的文本：单字 + 二字组合 + 英文/数字单词，空格分隔
pub fn index_tokens(text: &str) -> String {
    let mut toks: Vec<String> = Vec::new();
    for r in runs(text) {
        match r {
            Run::Cjk(cs) => {
                for c in &cs {
                    toks.push(c.to_string());
                }
                for w in cs.windows(2) {
                    toks.push(w.iter().collect());
                }
            }
            Run::Word(w) => toks.push(w),
        }
    }
    toks.join(" ")
}

/// 查询词：中文取二字组合（单个字的片段取单字），英文取长度 ≥ 2 的单词；去掉停用词、去重
pub fn query_terms(text: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let push = |t: String, terms: &mut Vec<String>| {
        if !STOP.contains(&t.as_str()) && !terms.contains(&t) {
            terms.push(t);
        }
    };
    for r in runs(text) {
        match r {
            Run::Cjk(cs) if cs.len() == 1 => push(cs[0].to_string(), &mut terms),
            Run::Cjk(cs) => {
                for w in cs.windows(2) {
                    push(w.iter().collect(), &mut terms);
                }
            }
            Run::Word(w) if w.chars().count() >= 2 => push(w, &mut terms),
            Run::Word(_) => {}
        }
    }
    terms.truncate(32);
    terms
}

/// 组装 FTS5 的 MATCH 表达式：每个词加引号，用 OR 连接。没有可用的词时返回 None。
pub fn match_expr(text: &str) -> Option<String> {
    let terms = query_terms(text);
    if terms.is_empty() {
        return None;
    }
    Some(
        terms
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// 两段文本的词重合度（0~1，按较短的一方计算）。用于判断记忆是否重复。
pub fn overlap(a: &str, b: &str) -> f64 {
    let ta = query_terms(a);
    let tb = query_terms(b);
    if ta.is_empty() || tb.is_empty() {
        return if a.trim() == b.trim() { 1.0 } else { 0.0 };
    }
    let common = ta.iter().filter(|t| tb.contains(t)).count();
    common as f64 / ta.len().min(tb.len()) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_has_unigrams_and_bigrams() {
        assert_eq!(index_tokens("喜欢猫"), "喜 欢 猫 喜欢 欢猫");
        assert_eq!(index_tokens("I love Rust编程!"), "i love rust 编 程 编程");
        assert_eq!(index_tokens("  ，。！ "), "");
    }

    #[test]
    fn query_terms_skip_stopwords_and_dedupe() {
        assert_eq!(query_terms("我喜欢猫"), vec!["我喜", "喜欢", "欢猫"]);
        assert_eq!(query_terms("猫"), vec!["猫"]);
        assert_eq!(query_terms("什么什么"), vec!["么什"]);
        assert_eq!(query_terms("the Python and a"), vec!["python"]);
    }

    #[test]
    fn match_expr_is_quoted_or() {
        assert_eq!(match_expr("小猫").as_deref(), Some("\"小猫\""));
        assert_eq!(
            match_expr("喜欢 rust").as_deref(),
            Some("\"喜欢\" OR \"rust\"")
        );
        assert_eq!(match_expr("，。"), None);
        // 引号等特殊字符不会进入表达式
        assert_eq!(match_expr("a\"b OR c"), Some("\"or\"".to_string()));
    }

    #[test]
    fn overlap_ratio() {
        assert!(overlap("用户喜欢猫", "用户很喜欢猫") > 0.6);
        assert!(overlap("用户养了一只狗", "用户住在上海") < 0.3);
        assert_eq!(overlap("", ""), 1.0);
    }
}
