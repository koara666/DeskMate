//! 用量统计与花费上限。
//! 每次调用模型后记录 token 和估算费用（美元）；调用前检查当天/当月是否已到上限。

use crate::backends::{BackendError, Usage};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// 花费上限（美元）。0 表示不限制。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Budget {
    pub daily_usd: f64,
    pub monthly_usd: f64,
}

impl Default for Budget {
    fn default() -> Self {
        // 默认上限保守一些（日常聊天远用不到），用户可以在设置里调
        Self {
            daily_usd: 1.0,
            monthly_usd: 10.0,
        }
    }
}

impl Budget {
    pub fn sanitize(self) -> Self {
        let fix = |v: f64| {
            if v.is_finite() && v >= 0.0 {
                v.min(100_000.0)
            } else {
                0.0
            }
        };
        Self {
            daily_usd: fix(self.daily_usd),
            monthly_usd: fix(self.monthly_usd),
        }
    }
}

/// 用途（便于用户看清钱花在哪）
pub mod purpose {
    pub const CHAT: &str = "chat";
    pub const MEMORY: &str = "memory";
    pub const TEST: &str = "test";
    pub const TASK: &str = "task";
}

pub struct Record<'a> {
    pub backend_id: &'a str,
    pub model: &'a str,
    pub purpose: &'a str,
    pub usage: Usage,
    pub cost_usd: f64,
}

/// 当前本地日期和月份，如 ("2026-10-27", "2026-10")
pub fn today() -> (String, String) {
    let now = chrono::Local::now();
    (
        now.format("%Y-%m-%d").to_string(),
        now.format("%Y-%m").to_string(),
    )
}

pub fn record(conn: &Connection, r: &Record<'_>) -> rusqlite::Result<()> {
    let (day, month) = today();
    record_at(conn, r, &day, &month)
}

pub fn record_at(
    conn: &Connection,
    r: &Record<'_>,
    day: &str,
    month: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO usage(ts, day, month, backend_id, model, purpose,
                           input_tokens, cached_input_tokens, cache_write_tokens, output_tokens, cost_usd)
         VALUES (strftime('%s','now'), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            day,
            month,
            r.backend_id,
            r.model,
            r.purpose,
            r.usage.input_tokens as i64,
            r.usage.cached_input_tokens as i64,
            r.usage.cache_write_input_tokens as i64,
            r.usage.output_tokens as i64,
            if r.cost_usd.is_finite() { r.cost_usd.max(0.0) } else { 0.0 },
        ],
    )?;
    Ok(())
}

pub fn spent_day(conn: &Connection, day: &str) -> f64 {
    conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM usage WHERE day = ?1",
        [day],
        |r| r.get(0),
    )
    .unwrap_or(0.0)
}

pub fn spent_month(conn: &Connection, month: &str) -> f64 {
    conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM usage WHERE month = ?1",
        [month],
        |r| r.get(0),
    )
    .unwrap_or(0.0)
}

