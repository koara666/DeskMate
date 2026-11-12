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
    // v2：模型用量（token 与估算费用，美元）
    r#"
    CREATE TABLE usage (
        id                  INTEGER PRIMARY KEY,
        ts                  INTEGER NOT NULL,
        day                 TEXT NOT NULL,
        month               TEXT NOT NULL,
        backend_id          TEXT NOT NULL,
        model               TEXT NOT NULL,
        purpose             TEXT NOT NULL,
        input_tokens        INTEGER NOT NULL DEFAULT 0,
        cached_input_tokens INTEGER NOT NULL DEFAULT 0,
        cache_write_tokens  INTEGER NOT NULL DEFAULT 0,
        output_tokens       INTEGER NOT NULL DEFAULT 0,
        cost_usd            REAL NOT NULL DEFAULT 0
    );
    CREATE INDEX usage_day ON usage(day);
    CREATE INDEX usage_month ON usage(month);
    "#,
    // v3：记忆系统——会话与消息（短期记忆）、摘要与事实（长期记忆）、全文索引
    r#"
    CREATE TABLE conversations (
        id               INTEGER PRIMARY KEY,
        character_id     TEXT NOT NULL,
        started_at       INTEGER NOT NULL,
        last_at          INTEGER NOT NULL,
        -- 暂停记忆期间的会话：不提取记忆，结束后删除
        no_memory        INTEGER NOT NULL DEFAULT 0,
        -- 0 待提取，1 已完成，2 多次失败后放弃
        extract_state    INTEGER NOT NULL DEFAULT 0,
        extract_attempts INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX conv_char ON conversations(character_id, last_at);
    CREATE INDEX conv_pending ON conversations(extract_state, last_at);

    CREATE TABLE messages (
        id              INTEGER PRIMARY KEY,
        conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        role            TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
        content         TEXT NOT NULL,
        created_at      INTEGER NOT NULL
    );
    CREATE INDEX msg_conv ON messages(conversation_id, id);

    CREATE TABLE memories (
        id            INTEGER PRIMARY KEY,
        kind          TEXT NOT NULL CHECK (kind IN ('fact', 'summary')),
        content       TEXT NOT NULL,
        scope         TEXT NOT NULL,
        category      TEXT NOT NULL DEFAULT 'other',
        source_kind   TEXT NOT NULL,
        source_ref    TEXT NOT NULL DEFAULT '',
        trusted       INTEGER NOT NULL DEFAULT 1,
        importance    INTEGER NOT NULL DEFAULT 3 CHECK (importance BETWEEN 1 AND 5),
        created_at    INTEGER NOT NULL,
        updated_at    INTEGER NOT NULL,
        last_used_at  INTEGER,
        use_count     INTEGER NOT NULL DEFAULT 0,
        status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'superseded')),
        superseded_by INTEGER REFERENCES memories(id),
        day           TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX mem_lookup ON memories(status, kind, scope);

    -- tokens 列存「单字 + 二字组合」切分后的文本（见 memory/tokenize.rs），rowid = memories.id
    CREATE VIRTUAL TABLE memory_fts USING fts5(tokens);
    "#,
    // v4：任务执行记录（完整日志在 tasks\<id>.jsonl）
    r#"
    CREATE TABLE tasks (
        id          TEXT PRIMARY KEY,
        created_at  INTEGER NOT NULL,
        finished_at INTEGER,
        executor    TEXT NOT NULL,
        goal        TEXT NOT NULL,
        workdir     TEXT NOT NULL,
        status      TEXT NOT NULL,
        summary     TEXT NOT NULL DEFAULT '',
        cost_usd    REAL NOT NULL DEFAULT 0
    );
    CREATE INDEX tasks_created ON tasks(created_at);
    "#,
    // v5：记忆维护——记录上次因为久未使用而降低重要度的时间
    r#"
    ALTER TABLE memories ADD COLUMN decayed_at INTEGER;
    "#,
    // v6：提醒（v2 P1）。at：定时提醒的下次时间 / 开机提醒从哪天起（Unix 秒）；repeat 是 JSON
    r#"
    CREATE TABLE reminders (
        id            INTEGER PRIMARY KEY,
        content       TEXT NOT NULL,
        trigger       TEXT NOT NULL CHECK (trigger IN ('at', 'startup', 'activity')),
        at            INTEGER,
        repeat        TEXT NOT NULL DEFAULT '{"type":"none"}',
        activity      TEXT NOT NULL DEFAULT '',
        line          TEXT NOT NULL DEFAULT '',
        important     INTEGER NOT NULL DEFAULT 0,
        status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'done')),
        created_at    INTEGER NOT NULL,
        last_fired_at INTEGER
    );
    CREATE INDEX reminders_due ON reminders(status, trigger, at);
    "#,
    // v7：活动日志（v2 P3）——只存汇总：日期、分类、清洗过的程序名、开始时间、时长；不存窗口标题
    r#"
    CREATE TABLE activity_log (
        id       INTEGER PRIMARY KEY,
        day      TEXT NOT NULL,
        category TEXT NOT NULL,
        label    TEXT NOT NULL DEFAULT '',
        start    INTEGER NOT NULL,
        secs     INTEGER NOT NULL
    );
    CREATE INDEX activity_day ON activity_log(day);
    "#,
    // v8：桌宠日记（v2 P4）——每个角色每天一篇；同时作为记忆（摘要类、分类 diary）参与检索
    r#"
    CREATE TABLE diaries (
        id           INTEGER PRIMARY KEY,
        day          TEXT NOT NULL,
        character_id TEXT NOT NULL,
        content      TEXT NOT NULL,
        created_at   INTEGER NOT NULL,
        UNIQUE (day, character_id)
    );
    "#,
    // v9：小游戏战绩（v2 P5）。outcome 是用户的结果：win / lose / draw / done（没有胜负）
    r#"
    CREATE TABLE game_records (
        id           INTEGER PRIMARY KEY,
        game         TEXT NOT NULL,
        character_id TEXT NOT NULL,
        outcome      TEXT NOT NULL CHECK (outcome IN ('win', 'lose', 'draw', 'done')),
        ts           INTEGER NOT NULL
    );
    CREATE INDEX game_records_char ON game_records(character_id, game);
    CREATE INDEX game_records_ts ON game_records(ts);
    "#,
    // v10：她说过的话的日语版本（中文原文 → 日语）。以前只存在内存里（重启就没了）：
    // 昨天设的提醒今天到点时查不到日语，只能再翻译一次。只存她自己的台词；记忆暂停时不存；清除记忆时一起清掉
    r#"
    CREATE TABLE voice_ja (
        zh TEXT PRIMARY KEY,
        ja TEXT NOT NULL,
        ts INTEGER NOT NULL
    );
    CREATE INDEX voice_ja_ts ON voice_ja(ts);
    "#,
    // v11：计划（见 plans/mod.rs）。日期一律是 YYYY-MM-DD 的本地「计划日」（凌晨 5 点前算前一天）；
    // week_minutes：周一到周日每天能用多少分钟（JSON 数组，0 = 那天不排）；off_days：请假的日子（JSON 数组）
    r#"
    CREATE TABLE plans (
        id           INTEGER PRIMARY KEY,
        title        TEXT NOT NULL,
        goal         TEXT NOT NULL DEFAULT '',
        emoji        TEXT NOT NULL DEFAULT '',
        mode         TEXT NOT NULL DEFAULT 'content' CHECK (mode IN ('content', 'habit')),
        start_day    TEXT NOT NULL,
        end_day      TEXT NOT NULL DEFAULT '',
        week_minutes TEXT NOT NULL DEFAULT '[60,60,60,60,60,60,60]',
        off_days     TEXT NOT NULL DEFAULT '[]',
        pace         TEXT NOT NULL DEFAULT 'even' CHECK (pace IN ('even', 'asap')),
        remind_at    TEXT NOT NULL DEFAULT '',
        supervise    INTEGER NOT NULL DEFAULT 1,
        strategy     TEXT NOT NULL DEFAULT '',
        status       TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'done', 'archived')),
        sort         INTEGER NOT NULL DEFAULT 0,
        arranged_day TEXT NOT NULL DEFAULT '',
        created_at   INTEGER NOT NULL,
        updated_at   INTEGER NOT NULL
    );
    CREATE TABLE plan_items (
        id      INTEGER PRIMARY KEY,
        plan_id INTEGER NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
        title   TEXT NOT NULL,
        detail  TEXT NOT NULL DEFAULT '',
        minutes INTEGER NOT NULL DEFAULT 0,
        status  TEXT NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'done', 'skipped')),
        sort    INTEGER NOT NULL DEFAULT 0,
        done_at INTEGER
    );
    CREATE INDEX plan_items_plan ON plan_items(plan_id, sort);
    CREATE TABLE plan_tasks (
        id         INTEGER PRIMARY KEY,
        plan_id    INTEGER REFERENCES plans(id) ON DELETE CASCADE,
        item_id    INTEGER REFERENCES plan_items(id) ON DELETE SET NULL,
        day        TEXT NOT NULL,
        orig_day   TEXT NOT NULL DEFAULT '',
        title      TEXT NOT NULL,
        minutes    INTEGER NOT NULL DEFAULT 0,
        start      TEXT NOT NULL DEFAULT '',
        status     TEXT NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'done', 'skipped')),
        spent      INTEGER NOT NULL DEFAULT 0,
        finishes   INTEGER NOT NULL DEFAULT 0,
        note       TEXT NOT NULL DEFAULT '',
        source     TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('auto', 'user')),
        sort       INTEGER NOT NULL DEFAULT 0,
        reminded   INTEGER NOT NULL DEFAULT 0,
        done_at    INTEGER,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX plan_tasks_day ON plan_tasks(day);
    CREATE INDEX plan_tasks_plan ON plan_tasks(plan_id, day);
    CREATE TABLE plan_notes (
        id         INTEGER PRIMARY KEY,
        plan_id    INTEGER REFERENCES plans(id) ON DELETE CASCADE,
        day        TEXT NOT NULL,
        content    TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX plan_notes_plan ON plan_notes(plan_id, day);
    CREATE INDEX plan_notes_day ON plan_notes(day);
    "#,
    // v12：计划书（复杂的长期计划，见 plans/goal.rs）。一个计划书（goals）下面有几条并行的「线」——每条线就是一个 plans，
    // 原来的单个计划 goal_id 为空，照旧用。阶段（goal_phases）：日期范围、目标、完成标准（JSON 数组）、这个阶段每天的时间
    // （JSON [7]，空 = 用计划书的）、各条线的份额（JSON {线的 id: 份额}）。节点（goal_milestones）：日期 + 标准，当天问达标没有。
    // 缓冲期、学习资源、自查清单、先问的问题和回答是 JSON 数组。内容条目加了所属阶段、最早哪天开始、截止、钉在哪天、是不是粗的（还没细化）
    r#"
    CREATE TABLE goals (
        id             INTEGER PRIMARY KEY,
        title          TEXT NOT NULL,
        emoji          TEXT NOT NULL DEFAULT '',
        summary        TEXT NOT NULL DEFAULT '',
        source         TEXT NOT NULL DEFAULT '',
        doc            TEXT NOT NULL DEFAULT '',
        strategy       TEXT NOT NULL DEFAULT '',
        start_day      TEXT NOT NULL,
        end_day        TEXT NOT NULL DEFAULT '',
        week_minutes   TEXT NOT NULL DEFAULT '[60,60,60,60,60,90,90]',
        off_days       TEXT NOT NULL DEFAULT '[]',
        buffers        TEXT NOT NULL DEFAULT '[]',
        resources      TEXT NOT NULL DEFAULT '[]',
        checklist      TEXT NOT NULL DEFAULT '[]',
        answers        TEXT NOT NULL DEFAULT '[]',
        remind_at      TEXT NOT NULL DEFAULT '',
        review_day     INTEGER NOT NULL DEFAULT 6,
        status         TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'done', 'archived')),
        sort           INTEGER NOT NULL DEFAULT 0,
        arranged_day   TEXT NOT NULL DEFAULT '',
        expanded_until TEXT NOT NULL DEFAULT '',
        created_at     INTEGER NOT NULL,
        updated_at     INTEGER NOT NULL
    );
    CREATE TABLE goal_phases (
        id           INTEGER PRIMARY KEY,
        goal_id      INTEGER NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
        title        TEXT NOT NULL,
        start_day    TEXT NOT NULL,
        end_day      TEXT NOT NULL,
        objective    TEXT NOT NULL DEFAULT '',
        criteria     TEXT NOT NULL DEFAULT '[]',
        week_minutes TEXT NOT NULL DEFAULT '',
        shares       TEXT NOT NULL DEFAULT '{}',
        sort         INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX goal_phases_goal ON goal_phases(goal_id, sort);
    CREATE TABLE goal_milestones (
        id         INTEGER PRIMARY KEY,
        goal_id    INTEGER NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
        phase_id   INTEGER REFERENCES goal_phases(id) ON DELETE SET NULL,
        plan_id    INTEGER REFERENCES plans(id) ON DELETE SET NULL,
        title      TEXT NOT NULL,
        day        TEXT NOT NULL,
        criteria   TEXT NOT NULL DEFAULT '',
        status     TEXT NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'passed', 'missed', 'skipped')),
        result     TEXT NOT NULL DEFAULT '',
        checked_at INTEGER,
        sort       INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX goal_milestones_goal ON goal_milestones(goal_id, day);
    ALTER TABLE plans ADD COLUMN goal_id INTEGER REFERENCES goals(id) ON DELETE CASCADE;
    ALTER TABLE plans ADD COLUMN share INTEGER NOT NULL DEFAULT 1;
    CREATE INDEX plans_goal ON plans(goal_id);
    ALTER TABLE plan_items ADD COLUMN phase_id INTEGER REFERENCES goal_phases(id) ON DELETE SET NULL;
    ALTER TABLE plan_items ADD COLUMN after_day TEXT NOT NULL DEFAULT '';
    ALTER TABLE plan_items ADD COLUMN due_day TEXT NOT NULL DEFAULT '';
    ALTER TABLE plan_items ADD COLUMN pin_day TEXT NOT NULL DEFAULT '';
    ALTER TABLE plan_items ADD COLUMN rough INTEGER NOT NULL DEFAULT 0;
    "#,
    // v13：日历（见 plans/agenda.rs）。提前知道的事（作业截止、某节课停课……）记在某一天上；
    // reminded = 已经说过的那一档（7 / 3 / 1 / 0 天前，空 = 还没说过），done = 搞定了不再提醒
    r#"
    CREATE TABLE agenda (
        id         INTEGER PRIMARY KEY,
        day        TEXT NOT NULL,
        title      TEXT NOT NULL,
        note       TEXT NOT NULL DEFAULT '',
        done       INTEGER NOT NULL DEFAULT 0,
        done_at    INTEGER,
        reminded   INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX agenda_day ON agenda(day);
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
