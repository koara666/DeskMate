//! 提醒与日程（v2 P1，DECISIONS #38）。
//!
//! - 用户在聊天里吩咐（「半小时后提醒我收衣服」）：先用本地规则解析（`parse`），解析不了再调用一次模型（`model`）；
//!   结果用一句话复述给用户，用户在气泡里点「好」才保存。都失败时让用户换个说法或手动填。
//! - 到时说的话在创建时就定好（台词池模板，不再调用模型）。
//! - 本地定时器：按下一个到期时间睡眠（最多 60 秒醒一次，防止系统睡眠后时钟跳变），提醒变了立刻醒来重新算。
//! - 关机期间错过的：下次启动时补提醒并说明是错过的。
//! - 说话统一经过主动行为调度器（种类 Reminder：勿扰挡不住，全屏时改发系统通知）。

pub mod chat;
pub mod model;
pub mod parse;

use crate::memory;
use crate::proactive::{self, Item, Kind};
use crate::state::AppState;
use chrono::{Local, NaiveDateTime, TimeZone};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

/// 触发方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// 某个时间点（可重复）
    At,
    /// 开机（DeskMate 启动）时；`at` 有值时从那天起
    Startup,
    /// 进入某种活动时（比如开始玩游戏，P3 接上活动感知）
    Activity,
}

/// 重复规则（星期一 = 1 … 星期日 = 7）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Repeat {
    #[default]
    None,
    Daily,
    Weekdays,
    Weekly {
        days: Vec<u8>,
    },
    Monthly {
        day: u8,
    },
}

/// 活动触发能用的活动（和活动感知的分类一致）
pub const ACTIVITIES: [(&str, &str); 6] = [
    ("game", "玩游戏"),
    ("coding", "写代码"),
    ("video", "看视频"),
    ("writing", "写作业 / 写东西"),
    ("music", "听音乐"),
    ("meeting", "开会"),
];

pub fn activity_name(id: &str) -> &'static str {
    ACTIVITIES
        .iter()
        .find(|(a, _)| *a == id)
        .map(|(_, n)| *n)
        .unwrap_or("做那件事")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    /// 0 = 还没保存
    #[serde(default)]
    pub id: i64,
    pub content: String,
    pub trigger: Trigger,
    /// At：下次提醒的时间（Unix 秒）；Startup：从哪天起（那天 0 点），None = 下次开机
    #[serde(default)]
    pub at: Option<i64>,
    #[serde(default)]
    pub repeat: Repeat,
    #[serde(default)]
    pub activity: String,
    /// 到时说的话（创建时就定好）
    #[serde(default)]
    pub line: String,
    /// 重要：到时同时发系统通知
    #[serde(default)]
    pub important: bool,
    /// active / paused / done
    #[serde(default = "active")]
    pub status: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_fired_at: Option<i64>,
}

fn active() -> String {
    "active".into()
}

/// 给界面的提醒（多一句「什么时候」）
#[derive(Debug, Clone, Serialize)]
pub struct ReminderView {
    #[serde(flatten)]
    pub reminder: Reminder,
    pub when: String,
}

pub fn to_local(ts: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&Local).naive_local())
        .unwrap_or_default()
}

pub fn from_local(dt: NaiveDateTime) -> i64 {
    Local
        .from_local_datetime(&dt)
        .earliest()
        .map(|d| d.timestamp())
        .unwrap_or_else(|| dt.and_utc().timestamp())
}

pub fn now_local() -> NaiveDateTime {
    Local::now().naive_local()
}

impl Reminder {
    pub fn view(self, now: NaiveDateTime) -> ReminderView {
        let when = parse::describe(
            self.trigger,
            self.at.map(to_local),
            &self.repeat,
            &self.activity,
            false,
            now,
        );
        ReminderView {
            reminder: self,
            when,
        }
    }

