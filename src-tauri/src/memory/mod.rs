//! 记忆系统（需求第 7 节）。
//!
//! 分层：角色设定（只读，不在这里）→ 短期记忆（`conversation`，最近对话原文）
//! → 对话摘要（kind = summary）→ 长期事实（kind = fact）。
//!
//! 规则：
//! - 关于用户的事实 scope = "global"（所有角色共享）；「和这个角色之间发生过什么」scope = 角色 id。
//! - 新旧信息冲突：新的生效，旧的标记为 superseded（保留为历史，不删除）。
//! - 写入前过滤敏感信息（`sensitive`），命中就不保存。
//! - 来自任务结果 / 网页 / 文件的内容是不可信来源：trusted = false，只能作为带来源的事实，
//!   不会进入「用户档案」，在提示词里标注为不可信。
//! - 用户删除、「忘掉这件事」是真正删除（隐私优先）。

pub mod backup;
pub mod conversation;
pub mod extract;
pub mod forget;
pub mod maintain;
pub mod sensitive;
pub mod tokenize;
pub mod transfer;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const KIND_FACT: &str = "fact";
pub const KIND_SUMMARY: &str = "summary";
pub const SCOPE_GLOBAL: &str = "global";

/// 来源类型
pub mod source {
    pub const CHAT: &str = "chat";
    pub const USER_EDIT: &str = "user_edit";
    pub const TASK: &str = "task";
    pub const FILE: &str = "file";
    pub const WEB: &str = "web";

    /// 只有聊天和用户手动编辑是可信来源
    pub fn is_trusted(kind: &str) -> bool {
        kind == CHAT || kind == USER_EDIT
    }

    pub fn label(kind: &str) -> &'static str {
        match kind {
            CHAT => "聊天",
            USER_EDIT => "你手动填写",
            TASK => "任务结果",
            FILE => "文件内容",
            WEB => "网页",
            _ => "未知来源",
        }
    }
}

/// 事实分类
pub const CATEGORIES: [&str; 6] = ["profile", "preference", "person", "plan", "event", "other"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub kind: String,
    pub content: String,
    pub scope: String,
    pub category: String,
    pub source_kind: String,
    pub source_ref: String,
    pub trusted: bool,
    pub importance: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
    pub use_count: i64,
    pub status: String,
    pub superseded_by: Option<i64>,
    pub day: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewMemory {
    pub kind: String,
    pub content: String,
    pub scope: String,
    pub category: String,
    pub source_kind: String,
    pub source_ref: String,
    pub importance: i64,
    pub day: String,
}

const MAX_CONTENT_CHARS: usize = 600;

const COLS: &str =
    "id, kind, content, scope, category, source_kind, source_ref, trusted, importance,
                    created_at, updated_at, last_used_at, use_count, status, superseded_by, day";

fn row_to_memory(r: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        kind: r.get(1)?,
        content: r.get(2)?,
        scope: r.get(3)?,
        category: r.get(4)?,
        source_kind: r.get(5)?,
        source_ref: r.get(6)?,
        trusted: r.get::<_, i64>(7)? != 0,
        importance: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        last_used_at: r.get(11)?,
        use_count: r.get(12)?,
        status: r.get(13)?,
        superseded_by: r.get(14)?,
        day: r.get(15)?,
    })
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MemoryError {
    #[error("内容为空")]
    Empty,
    #[error("内容包含密码、验证码、证件号、银行卡号或密钥之类的敏感信息，不会保存")]
    Sensitive,
    #[error("参数不合法：{0}")]
    Invalid(String),
    #[error("数据库错误：{0}")]
    Db(String),
}

impl From<rusqlite::Error> for MemoryError {
    fn from(e: rusqlite::Error) -> Self {
        MemoryError::Db(e.to_string())
    }
}

fn clean_content(s: &str) -> Result<String, MemoryError> {
    let t: String = s.trim().chars().take(MAX_CONTENT_CHARS).collect();
    if t.is_empty() {
        return Err(MemoryError::Empty);
    }
    if sensitive::contains_sensitive(&t) {
        return Err(MemoryError::Sensitive);
    }
    Ok(t)
}

fn valid_scope(scope: &str) -> bool {
    scope == SCOPE_GLOBAL || crate::character::valid_id(scope)
}

