//! 记忆导入（导出在 memory_cmds.rs）。
//!
//! 导入的文件来自用户自己的导出，但文件可能被改过，所以：
//! - 每条都重新过敏感信息过滤、长度和范围校验；
//! - 来源类型保留：原本不可信的（任务/网页/文件）导入后仍然不可信；
//! - 只导入有效的记录（历史版本不导入），与已有记忆重复的跳过；
//! - 一次最多 5000 条。

use super::{Memory, MemoryError, NewMemory, source};
use rusqlite::Connection;
use serde::Serialize;

pub const MAX_IMPORT: usize = 5000;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ImportReport {
    pub added: usize,
    pub duplicates: usize,
    pub rejected: usize,
}

fn allowed_source(kind: &str) -> &'static str {
    match kind {
        source::CHAT => source::CHAT,
        source::USER_EDIT => source::USER_EDIT,
        source::TASK => source::TASK,
        source::WEB => source::WEB,
        // 其他未知来源一律按「文件内容」处理（不可信）
        _ => source::FILE,
    }
}

pub fn import(
    conn: &Connection,
    items: &[Memory],
    file_label: &str,
    now: i64,
) -> Result<ImportReport, MemoryError> {
    let mut rep = ImportReport::default();
    for m in items.iter().take(MAX_IMPORT) {
        if m.status != "active" {
            continue;
        }
        if m.kind == super::KIND_FACT
            && super::find_similar(conn, &m.content, &m.scope, 0.9)?.is_some()
        {
            rep.duplicates += 1;
            continue;
        }
        let nm = NewMemory {
            kind: m.kind.clone(),
            content: m.content.clone(),
            scope: m.scope.clone(),
            category: m.category.clone(),
            source_kind: allowed_source(&m.source_kind).into(),
            source_ref: format!("import:{file_label}"),
            importance: m.importance,
            day: m.day.chars().take(10).collect(),
        };
        match super::insert(conn, &nm, now) {
            Ok(_) => rep.added += 1,
            Err(MemoryError::Db(e)) => return Err(MemoryError::Db(e)),
            Err(_) => rep.rejected += 1,
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::memory::{self, ListFilter};

    fn mem(content: &str, kind: &str, scope: &str, src: &str, status: &str) -> Memory {
        Memory {
            id: 1,
            kind: kind.into(),
            content: content.into(),
            scope: scope.into(),
            category: "profile".into(),
            source_kind: src.into(),
            source_ref: String::new(),
            trusted: true,
            importance: 4,
            created_at: 0,
            updated_at: 0,
            last_used_at: None,
            use_count: 0,
            status: status.into(),
            superseded_by: None,
            day: "2026-10-01".into(),
        }
    }

    #[test]
    fn imports_with_filters_and_dedupe() {
        let c = db::open_in_memory().unwrap();
        memory::insert(&c, &memory::tests::fact("用户叫小林", "global", 5), 1).unwrap();
        let items = vec![
            mem("用户叫小林", "fact", "global", "chat", "active"), // 重复
            mem("用户喜欢喝拿铁", "fact", "global", "chat", "active"), // 新增
            mem("旧的住址", "fact", "global", "chat", "superseded"), // 历史版本不导入
            mem("网页说用户喜欢红色", "fact", "global", "web", "active"), // 保持不可信
            mem("用户的密码是 abc12345", "fact", "global", "chat", "active"), // 敏感，拒绝
            mem("x", "fact", "../evil", "chat", "active"),         // 范围非法，拒绝
            mem("聊了考试的事", "summary", "nailong", "chat", "active"), // 摘要
            mem("来源被篡改", "fact", "global", "trusted_admin", "active"), // 未知来源 → 不可信
        ];
        let rep = import(&c, &items, "memory-1.json", 10).unwrap();
        assert_eq!(
            rep,
            ImportReport {
                added: 4,
                duplicates: 1,
                rejected: 2
            }
        );
        let all = memory::list(&c, &ListFilter::default()).unwrap().items;
        let red = all.iter().find(|m| m.content.contains("红色")).unwrap();
        assert!(!red.trusted);
        let forged = all.iter().find(|m| m.content.contains("篡改")).unwrap();
        assert!(!forged.trusted, "未知来源按不可信处理");
        assert_eq!(forged.source_ref, "import:memory-1.json");
    }
}