    /// 检查并收敛用户（或界面）传来的提醒
    pub fn sanitize(mut self, now: i64) -> Result<Self, String> {
        self.content = self.content.trim().chars().take(200).collect();
        if self.content.is_empty() {
            return Err("要提醒什么还没写呢".into());
        }
        self.line = self.line.trim().chars().take(300).collect();
        if !matches!(self.status.as_str(), "active" | "paused" | "done") {
            self.status = active();
        }
        match &mut self.repeat {
            Repeat::Weekly { days } => {
                days.retain(|d| (1..=7).contains(d));
                days.sort_unstable();
                days.dedup();
                if days.is_empty() {
                    return Err("每周重复至少要选一天".into());
                }
            }
            Repeat::Monthly { day } if !(1..=31).contains(day) => {
                return Err("每月几号要在 1~31 之间".into());
            }
            _ => {}
        }
        match self.trigger {
            Trigger::At => {
                let at = self.at.ok_or("还没选什么时候提醒")?;
                if self.status == "active" && at <= now {
                    if self.repeat == Repeat::None {
                        return Err("这个时间已经过去了".into());
                    }
                    // 重复的提醒：按那个钟点算出下一次
                    let t = to_local(at).time();
                    self.at = Some(from_local(parse::next_occurrence(
                        &self.repeat,
                        t,
                        to_local(now),
                    )));
                }
            }
            Trigger::Startup => {
                if !matches!(self.repeat, Repeat::None | Repeat::Daily) {
                    self.repeat = Repeat::None;
                }
            }
            Trigger::Activity => {
                if !ACTIVITIES.iter().any(|(a, _)| *a == self.activity) {
                    return Err("不认识这种活动".into());
                }
                self.repeat = Repeat::None;
                self.at = None;
            }
        }
        if self.trigger != Trigger::Activity {
            self.activity.clear();
        }
        Ok(self)
    }
}

// ---------------- 数据库 ----------------

const COLS: &str = "id, content, trigger, at, repeat, activity, line, important, status, created_at, last_fired_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Reminder> {
    let trigger: String = r.get(2)?;
    let repeat: String = r.get(4)?;
    Ok(Reminder {
        id: r.get(0)?,
        content: r.get(1)?,
        trigger: serde_json::from_value(serde_json::Value::String(trigger)).unwrap_or(Trigger::At),
        at: r.get(3)?,
        repeat: serde_json::from_str(&repeat).unwrap_or_default(),
        activity: r.get(5)?,
        line: r.get(6)?,
        important: r.get::<_, i64>(7)? != 0,
        status: r.get(8)?,
        created_at: r.get(9)?,
        last_fired_at: r.get(10)?,
    })
}

fn trigger_str(t: Trigger) -> &'static str {
    match t {
        Trigger::At => "at",
        Trigger::Startup => "startup",
        Trigger::Activity => "activity",
    }
}

/// 新建或更新，返回 id
pub fn save(db: &Connection, r: &Reminder, now: i64) -> rusqlite::Result<i64> {
    let repeat = serde_json::to_string(&r.repeat).unwrap_or_else(|_| "{\"type\":\"none\"}".into());
    if r.id > 0 {
        db.execute(
            "UPDATE reminders SET content=?2, trigger=?3, at=?4, repeat=?5, activity=?6, line=?7, important=?8, status=?9 WHERE id=?1",
            params![r.id, r.content, trigger_str(r.trigger), r.at, repeat, r.activity, r.line, r.important as i64, r.status],
        )?;
        Ok(r.id)
    } else {
        db.execute(
            "INSERT INTO reminders(content, trigger, at, repeat, activity, line, important, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![r.content, trigger_str(r.trigger), r.at, repeat, r.activity, r.line, r.important as i64, r.status, now],
        )?;
        Ok(db.last_insert_rowid())
    }
}

pub fn get(db: &Connection, id: i64) -> rusqlite::Result<Option<Reminder>> {
    db.query_row(
        &format!("SELECT {COLS} FROM reminders WHERE id = ?1"),
        [id],
        row,
    )
    .optional()
}