fn index(conn: &Connection, id: i64, content: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM memory_fts WHERE rowid = ?1", [id])?;
    conn.execute(
        "INSERT INTO memory_fts(rowid, tokens) VALUES (?1, ?2)",
        params![id, tokenize::index_tokens(content)],
    )?;
    Ok(())
}

/// 新增一条记忆。敏感内容直接拒绝（返回 Err(Sensitive)）。
pub fn insert(conn: &Connection, m: &NewMemory, now: i64) -> Result<i64, MemoryError> {
    let content = clean_content(&m.content)?;
    if m.kind != KIND_FACT && m.kind != KIND_SUMMARY {
        return Err(MemoryError::Invalid("kind".into()));
    }
    if !valid_scope(&m.scope) {
        return Err(MemoryError::Invalid("scope".into()));
    }
    // 「日记」只用于摘要类（角色自己写的日记，见 diary.rs）
    let category = if CATEGORIES.contains(&m.category.as_str())
        || (m.kind == KIND_SUMMARY && m.category == "diary")
    {
        m.category.as_str()
    } else {
        "other"
    };
    let trusted = source::is_trusted(&m.source_kind);
    conn.execute(
        "INSERT INTO memories(kind, content, scope, category, source_kind, source_ref, trusted,
                              importance, created_at, updated_at, status, day)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'active', ?10)",
        params![
            m.kind,
            content,
            m.scope,
            category,
            m.source_kind,
            m.source_ref,
            trusted as i64,
            m.importance.clamp(1, 5),
            now,
            m.day,
        ],
    )?;
    let id = conn.last_insert_rowid();
    index(conn, id, &content)?;
    Ok(id)
}

pub fn get(conn: &Connection, id: i64) -> Option<Memory> {
    conn.query_row(
        &format!("SELECT {COLS} FROM memories WHERE id = ?1"),
        [id],
        row_to_memory,
    )
    .optional()
    .ok()
    .flatten()
}

/// 用户在界面上编辑：内容和重要度；来源改为「你手动填写」（可信）
pub fn update(
    conn: &Connection,
    id: i64,
    content: &str,
    importance: i64,
    now: i64,
) -> Result<(), MemoryError> {
    let content = clean_content(content)?;
    let n = conn.execute(
        "UPDATE memories SET content = ?2, importance = ?3, updated_at = ?4,
                source_kind = 'user_edit', trusted = 1
         WHERE id = ?1",
        params![id, content, importance.clamp(1, 5), now],
    )?;
    if n == 0 {
        return Err(MemoryError::Invalid("找不到这条记忆".into()));
    }
    index(conn, id, &content)?;
    Ok(())
}

/// 真正删除（同时删除以它为前身的历史链接）
pub fn delete(conn: &Connection, id: i64) -> Result<(), MemoryError> {
    conn.execute(
        "UPDATE memories SET superseded_by = NULL WHERE superseded_by = ?1",
        [id],
    )?;
    conn.execute("DELETE FROM memory_fts WHERE rowid = ?1", [id])?;
    conn.execute("DELETE FROM memories WHERE id = ?1", [id])?;
    Ok(())
}

/// 新信息取代旧信息：旧的保留为历史
pub fn supersede(conn: &Connection, old_id: i64, new_id: i64, now: i64) -> Result<(), MemoryError> {
    conn.execute(
        "UPDATE memories SET status = 'superseded', superseded_by = ?2, updated_at = ?3 WHERE id = ?1",
        params![old_id, new_id, now],
    )?;
    // 历史版本不参与检索
    conn.execute("DELETE FROM memory_fts WHERE rowid = ?1", [old_id])?;
    Ok(())
}

/// 已有相似事实时只刷新它（返回其 id）
pub fn touch(conn: &Connection, id: i64, importance: i64, now: i64) -> Result<(), MemoryError> {
    conn.execute(
        "UPDATE memories SET updated_at = ?2, importance = MAX(importance, ?3) WHERE id = ?1",
        params![id, now, importance.clamp(1, 5)],
    )?;
    Ok(())
}

