//! 记忆维护（M8）：每天在用户空闲时运行一次。
//! - 合并重复的事实（内容几乎一样的只留一条，另一条标为历史）
//! - 降低久未使用记忆的重要度（不动用户手动写的、不动重要度 ≥ 4 的）
//! - 压缩旧摘要（同一个月、同一个角色的多条旧摘要合成一条）
//! - 清理很旧的原始聊天记录（已经整理过记忆的会话，超过 180 天只留摘要）

use super::{KIND_FACT, KIND_SUMMARY, Memory, MemoryError, NewMemory, source, tokenize};
use rusqlite::{Connection, params};

const DAY: i64 = 86_400;
/// 多久没用到就降低重要度
pub const DECAY_AFTER_DAYS: i64 = 90;
/// 摘要多旧才压缩
pub const SUMMARY_COMPRESS_AFTER_DAYS: i64 = 60;
/// 同一个月至少几条摘要才压缩
pub const SUMMARY_COMPRESS_MIN: usize = 4;
/// 原始聊天记录保留多久
pub const KEEP_MESSAGES_DAYS: i64 = 180;
const DUP_THRESHOLD: f64 = 0.85;
const MAX_FACTS_PER_SCOPE: usize = 500;

fn active_facts(conn: &Connection, scope: &str) -> Result<Vec<Memory>, MemoryError> {
    let r = super::list(
        conn,
        &super::ListFilter {
            kind: KIND_FACT.into(),
            scope: scope.into(),
            limit: MAX_FACTS_PER_SCOPE as i64,
            ..Default::default()
        },
    )?;
    Ok(r.items)
}