/// 全部提醒：进行中的在前（按时间），已完成的在后（最近 50 条）
pub fn list(db: &Connection) -> rusqlite::Result<Vec<Reminder>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {COLS} FROM reminders WHERE status != 'done'
         ORDER BY status, at IS NULL, at, id"
    ))?;
    let mut v: Vec<Reminder> = stmt.query_map([], row)?.flatten().collect();
    let mut stmt = db.prepare(&format!(
        "SELECT {COLS} FROM reminders WHERE status = 'done' ORDER BY last_fired_at DESC, id DESC LIMIT 50"
    ))?;
    v.extend(stmt.query_map([], row)?.flatten());
    Ok(v)
}

pub fn delete(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM reminders WHERE id = ?1", [id])? > 0)
}

/// 测试用：直接改状态（界面上暂停 / 恢复走 `reminder_set_status` 命令，整条保存）
#[cfg(test)]
pub fn set_status(db: &Connection, id: i64, status: &str) -> rusqlite::Result<bool> {
    Ok(db.execute(
        "UPDATE reminders SET status = ?2 WHERE id = ?1",
        params![id, status],
    )? > 0)
}

/// 到期的定时提醒
pub fn due_at(db: &Connection, now: i64) -> rusqlite::Result<Vec<Reminder>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {COLS} FROM reminders WHERE status = 'active' AND trigger = 'at' AND at <= ?1 ORDER BY at"
    ))?;
    Ok(stmt.query_map([now], row)?.flatten().collect())
}

/// 下一个定时提醒的时间
pub fn next_due(db: &Connection) -> Option<i64> {
    db.query_row(
        "SELECT MIN(at) FROM reminders WHERE status = 'active' AND trigger = 'at'",
        [],
        |r| r.get(0),
    )
    .ok()
    .flatten()
}

/// 开机时该说的提醒（`at` 为空，或那天已经到了）
pub fn due_startup(db: &Connection, now: i64) -> rusqlite::Result<Vec<Reminder>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {COLS} FROM reminders WHERE status = 'active' AND trigger = 'startup' AND (at IS NULL OR at <= ?1) ORDER BY id"
    ))?;
    Ok(stmt.query_map([now], row)?.flatten().collect())
}

/// 进入某种活动时该说的提醒
pub fn due_activity(db: &Connection, activity: &str) -> rusqlite::Result<Vec<Reminder>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {COLS} FROM reminders WHERE status = 'active' AND trigger = 'activity' AND activity = ?1 ORDER BY id"
    ))?;
    Ok(stmt.query_map([activity], row)?.flatten().collect())
}

/// 提醒说出去了：重复的算出下一次，不重复的标记完成
pub fn mark_fired(db: &Connection, r: &Reminder, now: i64) -> rusqlite::Result<()> {
    let local_now = to_local(now);
    let (status, at) = match (r.trigger, &r.repeat) {
        (Trigger::At, Repeat::None) | (Trigger::Activity, _) => ("done", r.at),
        (Trigger::At, rep) => {
            let t = r.at.map(to_local).map(|x| x.time()).unwrap_or_default();
            (
                "active",
                Some(from_local(parse::next_occurrence(rep, t, local_now))),
            )
        }
        (Trigger::Startup, Repeat::Daily) => {
            let tomorrow = local_now.date() + chrono::Duration::days(1);
            (
                "active",
                Some(from_local(tomorrow.and_time(chrono::NaiveTime::MIN))),
            )
        }
        (Trigger::Startup, _) => ("done", r.at),
    };
    db.execute(
        "UPDATE reminders SET status = ?2, at = ?3, last_fired_at = ?4 WHERE id = ?1",
        params![r.id, status, at, now],
    )?;
    Ok(())
}