pub fn clear_all(conn: &Connection) -> Result<(), MemoryError> {
    conn.execute_batch(
        "DELETE FROM memory_fts; DELETE FROM memories; DELETE FROM messages; DELETE FROM conversations; DELETE FROM voice_ja;",
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ListFilter {
    pub query: String,
    /// fact / summary / 空 = 全部
    pub kind: String,
    /// global / 角色 id / 空 = 全部
    pub scope: String,
    pub include_history: bool,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResult {
    pub items: Vec<Memory>,
    pub total: i64,
}

/// 管理界面的列表与搜索
pub fn list(conn: &Connection, f: &ListFilter) -> Result<ListResult, MemoryError> {
    let mut where_ = vec!["1 = 1".to_string()];
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    if !f.include_history {
        where_.push("status = 'active'".into());
    }
    if !f.kind.is_empty() {
        args.push(f.kind.clone().into());
        where_.push(format!("kind = ?{}", args.len()));
    }
    if !f.scope.is_empty() {
        args.push(f.scope.clone().into());
        where_.push(format!("scope = ?{}", args.len()));
    }
    let q = f.query.trim();
    if !q.is_empty() {
        // 子串匹配最符合「搜索」的直觉；数据量很小，LIKE 足够快
        args.push(format!("%{}%", q.replace('%', "\\%").replace('_', "\\_")).into());
        where_.push(format!("content LIKE ?{} ESCAPE '\\'", args.len()));
    }
    let w = where_.join(" AND ");
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM memories WHERE {w}"),
        rusqlite::params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let limit = if f.limit <= 0 { 50 } else { f.limit.min(500) };
    args.push(limit.into());
    args.push(f.offset.max(0).into());
    let sql = format!(
        "SELECT {COLS} FROM memories WHERE {w} ORDER BY updated_at DESC, id DESC LIMIT ?{} OFFSET ?{}",
        args.len() - 1,
        args.len()
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), row_to_memory)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ListResult { items, total })
}

/// 检索结果
#[derive(Debug, Clone, Default)]
pub struct Retrieved {
    /// 用户档案：可信、全局、重要度 ≥ 4 的事实（稳定，放在可缓存的提示词段落）
    pub profile: Vec<Memory>,
    /// 与本轮话题相关的记忆（按综合得分排序）
    pub related: Vec<(Memory, f64)>,
}

const PROFILE_LIMIT: usize = 20;
const RELATED_LIMIT: usize = 8;

/// 综合打分：相关度 60% + 新近 20% + 重要度 20%
pub fn score(bm25: f64, updated_at: i64, importance: i64, now: i64) -> f64 {
    let r = (-bm25).max(0.0);
    let relevance = r / (r + 3.0);
    let age_days = ((now - updated_at).max(0) as f64) / 86_400.0;
    let recency = 0.5f64.powf(age_days / 30.0);
    0.6 * relevance + 0.2 * recency + 0.2 * (importance.clamp(1, 5) as f64 / 5.0)
}

/// 每轮对话的记忆检索
pub fn retrieve(
    conn: &Connection,
    query: &str,
    character_id: &str,
    now: i64,
) -> Result<Retrieved, MemoryError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM memories
         WHERE status = 'active' AND kind = 'fact' AND trusted = 1 AND scope = 'global' AND importance >= 4
         ORDER BY importance DESC, id ASC LIMIT {PROFILE_LIMIT}"
    ))?;
    let profile: Vec<Memory> = stmt
        .query_map([], row_to_memory)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let profile_ids: Vec<i64> = profile.iter().map(|m| m.id).collect();

    let mut related: Vec<(Memory, f64)> = Vec::new();
    if let Some(expr) = tokenize::match_expr(query) {
        let cols = COLS
            .split(',')
            .map(|c| format!("m.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = conn.prepare(&format!(
            "SELECT {cols}, bm25(memory_fts) FROM memory_fts JOIN memories m ON m.id = memory_fts.rowid
             WHERE memory_fts MATCH ?1 AND m.status = 'active' AND (m.scope = 'global' OR m.scope = ?2)
             ORDER BY bm25(memory_fts) LIMIT 50"
        ))?;
        let rows = stmt.query_map(params![expr, character_id], |r| {
            Ok((row_to_memory(r)?, r.get::<_, f64>(16)?))
        })?;
        for row in rows {
            let (m, bm) = row?;
            if profile_ids.contains(&m.id) {
                continue;
            }
            let s = score(bm, m.updated_at, m.importance, now);
            related.push((m, s));
        }
        related.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        related.truncate(RELATED_LIMIT);
    }
    Ok(Retrieved { profile, related })
}