/// 调用前检查：已经花到上限就拒绝
pub fn check_budget(
    conn: &Connection,
    b: &Budget,
    day: &str,
    month: &str,
) -> Result<(), BackendError> {
    let d = spent_day(conn, day);
    if b.daily_usd > 0.0 && d >= b.daily_usd {
        return Err(BackendError::BudgetExceeded(format!(
            "今天已花费约 ${d:.4}，每日上限 ${:.2}",
            b.daily_usd
        )));
    }
    let m = spent_month(conn, month);
    if b.monthly_usd > 0.0 && m >= b.monthly_usd {
        return Err(BackendError::BudgetExceeded(format!(
            "本月已花费约 ${m:.4}，每月上限 ${:.2}",
            b.monthly_usd
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Row {
    pub backend_id: String,
    pub model: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
}

/// 按后端和模型汇总。`column` 是 day / month，`value` 为空时统计全部。
pub fn summary(conn: &Connection, column: &str, value: Option<&str>) -> Vec<Row> {
    let col = if column == "month" { "month" } else { "day" };
    let sql = format!(
        "SELECT backend_id, model, COUNT(*), SUM(input_tokens), SUM(cached_input_tokens), SUM(output_tokens), SUM(cost_usd)
         FROM usage {} GROUP BY backend_id, model ORDER BY SUM(cost_usd) DESC",
        if value.is_some() { format!("WHERE {col} = ?1") } else { String::new() }
    );
    let map = |r: &rusqlite::Row<'_>| {
        Ok(Row {
            backend_id: r.get(0)?,
            model: r.get(1)?,
            requests: r.get(2)?,
            input_tokens: r.get(3)?,
            cached_input_tokens: r.get(4)?,
            output_tokens: r.get(5)?,
            cost_usd: r.get(6)?,
        })
    };
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let rows = match value {
        Some(v) => stmt.query_map([v], map).map(|it| it.flatten().collect()),
        None => stmt.query_map([], map).map(|it| it.flatten().collect()),
    };
    rows.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn rec(cost: f64) -> Record<'static> {
        Record {
            backend_id: "deepseek",
            model: "deepseek-flash",
            purpose: purpose::CHAT,
            usage: Usage {
                input_tokens: 1000,
                output_tokens: 100,
                cached_input_tokens: 600,
                cache_write_input_tokens: 0,
            },
            cost_usd: cost,
        }
    }

    #[test]
    fn records_and_sums() {
        let c = db::open_in_memory().unwrap();
        record_at(&c, &rec(0.01), "2026-10-27", "2026-10").unwrap();
        record_at(&c, &rec(0.02), "2026-10-27", "2026-10").unwrap();
        record_at(&c, &rec(0.5), "2026-10-01", "2026-10").unwrap();
        record_at(&c, &rec(9.0), "2026-09-30", "2026-09").unwrap();
        assert!((spent_day(&c, "2026-10-27") - 0.03).abs() < 1e-9);
        assert!((spent_month(&c, "2026-10") - 0.53).abs() < 1e-9);
        let rows = summary(&c, "month", Some("2026-10"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].requests, 3);
        assert_eq!(rows[0].input_tokens, 3000);
        assert_eq!(summary(&c, "day", None)[0].requests, 4);
    }

    #[test]
    fn budget_blocks_when_reached() {
        let c = db::open_in_memory().unwrap();
        let b = Budget {
            daily_usd: 0.05,
            monthly_usd: 1.0,
        };
        assert!(check_budget(&c, &b, "2026-10-27", "2026-10").is_ok());
        record_at(&c, &rec(0.05), "2026-10-27", "2026-10").unwrap();
        let e = check_budget(&c, &b, "2026-10-27", "2026-10").unwrap_err();
        assert_eq!(e.kind(), "budget_exceeded");
        // 第二天恢复
        assert!(check_budget(&c, &b, "2026-10-28", "2026-10").is_ok());
        // 月上限
        record_at(&c, &rec(1.0), "2026-10-20", "2026-10").unwrap();
        assert!(check_budget(&c, &b, "2026-10-28", "2026-10").is_err());
        // 0 = 不限制
        let unlimited = Budget {
            daily_usd: 0.0,
            monthly_usd: 0.0,
        };
        assert!(check_budget(&c, &unlimited, "2026-10-27", "2026-10").is_ok());
    }

    #[test]
    fn negative_or_nan_values_are_sanitized() {
        let b = Budget {
            daily_usd: -1.0,
            monthly_usd: f64::NAN,
        }
        .sanitize();
        assert_eq!(
            b,
            Budget {
                daily_usd: 0.0,
                monthly_usd: 0.0
            }
        );
        let c = db::open_in_memory().unwrap();
        record_at(&c, &rec(f64::NAN), "d", "m").unwrap();
        record_at(&c, &rec(-5.0), "d", "m").unwrap();
        assert_eq!(spent_day(&c, "d"), 0.0);
    }
}