/// 今天还会响的定时提醒（每日播报用）
pub fn today_list(db: &Connection, now: i64) -> Vec<Reminder> {
    let end = from_local(
        (to_local(now).date() + chrono::Duration::days(1)).and_time(chrono::NaiveTime::MIN),
    );
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {COLS} FROM reminders WHERE status = 'active' AND trigger = 'at' AND at >= ?1 AND at < ?2 ORDER BY at"
    )) else {
        return Vec::new();
    };
    stmt.query_map([now, end], row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

// ---------------- 到时说的话 ----------------

/// 创建时就定好的台词（台词池，不调用模型）
pub fn make_line(st: &AppState, content: &str) -> String {
    st.line("reminder.fire", &[("内容", content.to_string())])
        .unwrap_or_else(|| format!("时间到啦：{content}。"))
}

/// 要说出去的话：错过的加一句说明
fn fire_text(st: &AppState, r: &Reminder, late_secs: i64, at_startup: bool) -> String {
    let base = if r.line.is_empty() {
        make_line(st, &r.content)
    } else {
        r.line.clone()
    };
    if r.trigger == Trigger::At && (at_startup || late_secs > 10 * 60) {
        let when =
            r.at.map(|a| {
                use chrono::{Datelike, Timelike};
                let t = to_local(a);
                format!(
                    "{}月{}日 {}:{:02}",
                    t.month(),
                    t.day(),
                    t.hour(),
                    t.minute()
                )
            })
            .unwrap_or_default();
        let note = st
            .line(
                "reminder.missed",
                &[("内容", r.content.clone()), ("时间", when.clone())],
            )
            .unwrap_or_else(|| format!("本来 {when} 要提醒你的，那时候电脑没开着，错过了："));
        return format!("{note}{base}");
    }
    base
}

fn submit(app: &AppHandle, r: &Reminder, text: String) {
    let now = memory::now_secs();
    let mut item = Item::new(Kind::Reminder, text.clone(), now + 12 * 3600);
    item.key = format!("reminder:{}", r.id);
    proactive::submit(app, item);
    if r.important {
        crate::notify::send(app, &text);
    }
}

// ---------------- 定时器 ----------------

#[derive(Default)]
pub struct ReminderState {
    /// 提醒变了（新建、编辑、删除）：叫醒定时器重新算下一个到期时间
    pub changed: Notify,
}

pub fn notify_changed(app: &AppHandle) {
    app.state::<AppState>().reminders.changed.notify_one();
}

/// 处理到期的定时提醒
pub fn fire_due(app: &AppHandle, at_startup: bool) {
    let st = app.state::<AppState>();
    let now = memory::now_secs();
    let due = due_at(&st.db(), now).unwrap_or_default();
    for r in due {
        let late = now - r.at.unwrap_or(now);
        let text = fire_text(&st, &r, late, at_startup);
        submit(app, &r, text);
        if let Err(e) = mark_fired(&st.db(), &r, now) {
            log::warn!("更新提醒状态失败：{e}");
        }
        log::info!(
            "提醒 #{} 到了{}",
            r.id,
            if at_startup { "（补提醒）" } else { "" }
        );
    }
}

/// 启动时：补上关机期间错过的，再说开机提醒
fn on_startup(app: &AppHandle) {
    fire_due(app, true);
    fire_startup(app);
}

/// 开机提醒（启动时；或者在开着的电脑上到了那一天，用户回来时）
fn fire_startup(app: &AppHandle) {
    let st = app.state::<AppState>();
    let now = memory::now_secs();
    for r in due_startup(&st.db(), now).unwrap_or_default() {
        let text = if r.line.is_empty() {
            make_line(&st, &r.content)
        } else {
            r.line.clone()
        };
        submit(app, &r, text);
        let _ = mark_fired(&st.db(), &r, now);
    }
}

/// 进入某种活动时（活动感知调用）
pub fn on_activity(app: &AppHandle, activity: &str) {
    let st = app.state::<AppState>();
    let now = memory::now_secs();
    for r in due_activity(&st.db(), activity).unwrap_or_default() {
        let text = if r.line.is_empty() {
            make_line(&st, &r.content)
        } else {
            r.line.clone()
        };
        submit(app, &r, text);
        let _ = mark_fired(&st.db(), &r, now);
    }
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 桌宠出现后再补提醒
        tokio::time::sleep(Duration::from_secs(4)).await;
        on_startup(&app);
        let mut last_day = to_local(memory::now_secs()).date();
        loop {
            let wait = {
                let st = app.state::<AppState>();
                let now = memory::now_secs();
                next_due(&st.db())
                    .map(|t| (t - now).clamp(1, 60))
                    .unwrap_or(60)
            };
            let st = app.state::<AppState>();
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(wait as u64)) => {}
                _ = st.reminders.changed.notified() => {}
            }
            fire_due(&app, false);
            // 电脑一直开着跨过了午夜：那天的「开机提醒」在用户回到电脑前时说
            let today = to_local(memory::now_secs()).date();
            if today != last_day && crate::daily::system_idle_secs() < 120 {
                last_day = today;
                fire_startup(&app);
            }
        }
    });
}

