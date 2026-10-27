//! SQLite 连接与结构迁移。
//!
//! - 结构版本号存在 `PRAGMA user_version`。
//! - `MIGRATIONS[i]` 把版本从 i 升到 i+1；只能在末尾追加，**已发布的迁移永远不要修改**。
//! - 对已有数据的库升级前，先用 `VACUUM INTO` 备份一份，升级失败可以回退。

use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// 按顺序执行的迁移脚本。新增表/字段：在末尾追加一项。
pub const MIGRATIONS: &[&str] = &[
    // v1：设置（键值对，值为 JSON）
    r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
];

pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("数据库错误：{0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("文件错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("数据库版本 {found} 比程序支持的 {supported} 更新，请升级 DeskMate")]
    TooNew { found: i64, supported: i64 },
}

/// 打开（或创建）数据库并执行迁移。`backup_dir` 用于存放升级前备份。
pub fn open(path: &Path, backup_dir: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    configure(&conn)?;
    migrate(&conn, Some(backup_dir))?;
    Ok(conn)
}

/// 内存数据库（测试用）
pub fn open_in_memory() -> Result<Connection, DbError> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn, None)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

pub fn user_version(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

pub fn migrate(conn: &Connection, backup_dir: Option<&Path>) -> Result<(), DbError> {
    let current = user_version(conn)?;
    let latest = latest_version();
    if current > latest {
        return Err(DbError::TooNew {
            found: current,
            supported: latest,
        });
    }
    if current == latest {
        return Ok(());
    }
    // 已有数据的库：升级前先备份
    if current > 0
        && let Some(dir) = backup_dir
    {
        std::fs::create_dir_all(dir)?;
        let name = format!(
            "pre-migrate-v{current}-{}.db",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        vacuum_into(conn, &dir.join(name))?;
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let next = i as i64 + 1;
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", next)?;
        tx.commit()?;
        log::info!("数据库已迁移到 v{next}");
    }
    Ok(())
}

/// 把整个数据库一致地复制到 `dest`（不会被并发写入破坏）。
pub fn vacuum_into(conn: &Connection, dest: &Path) -> Result<(), DbError> {
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_db_reaches_latest_version() {
        let conn = open_in_memory().unwrap();
        assert_eq!(user_version(&conn).unwrap(), latest_version());
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn, None).unwrap();
        assert_eq!(user_version(&conn).unwrap(), latest_version());
    }

    #[test]
    fn refuses_newer_db() {
        let conn = open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", latest_version() + 1)
            .unwrap();
        assert!(matches!(migrate(&conn, None), Err(DbError::TooNew { .. })));
    }

    #[test]
    fn upgrade_keeps_data_and_writes_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        let backups = dir.path().join("backups");
        {
            // 模拟旧版本：只执行 v1
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute("INSERT INTO settings(key, value) VALUES ('k', '\"v\"')", [])
                .unwrap();
        }
        let conn = open(&db_path, &backups).unwrap();
        assert_eq!(user_version(&conn).unwrap(), latest_version());
        let v: String = conn
            .query_row("SELECT value FROM settings WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, "\"v\"");
        if latest_version() > 1 {
            assert!(std::fs::read_dir(&backups).unwrap().count() >= 1);
        }
    }

    #[test]
    fn fts5_is_available() {
        let conn = open_in_memory().unwrap();
        conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(x);")
            .unwrap();
    }
}
