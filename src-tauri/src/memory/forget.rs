//! 「忘掉这件事」：识别用户在对话里要求角色忘掉某件事，并删除相关记忆。
//!
//! 判断要保守：「我忘记带钥匙了」是用户自己忘了，不能当成删除记忆的指令。

use super::{Memory, MemoryError, tokenize};
use rusqlite::Connection;

/// 明确是让角色忘掉的说法
const TRIGGERS: &[&str] = &[
    "忘掉",
    "忘了吧",
    "忘了它",
    "别记住",
    "不要记住",
    "别记着",
    "不要记着",
    "别记得",
    "不要记得",
    "请忘记",
    "你忘记",
    "帮我忘记",
    "忘记这件事",
    "忘记这个",
    "忘记它",
    "从记忆里删",
    "从记忆中删",
    "删掉这条记忆",
    "删除这条记忆",
    "删掉关于",
    "别再提",
];

/// 用户在说自己的心情，不是指令
const SELF_WISHES: &[&str] = &[
    "我想忘掉",
    "好想忘掉",
    "真想忘掉",
    "我要忘掉",
    "想忘掉",
    "忘不掉",
    "忘不了",
];

/// 目标前后的虚词
const FILLERS: &[&str] = &[
    "关于",
    "有关",
    "我刚才说的",
    "刚才说的",
    "我说的",
    "我刚说的",
    "这件事",
    "那件事",
    "这个",
    "那个",
    "这些",
    "那些",
    "的事情",
    "的事",
    "事情",
    "记忆",
    "吧",
    "吗",
    "呀",
    "啊",
    "哦",
    "了",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetRequest {
    /// 明确说了要忘掉什么；None 表示「这件事」（指刚才说的）
    pub target: Option<String>,
}

fn strip_fillers(mut s: &str) -> String {
    loop {
        let before = s;
        s = s.trim_matches(|c: char| {
            c.is_whitespace() || "，。,.!！?？:：「」\"'“”、~～".contains(c)
        });
        for f in FILLERS {
            if let Some(r) = s.strip_prefix(f) {
                s = r;
            }
            if let Some(r) = s.strip_suffix(f) {
                s = r;
            }
        }
        if s == before {
            return s.to_string();
        }
    }
}

pub fn detect(text: &str) -> Option<ForgetRequest> {
    let t = text.trim();
    if t.chars().count() > 200 {
        return None; // 长段落多半不是指令
    }
    let pos = TRIGGERS
        .iter()
        .filter_map(|k| t.find(k).map(|p| (p, k.len())))
        .min_by_key(|(p, _)| *p)?;
    // 「我想忘掉他」之类是心情，不是指令（除非明确对角色说「你」）
    if SELF_WISHES.iter().any(|w| t.contains(w)) && !t.contains('你') {
        return None;
    }
    let before = strip_fillers(&t[..pos.0]);
    let after = strip_fillers(&t[pos.0 + pos.1..]);
    // 「把 X 忘掉」→ X 在前面；「忘掉 X」→ X 在后面
    let before = before
        .trim_start_matches("请")
        .trim_start_matches("你")
        .trim_start_matches("帮我")
        .trim_start_matches('把')
        .to_string();
    let before = strip_fillers(&before);
    let target = if after.chars().count() >= 2 {
        Some(after)
    } else if before.chars().count() >= 2 {
        Some(before)
    } else {
        None
    };
    Some(ForgetRequest { target })
}

/// 删除与目标相关的记忆，返回删掉的内容。
/// 不区分角色：用户让一个角色忘掉的事，所有角色都不应该再记得（隐私优先）。
pub fn forget_matching(conn: &Connection, target: &str) -> Result<Vec<Memory>, MemoryError> {
    let Some(expr) = tokenize::match_expr(target) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT m.id FROM memory_fts JOIN memories m ON m.id = memory_fts.rowid
         WHERE memory_fts MATCH ?1
         ORDER BY bm25(memory_fts) LIMIT 30",
    )?;
    let ids: Vec<i64> = stmt
        .query_map([expr], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut removed = Vec::new();
    for id in ids {
        let Some(m) = super::get(conn, id) else {
            continue;
        };
        // 目标里的词至少一半出现在这条记忆里才删，避免误删
        let terms = tokenize::query_terms(target);
        let mterms = tokenize::query_terms(&m.content);
        let hit = terms.iter().filter(|t| mterms.contains(t)).count();
        if !terms.is_empty() && hit * 2 >= terms.len() {
            super::delete(conn, id)?;
            removed.push(m);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::memory::{self, tests::fact};

    #[test]
    fn detects_commands_with_targets() {
        assert_eq!(detect("忘掉这件事吧"), Some(ForgetRequest { target: None }));
        assert_eq!(detect("请忘记这个"), Some(ForgetRequest { target: None }));
        assert_eq!(
            detect("把我住在杭州这件事忘掉"),
            Some(ForgetRequest {
                target: Some("我住在杭州".into())
            })
        );
        assert_eq!(
            detect("忘掉关于我前任的事"),
            Some(ForgetRequest {
                target: Some("我前任".into())
            })
        );
        assert_eq!(
            detect("别记住我的生日"),
            Some(ForgetRequest {
                target: Some("我的生日".into())
            })
        );
    }

    #[test]
    fn ignores_user_forgetting_things() {
        for s in [
            "我忘记带钥匙了",
            "我又忘了密码",
            "我好想忘掉他",
            "这首歌我忘不了",
            "你还记得我吗",
        ] {
            assert_eq!(detect(s), None, "{s}");
        }
        assert!(detect("我想让你忘掉这件事").is_some());
    }

    #[test]
    fn forget_matching_deletes_only_related() {
        let c = db::open_in_memory().unwrap();
        let t = 1_800_000_000;
        memory::insert(&c, &fact("用户住在杭州西湖附近", "global", 3), t).unwrap();
        memory::insert(&c, &fact("用户喜欢杭州的天气", "nailong", 2), t).unwrap();
        memory::insert(&c, &fact("用户养了一只猫", "global", 3), t).unwrap();
        memory::insert(&c, &fact("用户住在杭州的公司宿舍", "naiwa", 2), t).unwrap();
        let removed = forget_matching(&c, "我住在杭州").unwrap();
        let mut contents: Vec<&str> = removed.iter().map(|m| m.content.as_str()).collect();
        contents.sort();
        assert_eq!(
            contents,
            vec!["用户住在杭州的公司宿舍", "用户住在杭州西湖附近"]
        );
        assert_eq!(
            memory::stats(&c).facts,
            2,
            "无关记忆（只提到杭州天气、养猫）都还在"
        );
    }
}
