//! 数据库备份与损坏恢复（需求第 7 节「维护」）。
//!
//! - 每天第一次运行时备份一份 `deskmate-YYYYMMDD.db`，只保留最近 7 份。
//! - 打开数据库时检查完整性；打不开或检查不通过，就把坏文件改名保留，
//!   从最新的可用备份恢复。没有可用备份时新建空库（坏文件仍保留，方便人工抢救）。

use crate::db::{self, DbError};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub const DAILY_PREFIX: &str = "deskmate-";
pub const KEEP_DAILY: usize = 7;
const KEEP_PRE_MIGRATE: usize = 3;

/// 完整性检查（quick_check 比 integrity_check 快很多，足够发现常见损坏）
pub fn integrity_ok(conn: &Connection) -> bool {
    conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
        .map(|s| s == "ok")
        .unwrap_or(false)
}

fn backups_sorted(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".db"))
                })
                .collect()
        })
        .unwrap_or_default();
    // 文件名里带日期时间，字典序就是时间顺序
    v.sort();
    v
}

fn prune(dir: &Path, prefix: &str, keep: usize) {
    let list = backups_sorted(dir, prefix);
    if list.len() > keep {
        for p in &list[..list.len() - keep] {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// 今天还没备份就备份一份；返回新备份的路径
pub fn daily_backup(
    conn: &Connection,
    dir: &Path,
    today: &str,
) -> Result<Option<PathBuf>, DbError> {
    std::fs::create_dir_all(dir)?;
    let name = format!("{DAILY_PREFIX}{}.db", today.replace('-', ""));
    let path = dir.join(&name);
    let made = if path.exists() {
        None
    } else {
        db::vacuum_into(conn, &path)?;
        Some(path)
    };
    prune(dir, DAILY_PREFIX, KEEP_DAILY);
    prune(dir, "pre-migrate-", KEEP_PRE_MIGRATE);
    Ok(made)
}

/// 最近一次每日备份的文件名（设置界面显示）
pub fn latest_backup(dir: &Path) -> Option<String> {
    backups_sorted(dir, DAILY_PREFIX)
        .last()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
}

fn remove_sidecars(db_path: &Path) {
    for ext in ["-wal", "-shm"] {
        let mut p = db_path.as_os_str().to_owned();
        p.push(ext);
        let _ = std::fs::remove_file(PathBuf::from(p));
    }
}

/// 打开数据库；损坏时从备份恢复。返回连接和给用户看的说明（正常时为 None）。
pub fn open_with_recovery(
    db_path: &Path,
    backup_dir: &Path,
) -> Result<(Connection, Option<String>), DbError> {
    if let Ok(conn) = db::open(db_path, backup_dir) {
        if integrity_ok(&conn) {
            return Ok((conn, None));
        }
        drop(conn);
    }
    log::error!("数据库损坏或无法打开，尝试从备份恢复");
    // 坏文件改名保留
    if db_path.exists() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let bad = db_path.with_file_name(format!("deskmate.corrupt-{stamp}.db"));
        std::fs::rename(db_path, &bad)?;
    }
    remove_sidecars(db_path);
    // 从新到旧找第一个能用的备份（每日备份优先，其次是迁移前备份）
    let mut candidates = backups_sorted(backup_dir, DAILY_PREFIX);
    candidates.reverse();
    let mut pre = backups_sorted(backup_dir, "pre-migrate-");
    pre.reverse();
    candidates.extend(pre);
    for b in candidates {
        let ok = Connection::open(&b)
            .map(|c| integrity_ok(&c))
            .unwrap_or(false);
        if !ok {
            continue;
        }
        std::fs::copy(&b, db_path)?;
        if let Ok(conn) = db::open(db_path, backup_dir)
            && integrity_ok(&conn)
        {
            let name = b
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            return Ok((
                conn,
                Some(format!(
                    "数据库文件损坏了，已经从备份 {name} 恢复。最近的部分聊天和记忆可能丢失。"
                )),
            ));
        }
        let _ = std::fs::remove_file(db_path);
        remove_sidecars(db_path);
    }
    let conn = db::open(db_path, backup_dir)?;
    Ok((conn, Some("数据库文件损坏了，而且没有可用的备份，已经新建了空的数据库。损坏的文件保留在数据目录里。".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_backup_once_per_day_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        let conn = db::open(&dir.path().join("a.db"), &backups).unwrap();
        assert!(
            daily_backup(&conn, &backups, "2026-10-01")
                .unwrap()
                .is_some()
        );
        assert!(
            daily_backup(&conn, &backups, "2026-10-01")
                .unwrap()
                .is_none(),
            "同一天只备份一次"
        );
        for d in 2..=10 {
            daily_backup(&conn, &backups, &format!("2026-10-{d:02}")).unwrap();
        }
        let list = backups_sorted(&backups, DAILY_PREFIX);
        assert_eq!(list.len(), KEEP_DAILY);
        assert_eq!(
            latest_backup(&backups).as_deref(),
            Some("deskmate-20261010.db")
        );
        assert!(list[0].ends_with("deskmate-20261004.db"));
    }

    #[test]
    fn corrupted_db_is_restored_from_backup() {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        let path = dir.path().join("deskmate.db");
        {
            let conn = db::open(&path, &backups).unwrap();
            conn.execute(
                "INSERT INTO settings(key, value) VALUES ('k', '\"备份里的值\"')",
                [],
            )
            .unwrap();
            daily_backup(&conn, &backups, "2026-10-27").unwrap();
        }
        // 把数据库文件写坏
        std::fs::write(
            &path,
            b"this is definitely not a sqlite database file, just garbage bytes......",
        )
        .unwrap();
        let (conn, note) = open_with_recovery(&path, &backups).unwrap();
        assert!(note.unwrap().contains("deskmate-20261027.db"));
        let v: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'k'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, "\"备份里的值\"");
        // 坏文件被保留下来
        let kept = std::fs::read_dir(dir.path()).unwrap().flatten().any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("deskmate.corrupt-")
        });
        assert!(kept);
    }

    #[test]
    fn no_backup_creates_fresh_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deskmate.db");
        std::fs::write(
            &path,
            b"garbage garbage garbage garbage garbage garbage garbage garbage!",
        )
        .unwrap();
        let (conn, note) = open_with_recovery(&path, &dir.path().join("backups")).unwrap();
        assert!(note.unwrap().contains("新建"));
        assert!(integrity_ok(&conn));
    }

    #[test]
    fn healthy_db_has_no_note() {
        let dir = tempfile::tempdir().unwrap();
        let (_c, note) =
            open_with_recovery(&dir.path().join("x.db"), &dir.path().join("b")).unwrap();
        assert!(note.is_none());
    }
}
