//! 活动日志：只存汇总（日期、分类、清洗过的程序名、开始时间、时长），不存窗口标题。
//! 用于日记和关怀；用户可以在设置里查看、删除，或者一键关掉记录。

use rusqlite::{Connection, params};
use serde::Serialize;

/// 新建或更新一段活动的时长，返回行号
#[allow(clippy::too_many_arguments)]
pub fn upsert(
    db: &Connection,
    row: Option<i64>,
    day: &str,
    category: &str,
    label: &str,
    start: i64,
    secs: i64,
) -> rusqlite::Result<i64> {
    if let Some(id) = row {
        db.execute(
            "UPDATE activity_log SET secs = ?2 WHERE id = ?1",
            params![id, secs],
        )?;
        Ok(id)
    } else {
        db.execute(
            "INSERT INTO activity_log(day, category, label, start, secs) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![day, category, label, start, secs],
        )?;
        Ok(db.last_insert_rowid())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Total {
    pub category: String,
    pub label: String,
    pub secs: i64,
}

/// 某一天按分类和程序汇总（时长从多到少）
pub fn day_totals(db: &Connection, day: &str) -> Vec<Total> {
    let Ok(mut stmt) = db.prepare(
        "SELECT category, label, SUM(secs) FROM activity_log WHERE day = ?1
         GROUP BY category, label ORDER BY SUM(secs) DESC",
    ) else {
        return Vec::new();
    };
    stmt.query_map([day], |r| {
        Ok(Total {
            category: r.get(0)?,
            label: r.get(1)?,
            secs: r.get(2)?,
        })
    })
    .map(|it| it.flatten().collect())
    .unwrap_or_default()
}

/// 最近有记录的几天
pub fn recent_days(db: &Connection, limit: i64) -> Vec<String> {
    let Ok(mut stmt) =
        db.prepare("SELECT DISTINCT day FROM activity_log ORDER BY day DESC LIMIT ?1")
    else {
        return Vec::new();
    };
    stmt.query_map([limit], |r| r.get(0))
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

pub fn delete_day(db: &Connection, day: &str) -> rusqlite::Result<usize> {
    db.execute("DELETE FROM activity_log WHERE day = ?1", [day])
}

pub fn delete_all(db: &Connection) -> rusqlite::Result<usize> {
    db.execute("DELETE FROM activity_log", [])
}

/// 时长的说法：「2 小时」「1 小时 20 分钟」「35 分钟」
pub fn duration_text(secs: i64) -> String {
    let m = (secs / 60).max(1);
    match (m / 60, m % 60) {
        (0, m) => format!("{m} 分钟"),
        (h, 0) => format!("{h} 小时"),
        (h, m) => format!("{h} 小时 {m} 分钟"),
    }
}

/// 一天的活动用一句话概括（日记、播报用）：只说分类和清洗过的程序名
pub fn day_summary_text(totals: &[Total]) -> String {
    totals
        .iter()
        .filter(|t| t.category != "idle" && t.category != "other" && t.secs >= 10 * 60)
        .take(5)
        .map(|t| {
            let name = super::classify::Category::from_id(&t.category)
                .map(|c| c.name())
                .unwrap_or("用电脑");
            if t.label.is_empty() {
                format!("{name} {}", duration_text(t.secs))
            } else {
                format!("{name}（{}）{}", t.label, duration_text(t.secs))
            }
        })
        .collect::<Vec<_>>()
        .join("、")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_summarize() {
        let db = crate::db::open_in_memory().unwrap();
        let id = upsert(&db, None, "2026-10-28", "game", "Hades", 100, 600).unwrap();
        upsert(&db, Some(id), "2026-10-28", "game", "Hades", 100, 7200).unwrap();
        upsert(&db, None, "2026-10-28", "coding", "VS Code", 9000, 1800).unwrap();
        upsert(&db, None, "2026-10-28", "other", "", 9000, 9999).unwrap();
        let t = day_totals(&db, "2026-10-28");
        assert_eq!(t[0].secs, 9999);
        assert_eq!(
            day_summary_text(&t),
            "玩游戏（Hades）2 小时、写代码（VS Code）30 分钟"
        );
        assert_eq!(recent_days(&db, 7), vec!["2026-10-28"]);
        assert_eq!(delete_day(&db, "2026-10-28").unwrap(), 3);
        assert!(day_totals(&db, "2026-10-28").is_empty());
    }

    #[test]
    fn durations() {
        assert_eq!(duration_text(30), "1 分钟");
        assert_eq!(duration_text(35 * 60), "35 分钟");
        assert_eq!(duration_text(7200), "2 小时");
        assert_eq!(duration_text(4800), "1 小时 20 分钟");
    }
}
