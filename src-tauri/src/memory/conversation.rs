//! 短期记忆：会话与消息原文（按角色分开）。
//!
//! - 和同一个角色聊天时，距离上一条消息不超过 `GAP_SECS` 就算同一次会话；否则开新会话。
//! - 会话结束（超过间隔没有新消息）后，后台会对它做摘要和事实提取。
//! - 写入前用 `sensitive::scrub` 隐藏密码、验证码等，数据库里不留这些内容。

use super::sensitive;
use crate::backends::{ChatMessage, Role};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

/// 超过 30 分钟没说话就算一次新的会话
pub const GAP_SECS: i64 = 30 * 60;

fn role_str(r: Role) -> &'static str {
    match r {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

fn parse_role(s: &str) -> Role {
    if s == "assistant" {
        Role::Assistant
    } else {
        Role::User
    }
}

/// 当前会话 id：最近一次会话还没结束就继续用，否则新建
pub fn current_or_new(
    conn: &Connection,
    character_id: &str,
    now: i64,
    no_memory: bool,
) -> rusqlite::Result<i64> {
    let last: Option<(i64, i64, i64)> = conn
        .query_row(
            "SELECT id, last_at, no_memory FROM conversations WHERE character_id = ?1 ORDER BY last_at DESC, id DESC LIMIT 1",
            [character_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((id, last_at, nm)) = last
        && now - last_at <= GAP_SECS
        && (nm != 0) == no_memory
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO conversations(character_id, started_at, last_at, no_memory) VALUES (?1, ?2, ?2, ?3)",
        params![character_id, now, no_memory as i64],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 某个角色最近一次会话（不管是否已经结束）
pub fn latest(conn: &Connection, character_id: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM conversations WHERE character_id = ?1 ORDER BY last_at DESC, id DESC LIMIT 1",
        [character_id],
        |r| r.get(0),
    )
    .optional()
}

/// 追加一条消息（内容先脱敏），并更新会话最后活动时间
pub fn append(
    conn: &Connection,
    conversation_id: i64,
    role: Role,
    content: &str,
    now: i64,
) -> rusqlite::Result<i64> {
    let content = sensitive::scrub(content);
    conn.execute(
        "INSERT INTO messages(conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![conversation_id, role_str(role), content, now],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "UPDATE conversations SET last_at = ?2 WHERE id = ?1",
        params![conversation_id, now],
    )?;
    Ok(id)
}

/// 某次会话的全部消息（从旧到新）
pub fn messages(conn: &Connection, conversation_id: i64) -> rusqlite::Result<Vec<ChatMessage>> {
    let mut stmt =
        conn.prepare("SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY id")?;
    stmt.query_map([conversation_id], |r| {
        Ok(ChatMessage {
            role: parse_role(&r.get::<_, String>(0)?),
            content: r.get(1)?,
            image: None,
        })
    })?
    .collect()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoredMessage {
    pub role: Role,
    pub content: String,
    pub created_at: i64,
}

/// 某个角色最近的聊天记录（给聊天面板显示，跨会话），从旧到新
pub fn recent_for_character(
    conn: &Connection,
    character_id: &str,
    limit: i64,
) -> rusqlite::Result<Vec<StoredMessage>> {
    let mut stmt = conn.prepare(
        "SELECT m.role, m.content, m.created_at FROM messages m JOIN conversations c ON c.id = m.conversation_id
         WHERE c.character_id = ?1 ORDER BY m.id DESC LIMIT ?2",
    )?;
    let mut v: Vec<StoredMessage> = stmt
        .query_map(params![character_id, limit], |r| {
            Ok(StoredMessage {
                role: parse_role(&r.get::<_, String>(0)?),
                content: r.get(1)?,
                created_at: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    v.reverse();
    Ok(v)
}

/// 重新生成前：删掉会话末尾的回复和最后一条用户消息，返回那条用户消息
pub fn pop_last_turn(conn: &Connection, conversation_id: i64) -> rusqlite::Result<Option<String>> {
    loop {
        let last: Option<(i64, String, String)> = conn
            .query_row(
                "SELECT id, role, content FROM messages WHERE conversation_id = ?1 ORDER BY id DESC LIMIT 1",
                [conversation_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match last {
            Some((id, role, _)) if role == "assistant" => {
                conn.execute("DELETE FROM messages WHERE id = ?1", [id])?;
            }
            Some((id, _, content)) => {
                conn.execute("DELETE FROM messages WHERE id = ?1", [id])?;
                return Ok(Some(content));
            }
            None => return Ok(None),
        }
    }
}

/// 删除会话里最后一轮「用户消息 + 回复」（用于「忘掉这件事」），返回被删的用户消息
pub fn forget_last_turn(
    conn: &Connection,
    conversation_id: i64,
) -> rusqlite::Result<Option<String>> {
    pop_last_turn(conn, conversation_id)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingConversation {
    pub id: i64,
    pub character_id: String,
    pub started_at: i64,
    pub no_memory: bool,
    pub attempts: i64,
}

/// 已经结束、还没提取记忆的会话（最旧的在前）
pub fn pending(
    conn: &Connection,
    now: i64,
    limit: i64,
) -> rusqlite::Result<Vec<PendingConversation>> {
    let mut stmt = conn.prepare(
        "SELECT id, character_id, started_at, no_memory, extract_attempts FROM conversations
         WHERE extract_state = 0 AND last_at < ?1 ORDER BY last_at LIMIT ?2",
    )?;
    stmt.query_map(params![now - GAP_SECS, limit], |r| {
        Ok(PendingConversation {
            id: r.get(0)?,
            character_id: r.get(1)?,
            started_at: r.get(2)?,
            no_memory: r.get::<_, i64>(3)? != 0,
            attempts: r.get(4)?,
        })
    })?
    .collect()
}

/// 标记提取结果：done = true 完成；false 记一次失败，3 次后放弃
pub fn mark_extracted(conn: &Connection, id: i64, done: bool) -> rusqlite::Result<()> {
    if done {
        conn.execute(
            "UPDATE conversations SET extract_state = 1 WHERE id = ?1",
            [id],
        )?;
    } else {
        conn.execute(
            "UPDATE conversations SET extract_attempts = extract_attempts + 1,
                    extract_state = CASE WHEN extract_attempts + 1 >= 3 THEN 2 ELSE 0 END
             WHERE id = ?1",
            [id],
        )?;
    }
    Ok(())
}

/// 删除整次会话（暂停记忆期间的会话结束后调用）
pub fn delete_conversation(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM messages WHERE conversation_id = ?1", [id])?;
    conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const T: i64 = 1_800_000_000;

    #[test]
    fn same_conversation_within_gap_new_after() {
        let c = db::open_in_memory().unwrap();
        let a = current_or_new(&c, "nailong", T, false).unwrap();
        append(&c, a, Role::User, "你好", T).unwrap();
        assert_eq!(current_or_new(&c, "nailong", T + 60, false).unwrap(), a);
        // 不同角色是不同会话
        assert_ne!(current_or_new(&c, "naiwa", T + 60, false).unwrap(), a);
        // 超过间隔开新会话
        assert_ne!(
            current_or_new(&c, "nailong", T + GAP_SECS + 61, false).unwrap(),
            a
        );
        // 暂停记忆前后的会话分开
        let b = current_or_new(&c, "yui-sao", T, false).unwrap();
        assert_ne!(current_or_new(&c, "yui-sao", T + 1, true).unwrap(), b);
    }

    #[test]
    fn messages_are_scrubbed_and_ordered() {
        let c = db::open_in_memory().unwrap();
        let a = current_or_new(&c, "x", T, false).unwrap();
        append(&c, a, Role::User, "我的密码是 abc12345", T).unwrap();
        append(&c, a, Role::Assistant, "收到", T + 1).unwrap();
        let m = messages(&c, a).unwrap();
        assert_eq!(m.len(), 2);
        assert!(!m[0].content.contains("abc12345"));
        assert_eq!(m[1], ChatMessage::assistant("收到"));
        let r = recent_for_character(&c, "x", 10).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].content, "收到");
    }

    #[test]
    fn pop_last_turn_for_regenerate() {
        let c = db::open_in_memory().unwrap();
        let a = current_or_new(&c, "x", T, false).unwrap();
        append(&c, a, Role::User, "一", T).unwrap();
        append(&c, a, Role::Assistant, "回一", T).unwrap();
        append(&c, a, Role::User, "二", T).unwrap();
        append(&c, a, Role::Assistant, "回二", T).unwrap();
        assert_eq!(pop_last_turn(&c, a).unwrap().as_deref(), Some("二"));
        assert_eq!(messages(&c, a).unwrap().len(), 2);
        append(&c, a, Role::User, "三", T).unwrap();
        assert_eq!(pop_last_turn(&c, a).unwrap().as_deref(), Some("三"));
        assert_eq!(messages(&c, a).unwrap().len(), 2);
    }

    #[test]
    fn pending_and_attempts() {
        let c = db::open_in_memory().unwrap();
        let a = current_or_new(&c, "x", T, false).unwrap();
        assert!(pending(&c, T + 10, 10).unwrap().is_empty(), "还没结束");
        let p = pending(&c, T + GAP_SECS + 1, 10).unwrap();
        assert_eq!(p.len(), 1);
        for _ in 0..3 {
            mark_extracted(&c, a, false).unwrap();
        }
        assert!(
            pending(&c, T + GAP_SECS + 1, 10).unwrap().is_empty(),
            "3 次失败后放弃"
        );
    }
}