fn scopes(conn: &Connection) -> Result<Vec<String>, MemoryError> {
    let mut stmt = conn.prepare("SELECT DISTINCT scope FROM memories WHERE status = 'active'")?;
    Ok(stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// 合并重复事实：保留更重要 / 更新 / 用户手动写的那条。返回合并掉的条数。
pub fn merge_duplicates(conn: &Connection, now: i64) -> Result<usize, MemoryError> {
    let mut merged = 0;
    for scope in scopes(conn)? {
        let facts = active_facts(conn, &scope)?;
        let mut gone: Vec<i64> = Vec::new();
        for i in 0..facts.len() {
            if gone.contains(&facts[i].id) {
                continue;
            }
            for j in i + 1..facts.len() {
                if gone.contains(&facts[j].id) {
                    continue;
                }
                let (a, b) = (&facts[i], &facts[j]);
                if tokenize::overlap(&a.content, &b.content) < DUP_THRESHOLD {
                    continue;
                }
                let rank = |m: &Memory| {
                    (
                        m.source_kind == source::USER_EDIT,
                        m.trusted,
                        m.importance,
                        m.updated_at,
                    )
                };
                let (keep, drop) = if rank(a) >= rank(b) { (a, b) } else { (b, a) };
                super::supersede(conn, drop.id, keep.id, now)?;
                super::touch(conn, keep.id, drop.importance, now)?;
                gone.push(drop.id);
                merged += 1;
                if drop.id == a.id {
                    break;
                }
            }
        }
    }
    Ok(merged)
}

/// 降低久未使用记忆的重要度（每条最多每 90 天降一次，最低到 1）。返回调整的条数。
pub fn decay_unused(conn: &Connection, now: i64) -> Result<usize, MemoryError> {
    let cutoff = now - DECAY_AFTER_DAYS * DAY;
    let n = conn.execute(
        "UPDATE memories SET importance = importance - 1, decayed_at = ?1
         WHERE status = 'active' AND kind = 'fact' AND source_kind != 'user_edit'
           AND importance BETWEEN 2 AND 3
           AND COALESCE(last_used_at, created_at) < ?2
           AND (decayed_at IS NULL OR decayed_at < ?2)",
        params![now, cutoff],
    )?;
    Ok(n)
}

/// 一组需要压缩的旧摘要（同一个范围、同一个月）
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryGroup {
    pub scope: String,
    pub month: String,
    pub items: Vec<Memory>,
}

/// 找出可以压缩的旧摘要组
pub fn summary_groups(conn: &Connection, now: i64) -> Result<Vec<SummaryGroup>, MemoryError> {
    let cutoff = now - SUMMARY_COMPRESS_AFTER_DAYS * DAY;
    let all = super::list(
        conn,
        &super::ListFilter {
            kind: KIND_SUMMARY.into(),
            limit: 5000,
            ..Default::default()
        },
    )?
    .items;
    let mut groups: Vec<SummaryGroup> = Vec::new();
    for m in all
        .into_iter()
        .filter(|m| m.created_at < cutoff && m.day.len() >= 7)
    {
        let month = m.day[..7].to_string();
        match groups
            .iter_mut()
            .find(|g| g.scope == m.scope && g.month == month)
        {
            Some(g) => g.items.push(m),
            None => groups.push(SummaryGroup {
                scope: m.scope.clone(),
                month,
                items: vec![m],
            }),
        }
    }
    groups.retain(|g| g.items.len() >= SUMMARY_COMPRESS_MIN);
    for g in &mut groups {
        g.items.sort_by(|a, b| a.day.cmp(&b.day));
    }
    Ok(groups)
}

/// 不调用模型的压缩：按日期拼接，截断到 300 字
pub fn concat_summary(g: &SummaryGroup) -> String {
    let joined = g
        .items
        .iter()
        .map(|m| {
            format!(
                "{}：{}",
                &m.day[5..],
                m.content.trim().trim_end_matches('。')
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    let body: String = joined.chars().take(280).collect();
    format!("{} 的聊天回顾：{body}", g.month)
}

/// 用压缩后的内容替换这一组摘要（旧摘要标为历史，可在「显示历史版本」里看到）
pub fn replace_group(
    conn: &Connection,
    g: &SummaryGroup,
    content: &str,
    now: i64,
) -> Result<i64, MemoryError> {
    let m = NewMemory {
        kind: KIND_SUMMARY.into(),
        content: content.into(),
        scope: g.scope.clone(),
        category: "event".into(),
        source_kind: source::CHAT.into(),
        source_ref: format!("compressed:{}", g.month),
        importance: 2,
        day: format!("{}-01", g.month),
    };
    let id = super::insert(conn, &m, now)?;
    for old in &g.items {
        super::supersede(conn, old.id, id, now)?;
    }
    Ok(id)
}

/// 删除很旧的原始聊天记录（只删已经整理过记忆的会话）。返回删除的会话数。
pub fn prune_old_messages(conn: &Connection, now: i64) -> Result<usize, MemoryError> {
    let cutoff = now - KEEP_MESSAGES_DAYS * DAY;
    let ids: Vec<i64> = {
        let mut stmt =
            conn.prepare("SELECT id FROM conversations WHERE extract_state != 0 AND last_at < ?1")?;
        stmt.query_map([cutoff], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for id in &ids {
        super::conversation::delete_conversation(conn, *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::memory::{self, ListFilter, tests::fact};

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn merges_near_duplicates_keeping_the_better_one() {
        let c = db::open_in_memory().unwrap();
        let a =
            memory::insert(&c, &fact("用户养了一只橘猫叫团子", "global", 3), NOW - 100).unwrap();
        let mut f = fact("用户养了一只橘猫叫团子呀", "global", 4);
        f.source_kind = source::USER_EDIT.into();
        let b = memory::insert(&c, &f, NOW).unwrap();
        memory::insert(&c, &fact("用户养了一只橘猫叫团子", "nailong", 2), NOW).unwrap(); // 不同范围不合并
        memory::insert(&c, &fact("用户住在杭州", "global", 3), NOW).unwrap();
        assert_eq!(merge_duplicates(&c, NOW + 1).unwrap(), 1);
        assert_eq!(memory::get(&c, a).unwrap().status, "superseded");
        assert_eq!(
            memory::get(&c, b).unwrap().status,
            "active",
            "用户手动写的优先保留"
        );
        assert_eq!(memory::stats(&c).facts, 3);
    }

    #[test]
    fn decays_only_old_unused_low_importance_facts() {
        let c = db::open_in_memory().unwrap();
        let old = NOW - 200 * DAY;
        let a = memory::insert(&c, &fact("用户上过一次瑜伽课", "global", 3), old).unwrap();
        let b = memory::insert(&c, &fact("用户叫小林", "global", 5), old).unwrap();
        let mut f = fact("用户手动写的事", "global", 3);
        f.source_kind = source::USER_EDIT.into();
        let u = memory::insert(&c, &f, old).unwrap();
        let fresh = memory::insert(&c, &fact("用户最近在学钢琴", "global", 3), NOW).unwrap();
        assert_eq!(decay_unused(&c, NOW).unwrap(), 1);
        assert_eq!(memory::get(&c, a).unwrap().importance, 2);
        assert_eq!(memory::get(&c, b).unwrap().importance, 5);
        assert_eq!(memory::get(&c, u).unwrap().importance, 3);
        assert_eq!(memory::get(&c, fresh).unwrap().importance, 3);
        // 刚降过的不会马上再降
        assert_eq!(decay_unused(&c, NOW + DAY).unwrap(), 0);
    }

    #[test]
    fn compresses_old_summary_groups() {
        let c = db::open_in_memory().unwrap();
        let old = NOW - 100 * DAY;
        for d in 1..=5 {
            let m = NewMemory {
                kind: KIND_SUMMARY.into(),
                content: format!("第{d}次聊了复习计划。"),
                scope: "nailong".into(),
                category: "event".into(),
                source_kind: source::CHAT.into(),
                source_ref: String::new(),
                importance: 2,
                day: format!("2026-07-{d:02}"),
            };
            memory::insert(&c, &m, old).unwrap();
        }
        let groups = summary_groups(&c, NOW).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].month, "2026-07");
        let text = concat_summary(&groups[0]);
        assert!(text.starts_with("2026-07 的聊天回顾：07-01：第1次聊了复习计划"));
        replace_group(&c, &groups[0], &text, NOW).unwrap();
        let active = memory::list(
            &c,
            &ListFilter {
                kind: "summary".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(active.total, 1);
        assert!(summary_groups(&c, NOW).unwrap().is_empty());
    }

    #[test]
    fn prunes_only_old_processed_conversations() {
        let c = db::open_in_memory().unwrap();
        let old = NOW - 200 * DAY;
        let a = memory::conversation::current_or_new(&c, "x", old, false).unwrap();
        memory::conversation::append(&c, a, crate::backends::Role::User, "很久以前", old).unwrap();
        memory::conversation::mark_extracted(&c, a, true).unwrap();
        let b = memory::conversation::current_or_new(&c, "y", old, false).unwrap(); // 还没整理，保留
        memory::conversation::append(&c, b, crate::backends::Role::User, "还没整理", old).unwrap();
        assert_eq!(prune_old_messages(&c, NOW).unwrap(), 1);
        assert_eq!(memory::stats(&c).conversations, 1);
    }
}