/// 记录哪些记忆被用到了（维护时据此降低久未使用记忆的权重）
pub fn mark_used(conn: &Connection, ids: &[i64], now: i64) -> Result<(), MemoryError> {
    for id in ids {
        conn.execute(
            "UPDATE memories SET last_used_at = ?2, use_count = use_count + 1 WHERE id = ?1",
            params![id, now],
        )?;
    }
    Ok(())
}

/// 与给定内容高度相似的有效事实（去重用）
pub fn find_similar(
    conn: &Connection,
    content: &str,
    scope: &str,
    threshold: f64,
) -> Result<Option<Memory>, MemoryError> {
    let Some(expr) = tokenize::match_expr(content) else {
        return Ok(None);
    };
    let cols = COLS
        .split(',')
        .map(|c| format!("m.{}", c.trim()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut stmt = conn.prepare(&format!(
        "SELECT {cols} FROM memory_fts JOIN memories m ON m.id = memory_fts.rowid
         WHERE memory_fts MATCH ?1 AND m.status = 'active' AND m.kind = 'fact' AND m.scope = ?2
         ORDER BY bm25(memory_fts) LIMIT 10"
    ))?;
    let found = stmt
        .query_map(params![expr, scope], row_to_memory)?
        .flatten()
        .find(|m| tokenize::overlap(&m.content, content) >= threshold);
    Ok(found)
}

/// 统计
#[derive(Debug, Clone, Serialize, Default)]
pub struct Stats {
    pub facts: i64,
    pub summaries: i64,
    pub history: i64,
    pub conversations: i64,
}

pub fn stats(conn: &Connection) -> Stats {
    let q = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    Stats {
        facts: q("SELECT COUNT(*) FROM memories WHERE kind = 'fact' AND status = 'active'"),
        summaries: q("SELECT COUNT(*) FROM memories WHERE kind = 'summary' AND status = 'active'"),
        history: q("SELECT COUNT(*) FROM memories WHERE status = 'superseded'"),
        conversations: q("SELECT COUNT(*) FROM conversations"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db;

    pub(crate) fn fact(content: &str, scope: &str, importance: i64) -> NewMemory {
        NewMemory {
            kind: KIND_FACT.into(),
            content: content.into(),
            scope: scope.into(),
            category: "profile".into(),
            source_kind: source::CHAT.into(),
            source_ref: "conversation:1".into(),
            importance,
            day: String::new(),
        }
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn insert_rejects_sensitive_and_invalid() {
        let c = db::open_in_memory().unwrap();
        assert_eq!(
            insert(&c, &fact("我的密码是 abc12345", "global", 3), NOW),
            Err(MemoryError::Sensitive)
        );
        assert_eq!(
            insert(&c, &fact("   ", "global", 3), NOW),
            Err(MemoryError::Empty)
        );
        assert!(matches!(
            insert(&c, &fact("x", "../bad", 3), NOW),
            Err(MemoryError::Invalid(_))
        ));
        let id = insert(&c, &fact("用户叫小林", "global", 9), NOW).unwrap();
        let m = get(&c, id).unwrap();
        assert_eq!(m.importance, 5, "重要度被限制在 1~5");
        assert!(m.trusted);
    }

    #[test]
    fn untrusted_sources_are_marked_and_never_profile() {
        let c = db::open_in_memory().unwrap();
        let mut m = fact("网页上说用户喜欢红色", "global", 5);
        m.source_kind = source::WEB.into();
        let id = insert(&c, &m, NOW).unwrap();
        assert!(!get(&c, id).unwrap().trusted);
        let r = retrieve(&c, "红色", "nailong", NOW).unwrap();
        assert!(r.profile.is_empty(), "不可信来源不能进入用户档案");
        assert_eq!(r.related.len(), 1, "但仍可作为带来源的资料被检索到");
    }

    #[test]
    fn retrieval_ranks_and_respects_scope() {
        let c = db::open_in_memory().unwrap();
        insert(&c, &fact("用户叫小林", "global", 5), NOW).unwrap();
        insert(&c, &fact("用户养了一只叫团子的猫", "global", 3), NOW).unwrap();
        insert(
            &c,
            &fact("用户上周和奶龙一起给猫取了外号", "nailong", 2),
            NOW,
        )
        .unwrap();
        insert(&c, &fact("用户住在杭州", "global", 3), NOW).unwrap();
        let r = retrieve(&c, "我家的猫最近不爱吃饭", "naiwa", NOW).unwrap();
        assert_eq!(r.profile.len(), 1);
        assert_eq!(r.profile[0].content, "用户叫小林");
        let contents: Vec<&str> = r.related.iter().map(|(m, _)| m.content.as_str()).collect();
        assert_eq!(contents[0], "用户养了一只叫团子的猫");
        assert!(
            !contents.iter().any(|c| c.contains("奶龙")),
            "其他角色的专属记忆不可见"
        );
        let r = retrieve(&c, "猫", "nailong", NOW).unwrap();
        assert_eq!(r.related.len(), 2, "本角色的专属记忆可见");
    }

    #[test]
    fn newer_and_more_important_scores_higher() {
        let old = score(-2.0, NOW - 90 * 86_400, 3, NOW);
        let new = score(-2.0, NOW, 3, NOW);
        assert!(new > old);
        assert!(score(-2.0, NOW, 5, NOW) > score(-2.0, NOW, 1, NOW));
        assert!(score(-8.0, NOW, 3, NOW) > score(-1.0, NOW, 3, NOW));
    }

    #[test]
    fn supersede_keeps_history_but_hides_from_search() {
        let c = db::open_in_memory().unwrap();
        let old = insert(&c, &fact("用户住在杭州", "global", 3), NOW).unwrap();
        let new = insert(&c, &fact("用户搬到了上海住", "global", 3), NOW + 10).unwrap();
        supersede(&c, old, new, NOW + 10).unwrap();
        let r = retrieve(&c, "住在哪里 杭州 上海", "x", NOW + 20).unwrap();
        let ids: Vec<i64> = r.related.iter().map(|(m, _)| m.id).collect();
        assert_eq!(ids, vec![new]);
        let all = list(
            &c,
            &ListFilter {
                include_history: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.total, 2);
        assert_eq!(get(&c, old).unwrap().superseded_by, Some(new));
        let active = list(&c, &ListFilter::default()).unwrap();
        assert_eq!(active.total, 1);
    }

    #[test]
    fn update_delete_and_search() {
        let c = db::open_in_memory().unwrap();
        let id = insert(&c, &fact("用户喜欢喝咖啡", "global", 2), NOW).unwrap();
        update(&c, id, "用户喜欢喝拿铁", 4, NOW + 1).unwrap();
        let m = get(&c, id).unwrap();
        assert_eq!(m.source_kind, "user_edit");
        assert_eq!(m.importance, 4);
        assert_eq!(retrieve(&c, "拿铁", "x", NOW).unwrap().profile.len(), 1);
        assert_eq!(
            update(&c, id, "密码是 hunter2abc", 3, NOW),
            Err(MemoryError::Sensitive)
        );
        let found = list(
            &c,
            &ListFilter {
                query: "拿铁".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(found.total, 1);
        let none = list(
            &c,
            &ListFilter {
                query: "100%".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(none.total, 0);
        delete(&c, id).unwrap();
        assert!(get(&c, id).is_none());
        assert!(retrieve(&c, "拿铁", "x", NOW).unwrap().profile.is_empty());
    }

    #[test]
    fn similar_detection() {
        let c = db::open_in_memory().unwrap();
        insert(&c, &fact("用户养了一只猫", "global", 3), NOW).unwrap();
        assert!(
            find_similar(&c, "用户养了一只猫咪", "global", 0.7)
                .unwrap()
                .is_some()
        );
        assert!(
            find_similar(&c, "用户养了一只猫咪", "nailong", 0.7)
                .unwrap()
                .is_none()
        );
        assert!(
            find_similar(&c, "用户喜欢跑步", "global", 0.7)
                .unwrap()
                .is_none()
        );
    }
}