// ---------------- 聊天里吩咐 ----------------

/// 聊天里解析出来、等用户确认的提醒
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Draft {
    pub reminder: Reminder,
    pub when: String,
}

/// 把解析结果变成待确认的提醒
pub fn draft_from(st: &AppState, p: &parse::Parsed, now: NaiveDateTime) -> Draft {
    let when = parse::describe(p.trigger, p.at, &p.repeat, &p.activity, p.relative, now);
    Draft {
        reminder: Reminder {
            id: 0,
            content: p.content.clone(),
            trigger: p.trigger,
            at: p.at.map(from_local),
            repeat: p.repeat.clone(),
            activity: p.activity.clone(),
            line: make_line(st, &p.content),
            important: false,
            status: active(),
            created_at: 0,
            last_fired_at: None,
        },
        when,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn r(trigger: Trigger, at: Option<i64>, repeat: Repeat) -> Reminder {
        Reminder {
            id: 0,
            content: "收衣服".into(),
            trigger,
            at,
            repeat,
            activity: String::new(),
            line: "收衣服啦".into(),
            important: false,
            status: "active".into(),
            created_at: 0,
            last_fired_at: None,
        }
    }

    #[test]
    fn save_list_and_fire_once() {
        let db = db::open_in_memory().unwrap();
        let now = 1_800_000_000;
        let id = save(&db, &r(Trigger::At, Some(now + 60), Repeat::None), now).unwrap();
        assert_eq!(next_due(&db), Some(now + 60));
        assert!(due_at(&db, now).unwrap().is_empty());
        let due = due_at(&db, now + 61).unwrap();
        assert_eq!(due.len(), 1);
        mark_fired(&db, &due[0], now + 61).unwrap();
        let x = get(&db, id).unwrap().unwrap();
        assert_eq!(x.status, "done");
        assert_eq!(x.last_fired_at, Some(now + 61));
        assert!(due_at(&db, now + 3600).unwrap().is_empty(), "只响一次");
        assert_eq!(next_due(&db), None);
        assert_eq!(list(&db).unwrap().len(), 1, "完成的也在列表里");
    }

    #[test]
    fn repeating_reminder_moves_to_next_occurrence() {
        let db = db::open_in_memory().unwrap();
        let first = from_local(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 30)
                .unwrap()
                .and_hms_opt(20, 0, 0)
                .unwrap(),
        );
        let id = save(
            &db,
            &r(Trigger::At, Some(first), Repeat::Weekly { days: vec![5] }),
            first - 100,
        )
        .unwrap();
        let due = due_at(&db, first + 5).unwrap();
        mark_fired(&db, &due[0], first + 5).unwrap();
        let x = get(&db, id).unwrap().unwrap();
        assert_eq!(x.status, "active");
        assert_eq!(x.at, Some(first + 7 * 86400), "下周五同一时间");
    }

    #[test]
    fn startup_and_activity_triggers() {
        let db = db::open_in_memory().unwrap();
        let now = 1_800_000_000;
        save(&db, &r(Trigger::Startup, None, Repeat::None), now).unwrap();
        save(
            &db,
            &r(Trigger::Startup, Some(now + 86400), Repeat::None),
            now,
        )
        .unwrap();
        let mut a = r(Trigger::Activity, None, Repeat::None);
        a.activity = "game".into();
        let aid = save(&db, &a, now).unwrap();
        assert_eq!(due_startup(&db, now).unwrap().len(), 1, "明天的还没到");
        assert_eq!(due_startup(&db, now + 86400).unwrap().len(), 2);
        assert!(
            due_at(&db, now + 999_999).unwrap().is_empty(),
            "开机提醒不走定时器"
        );
        assert_eq!(due_activity(&db, "game").unwrap().len(), 1);
        assert!(due_activity(&db, "video").unwrap().is_empty());
        let g = get(&db, aid).unwrap().unwrap();
        mark_fired(&db, &g, now).unwrap();
        assert!(
            due_activity(&db, "game").unwrap().is_empty(),
            "活动提醒只响一次"
        );
    }

    #[test]
    fn paused_reminders_do_not_fire() {
        let db = db::open_in_memory().unwrap();
        let now = 1_800_000_000;
        let id = save(&db, &r(Trigger::At, Some(now + 1), Repeat::None), now).unwrap();
        set_status(&db, id, "paused").unwrap();
        assert!(due_at(&db, now + 10).unwrap().is_empty());
        assert_eq!(next_due(&db), None);
        set_status(&db, id, "active").unwrap();
        assert_eq!(due_at(&db, now + 10).unwrap().len(), 1);
        assert!(delete(&db, id).unwrap());
        assert!(get(&db, id).unwrap().is_none());
    }

    #[test]
    fn sanitize_checks_input() {
        let now = 1_800_000_000;
        let mut x = r(Trigger::At, Some(now - 10), Repeat::None);
        assert!(x.clone().sanitize(now).is_err(), "过去的时间");
        x.repeat = Repeat::Daily;
        let y = x.clone().sanitize(now).unwrap();
        assert!(y.at.unwrap() > now, "重复的提醒自动算到下一次");
        x.content = "  ".into();
        assert!(x.clone().sanitize(now).is_err());
        let mut w = r(
            Trigger::At,
            Some(now + 10),
            Repeat::Weekly { days: vec![9, 0] },
        );
        assert!(w.clone().sanitize(now).is_err());
        w.repeat = Repeat::Weekly {
            days: vec![5, 1, 5],
        };
        assert_eq!(
            w.sanitize(now).unwrap().repeat,
            Repeat::Weekly { days: vec![1, 5] }
        );
        let mut a = r(Trigger::Activity, Some(5), Repeat::Daily);
        a.activity = "hacking".into();
        assert!(a.clone().sanitize(now).is_err());
        a.activity = "game".into();
        let a = a.sanitize(now).unwrap();
        assert_eq!((a.at, a.repeat), (None, Repeat::None));
        let mut s = r(Trigger::Startup, None, Repeat::Weekdays);
        s.status = "weird".into();
        let s = s.sanitize(now).unwrap();
        assert_eq!((s.repeat, s.status.as_str()), (Repeat::None, "active"));
    }

    #[test]
    fn today_list_only_today() {
        let db = db::open_in_memory().unwrap();
        let noon = from_local(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 28)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        );
        save(&db, &r(Trigger::At, Some(noon + 3600), Repeat::None), noon).unwrap();
        save(&db, &r(Trigger::At, Some(noon + 86400), Repeat::None), noon).unwrap();
        assert_eq!(today_list(&db, noon).len(), 1);
    }
}
