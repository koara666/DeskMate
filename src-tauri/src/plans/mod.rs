//! 计划：多个长期计划 + 每天的安排 + 桌宠陪着监督（设计见本机 docs/PLANS.md）。
//!
//! - AI 只负责「理解」：新建 / 重新安排时调用一次模型，把用户的话拆成内容条目和估时（`ai.rs`，没有模型时 `parse.rs` 本地兜底）。
//! - 每天做什么由本地排程器算（`schedule.rs`，纯函数）：每周每天能用的分钟、截止、请假、节奏；
//!   今天的安排在当天第一次用到时落库（`ensure_day`），以后的日子只是推算出来的预览。
//! - 没做完的内容自动回到池子里，后面的日子顺延 / 加量；单独的事情（不属于任何计划）没做完自动挪到今天。
//! - 「计划日」和每日播报一样：凌晨 5 点前算前一天（熬夜学习的那一段还算今天的）。

pub mod agenda;
pub mod ai;
pub mod cmds;
pub mod diary_note;
pub mod goal;
pub mod goal_ai;
pub mod goal_cmds;
pub mod lanes;
pub mod parse;
pub mod schedule;
pub mod stats;
pub mod supervise;

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use schedule::{Left, Shape, Slot, Used, Work};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const DAY_FMT: &str = "%Y-%m-%d";
/// 最多同时有多少个计划（不含已归档）
pub const MAX_PLANS: usize = 50;
/// 一个计划最多多少条内容
pub const MAX_ITEMS: usize = 300;
/// 条目已经超时但还没标完成：至少再留这么多分钟
pub const MIN_LEFT: u32 = 15;
/// 内容条目没写多久时，按这个估
pub const DEFAULT_ITEM_MINUTES: u32 = 30;

pub fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), DAY_FMT).ok()
}

pub fn day_str(d: NaiveDate) -> String {
    d.format(DAY_FMT).to_string()
}

/// 现在是哪个「计划日」
pub fn today() -> NaiveDate {
    crate::briefing::briefing_day(crate::reminders::now_local())
}

fn valid_hm(s: &str) -> bool {
    s.is_empty() || crate::proactive::parse_hm(s).is_some()
}

fn clip(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// 有总量的内容（学完一本书、做完一个项目）
    #[default]
    Content,
    /// 每天都做的事（背单词、健身、练琴）
    Habit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    /// 有截止时均匀排到截止
    #[default]
    Even,
    /// 能做多少做多少，尽快做完
    Asap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    pub id: i64,
    pub title: String,
    /// 用户的原话（新建时写的那段）
    pub goal: String,
    pub emoji: String,
    pub mode: Mode,
    pub start_day: String,
    /// 截止（空 = 没有）
    pub end_day: String,
    /// 周一到周日每天能用多少分钟（0 = 那天不排）
    pub week_minutes: [u32; 7],
    /// 请假的日子
    pub off_days: Vec<String>,
    pub pace: Pace,
    /// 几点开始（"HH:MM"，空 = 不固定）
    pub remind_at: String,
    /// 让她提醒、监督这个计划
    pub supervise: bool,
    /// 她的安排思路（AI 写的，用户可以改）
    pub strategy: String,
    /// active / paused / done / archived
    pub status: String,
    pub sort: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// 上次排今天的安排是哪天（内部用）
    #[serde(skip)]
    pub arranged_day: String,
    /// 属于哪个计划书（是计划书里的一条线；None = 单独的计划）
    pub goal_id: Option<i64>,
    /// 在计划书里默认占多少份（阶段没写份额时用；0 = 不分时间）
    pub share: u32,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            id: 0,
            title: String::new(),
            goal: String::new(),
            emoji: String::new(),
            mode: Mode::Content,
            start_day: String::new(),
            end_day: String::new(),
            week_minutes: [60; 7],
            off_days: Vec::new(),
            pace: Pace::Even,
            remind_at: String::new(),
            supervise: true,
            strategy: String::new(),
            status: "active".into(),
            sort: 0,
            created_at: 0,
            updated_at: 0,
            arranged_day: String::new(),
            goal_id: None,
            share: 1,
        }
    }
}

impl Plan {
    /// 检查并收敛用户（或模型）给的计划
    pub fn sanitize(mut self, today: NaiveDate) -> Result<Self, String> {
        self.title = clip(&self.title, 60);
        if self.title.is_empty() {
            return Err("计划还没有名字".into());
        }
        self.goal = clip(&self.goal, 4000);
        self.emoji = self.emoji.trim().chars().take(4).collect();
        self.strategy = clip(&self.strategy, 3000);
        let start = parse_day(&self.start_day).unwrap_or(today);
        self.start_day = day_str(start);
        match parse_day(&self.end_day) {
            Some(e) if e < start => return Err("截止日期比开始日期还早".into()),
            Some(e) => self.end_day = day_str(e),
            None => self.end_day.clear(),
        }
        for m in &mut self.week_minutes {
            *m = ((*m).min(720) / 5) * 5;
        }
        if self.week_minutes.iter().all(|m| *m == 0) {
            return Err("每周至少要有一天能做".into());
        }
        let mut off: Vec<NaiveDate> = self.off_days.iter().filter_map(|d| parse_day(d)).collect();
        off.sort_unstable();
        off.dedup();
        off.truncate(400);
        self.off_days = off.into_iter().map(day_str).collect();
        self.remind_at = self.remind_at.trim().to_string();
        if !valid_hm(&self.remind_at) {
            self.remind_at.clear();
        }
        if !matches!(
            self.status.as_str(),
            "active" | "paused" | "done" | "archived"
        ) {
            self.status = "active".into();
        }
        self.share = self.share.min(100);
        Ok(self)
    }

    pub fn shape(&self) -> Shape {
        Shape {
            mode: match self.mode {
                Mode::Content => schedule::Mode::Content,
                Mode::Habit => schedule::Mode::Habit,
            },
            start: parse_day(&self.start_day).unwrap_or_else(today),
            end: parse_day(&self.end_day),
            week: self.week_minutes,
            off: self.off_days.iter().filter_map(|d| parse_day(d)).collect(),
            pace: match self.pace {
                Pace::Even => schedule::Pace::Even,
                Pace::Asap => schedule::Pace::Asap,
            },
        }
    }

    pub fn label(&self) -> String {
        if self.emoji.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.emoji, self.title)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: i64,
    pub plan_id: i64,
    pub title: String,
    pub detail: String,
    /// 预计要多久（分钟）；习惯类是每次多久（0 = 用当天的量）
    pub minutes: u32,
    /// todo / done / skipped
    pub status: String,
    pub sort: i64,
    pub done_at: Option<i64>,
    /// 属于计划书的哪个阶段
    pub phase_id: Option<i64>,
    /// 最早哪天开始（空 = 不限）
    pub after_day: String,
    /// 这条自己的截止（空 = 没有；截止早的先排）
    pub due_day: String,
    /// 钉在哪天做（空 = 不钉；钉住的不顺延）
    pub pin_day: String,
    /// 还是路线图里的一大块，没细化成具体的小块
    pub rough: bool,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            id: 0,
            plan_id: 0,
            title: String::new(),
            detail: String::new(),
            minutes: 0,
            status: "todo".into(),
            sort: 0,
            done_at: None,
            phase_id: None,
            after_day: String::new(),
            due_day: String::new(),
            pin_day: String::new(),
            rough: false,
        }
    }
}

impl Item {
    pub fn sanitize(mut self, mode: Mode) -> Result<Self, String> {
        self.title = clip(&self.title, 100);
        if self.title.is_empty() {
            return Err("内容的名字不能是空的".into());
        }
        self.detail = clip(&self.detail, 1000);
        self.minutes = self.minutes.min(100 * 60);
        if mode == Mode::Content && self.minutes == 0 {
            self.minutes = DEFAULT_ITEM_MINUTES;
        }
        if !matches!(self.status.as_str(), "todo" | "done" | "skipped") {
            self.status = "todo".into();
        }
        for d in [&mut self.after_day, &mut self.due_day, &mut self.pin_day] {
            *d = parse_day(d).map(day_str).unwrap_or_default();
        }
        Ok(self)
    }

    /// 最早哪天开始、截止、钉在哪天
    pub fn dates(&self) -> (Option<NaiveDate>, Option<NaiveDate>, Option<NaiveDate>) {
        (
            parse_day(&self.after_day),
            parse_day(&self.due_day),
            parse_day(&self.pin_day),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub id: i64,
    /// 属于哪个计划（None = 单独的一件事）
    pub plan_id: Option<i64>,
    pub item_id: Option<i64>,
    pub day: String,
    /// 从哪天挪过来的（空 = 本来就是这天的）
    pub orig_day: String,
    pub title: String,
    pub minutes: u32,
    /// "HH:MM"，空 = 不固定
    pub start: String,
    /// todo / done / skipped
    pub status: String,
    /// 实际用了多少分钟
    pub spent: u32,
    /// 做完这项，对应的内容就完成了
    pub finishes: bool,
    pub note: String,
    /// auto（排程器排的，改计划时会重排）/ user（用户加的或改过的）
    pub source: String,
    pub sort: i64,
    /// 已经提醒过该开始了
    pub reminded: bool,
    pub done_at: Option<i64>,
    pub created_at: i64,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            id: 0,
            plan_id: None,
            item_id: None,
            day: String::new(),
            orig_day: String::new(),
            title: String::new(),
            minutes: 0,
            start: String::new(),
            status: "todo".into(),
            spent: 0,
            finishes: false,
            note: String::new(),
            source: "user".into(),
            sort: 0,
            reminded: false,
            done_at: None,
            created_at: 0,
        }
    }
}

impl Task {
    pub fn sanitize(mut self, today: NaiveDate) -> Result<Self, String> {
        self.title = clip(&self.title, 100);
        if self.title.is_empty() {
            return Err("要做什么还没写呢".into());
        }
        self.day = day_str(parse_day(&self.day).unwrap_or(today));
        if parse_day(&self.orig_day).is_none() {
            self.orig_day.clear();
        }
        self.minutes = self.minutes.min(720);
        self.spent = self.spent.min(24 * 60);
        self.start = self.start.trim().to_string();
        if !valid_hm(&self.start) {
            self.start.clear();
        }
        self.note = clip(&self.note, 1000);
        if !matches!(self.status.as_str(), "todo" | "done" | "skipped") {
            self.status = "todo".into();
        }
        if !matches!(self.source.as_str(), "auto" | "user") {
            self.source = "user".into();
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub plan_id: Option<i64>,
    #[serde(default)]
    pub day: String,
    pub content: String,
    #[serde(default)]
    pub created_at: i64,
}

// ---------------- 数据库：计划 ----------------

const PLAN_COLS: &str = "id, title, goal, emoji, mode, start_day, end_day, week_minutes, off_days, pace, remind_at, supervise, strategy, status, sort, arranged_day, created_at, updated_at, goal_id, share";

fn plan_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Plan> {
    let mode: String = r.get(4)?;
    let week: String = r.get(7)?;
    let off: String = r.get(8)?;
    let pace: String = r.get(9)?;
    Ok(Plan {
        id: r.get(0)?,
        title: r.get(1)?,
        goal: r.get(2)?,
        emoji: r.get(3)?,
        mode: if mode == "habit" {
            Mode::Habit
        } else {
            Mode::Content
        },
        start_day: r.get(5)?,
        end_day: r.get(6)?,
        week_minutes: serde_json::from_str(&week).unwrap_or([60; 7]),
        off_days: serde_json::from_str(&off).unwrap_or_default(),
        pace: if pace == "asap" {
            Pace::Asap
        } else {
            Pace::Even
        },
        remind_at: r.get(10)?,
        supervise: r.get::<_, i64>(11)? != 0,
        strategy: r.get(12)?,
        status: r.get(13)?,
        sort: r.get(14)?,
        arranged_day: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        goal_id: r.get(18)?,
        share: r.get::<_, i64>(19)?.clamp(0, 100) as u32,
    })
}

fn mode_str(m: Mode) -> &'static str {
    match m {
        Mode::Content => "content",
        Mode::Habit => "habit",
    }
}

fn pace_str(p: Pace) -> &'static str {
    match p {
        Pace::Even => "even",
        Pace::Asap => "asap",
    }
}

/// 新建或更新计划（不动 arranged_day），返回 id
pub fn save_plan(db: &Connection, p: &Plan, now: i64) -> rusqlite::Result<i64> {
    let week =
        serde_json::to_string(&p.week_minutes).unwrap_or_else(|_| "[60,60,60,60,60,60,60]".into());
    let off = serde_json::to_string(&p.off_days).unwrap_or_else(|_| "[]".into());
    if p.id > 0 {
        db.execute(
            "UPDATE plans SET title=?2, goal=?3, emoji=?4, mode=?5, start_day=?6, end_day=?7, week_minutes=?8, off_days=?9,
             pace=?10, remind_at=?11, supervise=?12, strategy=?13, status=?14, sort=?15, updated_at=?16, goal_id=?17, share=?18
             WHERE id=?1",
            params![
                p.id, p.title, p.goal, p.emoji, mode_str(p.mode), p.start_day, p.end_day, week, off,
                pace_str(p.pace), p.remind_at, p.supervise as i64, p.strategy, p.status, p.sort, now, p.goal_id, p.share
            ],
        )?;
        Ok(p.id)
    } else {
        let sort: i64 = db.query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM plans", [], |r| {
            r.get(0)
        })?;
        db.execute(
            "INSERT INTO plans(title, goal, emoji, mode, start_day, end_day, week_minutes, off_days, pace, remind_at,
             supervise, strategy, status, sort, created_at, updated_at, goal_id, share)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16, ?17)",
            params![
                p.title, p.goal, p.emoji, mode_str(p.mode), p.start_day, p.end_day, week, off, pace_str(p.pace),
                p.remind_at, p.supervise as i64, p.strategy, p.status, sort, now, p.goal_id, p.share
            ],
        )?;
        Ok(db.last_insert_rowid())
    }
}

pub fn get_plan(db: &Connection, id: i64) -> rusqlite::Result<Option<Plan>> {
    db.query_row(
        &format!("SELECT {PLAN_COLS} FROM plans WHERE id = ?1"),
        [id],
        plan_row,
    )
    .optional()
}

/// 全部计划：进行中、暂停、完成、归档，各自按排序
pub fn list_plans(db: &Connection) -> rusqlite::Result<Vec<Plan>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {PLAN_COLS} FROM plans ORDER BY
         CASE status WHEN 'active' THEN 0 WHEN 'paused' THEN 1 WHEN 'done' THEN 2 ELSE 3 END, sort, id"
    ))?;
    Ok(stmt.query_map([], plan_row)?.flatten().collect())
}

pub fn active_plans(db: &Connection) -> Vec<Plan> {
    list_plans(db)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.status == "active")
        .collect()
}

pub fn delete_plan(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM plans WHERE id = ?1", [id])? > 0)
}

pub fn set_plan_status(db: &Connection, id: i64, status: &str, now: i64) -> rusqlite::Result<bool> {
    Ok(db.execute(
        "UPDATE plans SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status, now],
    )? > 0)
}

fn set_arranged(db: &Connection, id: i64, day: &str) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE plans SET arranged_day = ?2 WHERE id = ?1",
        params![id, day],
    )?;
    Ok(())
}

// ---------------- 数据库：内容条目 ----------------

const ITEM_COLS: &str = "id, plan_id, title, detail, minutes, status, sort, done_at, phase_id, after_day, due_day, pin_day, rough";

fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: r.get(0)?,
        plan_id: r.get(1)?,
        title: r.get(2)?,
        detail: r.get(3)?,
        minutes: r.get::<_, i64>(4)?.max(0) as u32,
        status: r.get(5)?,
        sort: r.get(6)?,
        done_at: r.get(7)?,
        phase_id: r.get(8)?,
        after_day: r.get(9)?,
        due_day: r.get(10)?,
        pin_day: r.get(11)?,
        rough: r.get::<_, i64>(12)? != 0,
    })
}

pub fn list_items(db: &Connection, plan_id: i64) -> rusqlite::Result<Vec<Item>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {ITEM_COLS} FROM plan_items WHERE plan_id = ?1 ORDER BY sort, id"
    ))?;
    Ok(stmt.query_map([plan_id], item_row)?.flatten().collect())
}

pub fn get_item(db: &Connection, id: i64) -> rusqlite::Result<Option<Item>> {
    db.query_row(
        &format!("SELECT {ITEM_COLS} FROM plan_items WHERE id = ?1"),
        [id],
        item_row,
    )
    .optional()
}

/// 新建或更新一条内容，返回 id
pub fn save_item(db: &Connection, it: &Item) -> rusqlite::Result<i64> {
    if it.id > 0 {
        db.execute(
            "UPDATE plan_items SET title=?2, detail=?3, minutes=?4, status=?5, sort=?6, done_at=?7, phase_id=?8,
             after_day=?9, due_day=?10, pin_day=?11, rough=?12 WHERE id=?1",
            params![
                it.id, it.title, it.detail, it.minutes, it.status, it.sort, it.done_at, it.phase_id,
                it.after_day, it.due_day, it.pin_day, it.rough as i64
            ],
        )?;
        Ok(it.id)
    } else {
        db.execute(
            "INSERT INTO plan_items(plan_id, title, detail, minutes, status, sort, done_at, phase_id, after_day, due_day,
             pin_day, rough) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                it.plan_id, it.title, it.detail, it.minutes, it.status, it.sort, it.done_at, it.phase_id,
                it.after_day, it.due_day, it.pin_day, it.rough as i64
            ],
        )?;
        Ok(db.last_insert_rowid())
    }
}

/// 整体换成这份内容（按顺序）：有 id 的更新，没有的新建，列表里没有的删掉
pub fn set_items(db: &Connection, plan_id: i64, items: &[Item]) -> rusqlite::Result<()> {
    let keep: Vec<i64> = items.iter().filter(|i| i.id > 0).map(|i| i.id).collect();
    for old in list_items(db, plan_id)? {
        if !keep.contains(&old.id) {
            db.execute("DELETE FROM plan_items WHERE id = ?1", [old.id])?;
        }
    }
    for (i, it) in items.iter().enumerate() {
        let mut it = it.clone();
        it.plan_id = plan_id;
        it.sort = i as i64;
        // 别的计划的条目 id：当成新条目
        if it.id > 0 && get_item(db, it.id)?.is_none_or(|x| x.plan_id != plan_id) {
            it.id = 0;
        }
        save_item(db, &it)?;
    }
    Ok(())
}

pub fn delete_item(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM plan_items WHERE id = ?1", [id])? > 0)
}

/// 每条内容实际已经用掉的分钟（做完的事项的 spent 之和）
pub fn item_spent(db: &Connection, plan_id: i64) -> HashMap<i64, u32> {
    let Ok(mut stmt) = db.prepare(
        "SELECT item_id, SUM(spent) FROM plan_tasks WHERE plan_id = ?1 AND status = 'done' AND item_id IS NOT NULL GROUP BY item_id",
    ) else {
        return HashMap::new();
    };
    stmt.query_map([plan_id], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })
    .map(|it| it.flatten().map(|(a, b)| (a, b.max(0) as u32)).collect())
    .unwrap_or_default()
}

/// 这条内容还剩多少分钟
pub fn item_left(it: &Item, spent: u32) -> u32 {
    if it.status != "todo" {
        return 0;
    }
    it.minutes
        .saturating_sub(spent)
        .max(MIN_LEFT.min(it.minutes.max(1)))
}

// ---------------- 数据库：事项 ----------------

const TASK_COLS: &str = "id, plan_id, item_id, day, orig_day, title, minutes, start, status, spent, finishes, note, source, sort, reminded, done_at, created_at";

fn task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        plan_id: r.get(1)?,
        item_id: r.get(2)?,
        day: r.get(3)?,
        orig_day: r.get(4)?,
        title: r.get(5)?,
        minutes: r.get::<_, i64>(6)?.max(0) as u32,
        start: r.get(7)?,
        status: r.get(8)?,
        spent: r.get::<_, i64>(9)?.max(0) as u32,
        finishes: r.get::<_, i64>(10)? != 0,
        note: r.get(11)?,
        source: r.get(12)?,
        sort: r.get(13)?,
        reminded: r.get::<_, i64>(14)? != 0,
        done_at: r.get(15)?,
        created_at: r.get(16)?,
    })
}

pub fn save_task(db: &Connection, t: &Task, now: i64) -> rusqlite::Result<i64> {
    if t.id > 0 {
        db.execute(
            "UPDATE plan_tasks SET plan_id=?2, item_id=?3, day=?4, orig_day=?5, title=?6, minutes=?7, start=?8, status=?9,
             spent=?10, finishes=?11, note=?12, source=?13, sort=?14, reminded=?15, done_at=?16 WHERE id=?1",
            params![
                t.id, t.plan_id, t.item_id, t.day, t.orig_day, t.title, t.minutes, t.start, t.status, t.spent,
                t.finishes as i64, t.note, t.source, t.sort, t.reminded as i64, t.done_at
            ],
        )?;
        Ok(t.id)
    } else {
        db.execute(
            "INSERT INTO plan_tasks(plan_id, item_id, day, orig_day, title, minutes, start, status, spent, finishes, note,
             source, sort, reminded, done_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                t.plan_id, t.item_id, t.day, t.orig_day, t.title, t.minutes, t.start, t.status, t.spent,
                t.finishes as i64, t.note, t.source, t.sort, t.reminded as i64, t.done_at, now
            ],
        )?;
        Ok(db.last_insert_rowid())
    }
}

pub fn get_task(db: &Connection, id: i64) -> rusqlite::Result<Option<Task>> {
    db.query_row(
        &format!("SELECT {TASK_COLS} FROM plan_tasks WHERE id = ?1"),
        [id],
        task_row,
    )
    .optional()
}

pub fn delete_task(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM plan_tasks WHERE id = ?1", [id])? > 0)
}

/// 某一天的全部事项：有时间的按时间，没时间的在后面
pub fn day_tasks(db: &Connection, day: &str) -> Vec<Task> {
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM plan_tasks WHERE day = ?1 ORDER BY start = '', start, plan_id IS NULL, sort, id"
    )) else {
        return Vec::new();
    };
    stmt.query_map([day], task_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

/// 某个计划在 [from, to] 里的事项
pub fn plan_tasks(db: &Connection, plan_id: i64, from: &str, to: &str) -> Vec<Task> {
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM plan_tasks WHERE plan_id = ?1 AND day >= ?2 AND day <= ?3 ORDER BY day, sort, id"
    )) else {
        return Vec::new();
    };
    stmt.query_map(params![plan_id, from, to], task_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

/// [from, to] 里落库了的事项（今天以后的：挪过去的、提前加的）
fn range_tasks(db: &Connection, from: &str, to: &str) -> Vec<Task> {
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM plan_tasks WHERE day >= ?1 AND day <= ?2 ORDER BY day, start = '', start, sort, id"
    )) else {
        return Vec::new();
    };
    stmt.query_map(params![from, to], task_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

// ---------------- 数据库：备注 ----------------

pub fn add_note(db: &Connection, n: &Note, now: i64) -> rusqlite::Result<i64> {
    db.execute(
        "INSERT INTO plan_notes(plan_id, day, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![n.plan_id, n.day, n.content, now],
    )?;
    Ok(db.last_insert_rowid())
}

pub fn delete_note(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM plan_notes WHERE id = ?1", [id])? > 0)
}

fn note_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        plan_id: r.get(1)?,
        day: r.get(2)?,
        content: r.get(3)?,
        created_at: r.get(4)?,
    })
}

/// 某个计划的备注（新的在前）
pub fn plan_notes(db: &Connection, plan_id: i64, limit: u32) -> Vec<Note> {
    let Ok(mut stmt) = db.prepare(
        "SELECT id, plan_id, day, content, created_at FROM plan_notes WHERE plan_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
    ) else {
        return Vec::new();
    };
    stmt.query_map(params![plan_id, limit], note_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

/// 某一天写的备注（所有计划的 + 这一天自己的）
pub fn day_notes(db: &Connection, day: &str) -> Vec<Note> {
    let Ok(mut stmt) = db.prepare(
        "SELECT id, plan_id, day, content, created_at FROM plan_notes WHERE day = ?1 ORDER BY created_at, id",
    ) else {
        return Vec::new();
    };
    stmt.query_map([day], note_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

// ---------------- 排今天 ----------------

/// 习惯类轮到第几条：做完过的、关联了条目的次数
fn habit_rot(db: &Connection, plan_id: i64, before: &str) -> usize {
    db.query_row(
        "SELECT COUNT(*) FROM plan_tasks WHERE plan_id = ?1 AND item_id IS NOT NULL AND status = 'done' AND day < ?2",
        params![plan_id, before],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
    .max(0) as usize
}

/// 排程器的输入：从 `from` 起已经落库的事项占掉的分钟、还剩的内容
fn work_for(db: &Connection, plan: &Plan, items: &[Item], from: NaiveDate) -> (Work, Used) {
    let from_s = day_str(from);
    let stored = plan_tasks(db, plan.id, &from_s, "9999-12-31");
    let mut used = Used::new();
    let mut reserved: HashMap<i64, u32> = HashMap::new();
    for t in &stored {
        let Some(d) = parse_day(&t.day) else { continue };
        // 今天的：做完、跳过、待做都算占掉了；以后的：只有待做的
        if d == from || t.status == "todo" {
            *used.entry(d).or_default() += t.minutes;
        }
        if t.status == "todo"
            && let Some(i) = t.item_id
        {
            *reserved.entry(i).or_default() += t.minutes;
        }
    }
    let work = match plan.mode {
        Mode::Content => {
            let spent = item_spent(db, plan.id);
            Work::Content(
                items
                    .iter()
                    .filter(|i| i.status == "todo")
                    .map(|i| {
                        let sp = spent.get(&i.id).copied().unwrap_or(0);
                        let r = reserved.get(&i.id).copied().unwrap_or(0);
                        Left {
                            item_id: i.id,
                            title: i.title.clone(),
                            minutes: item_left(i, sp).saturating_sub(r),
                            started: sp > 0 || r > 0,
                        }
                    })
                    .collect(),
            )
        }
        Mode::Habit => {
            let today_todo = stored
                .iter()
                .filter(|t| t.item_id.is_some() && t.status == "todo")
                .count();
            Work::Habit {
                title: plan.title.clone(),
                items: items
                    .iter()
                    .filter(|i| i.status == "todo")
                    .map(|i| (i.id, i.title.clone(), i.minutes))
                    .collect(),
                rot: habit_rot(db, plan.id, &from_s) + today_todo,
            }
        }
    };
    (work, used)
}

/// 一项安排显示的名字：拆开做的标上是哪一段
pub fn slot_title(s: &Slot) -> String {
    match (s.continues, s.finishes || s.item_id.is_none()) {
        (false, true) => s.title.clone(),
        (false, false) => format!("{}（第一部分）", s.title),
        (true, false) => format!("{}（继续）", s.title),
        (true, true) => format!("{}（收尾）", s.title),
    }
}

/// 开始时间是不是已经过了（按「计划日」算，凌晨也不会算错）
pub fn start_passed(start: &str, now: i64) -> bool {
    use chrono::Timelike;
    let Some(s) = crate::proactive::parse_hm(start) else {
        return false;
    };
    let t = crate::reminders::to_local(now);
    supervise::plan_minute(s) < supervise::plan_minute(t.hour() * 60 + t.minute())
}

/// 用户改了计划之后重排今天（开始时间已经过了的不再「到点提醒」，免得刚保存就被叫）
pub fn rearrange_today(
    db: &Connection,
    plan: &Plan,
    day: NaiveDate,
    now: i64,
) -> rusqlite::Result<usize> {
    let n = arrange_today(db, plan, day, now)?;
    if !plan.remind_at.is_empty() && start_passed(&plan.remind_at, now) {
        db.execute(
            "UPDATE plan_tasks SET reminded = 1 WHERE plan_id = ?1 AND day = ?2 AND source = 'auto' AND status = 'todo'",
            params![plan.id, day_str(day)],
        )?;
    }
    Ok(n)
}

/// 重新排今天这个计划：删掉排程器排的、还没动过的，再按现在的内容排一次
pub fn arrange_today(
    db: &Connection,
    plan: &Plan,
    day: NaiveDate,
    now: i64,
) -> rusqlite::Result<usize> {
    let d = day_str(day);
    db.execute(
        "DELETE FROM plan_tasks WHERE plan_id = ?1 AND day = ?2 AND source = 'auto' AND status = 'todo'",
        params![plan.id, d],
    )?;
    set_arranged(db, plan.id, &d)?;
    if plan.status != "active" {
        return Ok(0);
    }
    let items = list_items(db, plan.id)?;
    let (work, used) = work_for(db, plan, &items, day);
    let p = schedule::project(&plan.shape(), &work, day, &used, 1);
    let Some((first, slots)) = p.days.first() else {
        return Ok(0);
    };
    if *first != day {
        return Ok(0);
    }
    let base_sort: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(sort), 0) + 1 FROM plan_tasks WHERE day = ?1",
            [&d],
            |r| r.get(0),
        )
        .unwrap_or(1);
    for (i, s) in slots.iter().enumerate() {
        save_task(
            db,
            &Task {
                plan_id: Some(plan.id),
                item_id: s.item_id,
                day: d.clone(),
                title: slot_title(s),
                minutes: s.minutes,
                start: plan.remind_at.clone(),
                finishes: s.finishes,
                source: "auto".into(),
                sort: base_sort + i as i64,
                ..Default::default()
            },
            now,
        )?;
    }
    Ok(slots.len())
}

/// 今天第一次用到时：单独的事情没做完的挪到今天；每个进行中的计划排好今天；过了截止的习惯计划标完成
pub fn ensure_day(db: &Connection, day: NaiveDate, now: i64) {
    let d = day_str(day);
    if let Err(e) = db.execute(
        "UPDATE plan_tasks SET orig_day = CASE WHEN orig_day = '' THEN day ELSE orig_day END, day = ?1
         WHERE plan_id IS NULL AND status = 'todo' AND day < ?1",
        [&d],
    ) {
        log::warn!("挪动没做完的事情失败：{e}");
    }
    for p in active_plans(db) {
        // 计划书里的线由计划书一起排
        if p.arranged_day == d || p.goal_id.is_some() {
            continue;
        }
        if p.mode == Mode::Habit && parse_day(&p.end_day).is_some_and(|e| e < day) {
            let _ = set_plan_status(db, p.id, "done", now);
            continue;
        }
        if let Err(e) = arrange_today(db, &p, day, now) {
            log::warn!("排今天的计划失败（#{}）：{e}", p.id);
        }
    }
    goal::ensure_day(db, day, now);
}

/// 改了计划之后重排今天：计划书里的线整个计划书一起排
pub fn rearrange_any(
    db: &Connection,
    plan: &Plan,
    day: NaiveDate,
    now: i64,
) -> rusqlite::Result<usize> {
    match plan.goal_id {
        Some(g) => goal::rearrange_today(db, g, day, now),
        None => rearrange_today(db, plan, day, now),
    }
}

// ---------------- 做完 / 跳过 / 挪动 ----------------

/// 把一项标成做完之后发生了什么（桌宠的反应用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct DoneEffect {
    /// 今天的事项全做完了
    pub all_today: bool,
    /// 这个计划的内容全部做完了（计划标成完成）
    pub plan_finished: bool,
}

/// 做完一项。`spent`：实际用了多久（空 = 按计划的分钟）；`finished`：对应的内容是不是做完了（空 = 按排的时候算的）
pub fn complete_task(
    db: &Connection,
    id: i64,
    spent: Option<u32>,
    finished: Option<bool>,
    now: i64,
) -> Result<DoneEffect, String> {
    let mut t = get_task(db, id)
        .map_err(|e| e.to_string())?
        .ok_or("这一项已经不在了")?;
    t.status = "done".into();
    t.spent = spent.unwrap_or(t.minutes).min(24 * 60);
    t.done_at = Some(now);
    if let Some(f) = finished {
        t.finishes = f;
    }
    save_task(db, &t, now).map_err(|e| e.to_string())?;
    let mut eff = DoneEffect::default();
    if let (Some(item_id), true) = (t.item_id, t.finishes)
        && let Ok(Some(mut it)) = get_item(db, item_id)
        && it.status == "todo"
    {
        it.status = "done".into();
        it.done_at = Some(now);
        save_item(db, &it).map_err(|e| e.to_string())?;
    }
    if let Some(pid) = t.plan_id
        && let Ok(Some(p)) = get_plan(db, pid)
        && p.mode == Mode::Content
        && p.status == "active"
    {
        let items = list_items(db, pid).unwrap_or_default();
        if !items.is_empty() && items.iter().all(|i| i.status != "todo") {
            let _ = set_plan_status(db, pid, "done", now);
            eff.plan_finished = true;
            if let Some(g) = p.goal_id {
                goal::check_done(db, g, now);
            }
        }
    }
    eff.all_today = day_tasks(db, &t.day).iter().all(|x| x.status != "todo");
    Ok(eff)
}

/// 改回没做 / 跳过。改回没做时，这一项标完成的内容也改回没做完。
pub fn set_task_status(db: &Connection, id: i64, status: &str, now: i64) -> Result<(), String> {
    let mut t = get_task(db, id)
        .map_err(|e| e.to_string())?
        .ok_or("这一项已经不在了")?;
    let was_done = t.status == "done";
    match status {
        "todo" | "skipped" => t.status = status.into(),
        _ => return Err("不认识的状态".into()),
    }
    t.spent = 0;
    t.done_at = None;
    save_task(db, &t, now).map_err(|e| e.to_string())?;
    if was_done
        && t.finishes
        && let Some(item_id) = t.item_id
        && let Ok(Some(mut it)) = get_item(db, item_id)
        && it.status == "done"
    {
        it.status = "todo".into();
        it.done_at = None;
        save_item(db, &it).map_err(|e| e.to_string())?;
        if let Some(pid) = t.plan_id
            && let Ok(Some(p)) = get_plan(db, pid)
            && p.status == "done"
        {
            let _ = set_plan_status(db, pid, "active", now);
            if let Some(g) = p.goal_id {
                let _ = db.execute(
                    "UPDATE goals SET status = 'active', updated_at = ?2 WHERE id = ?1 AND status = 'done'",
                    params![g, now],
                );
            }
        }
    }
    Ok(())
}

/// 挪到另一天（推到明天等）：变成用户的安排，重排时不会被删
pub fn move_task(db: &Connection, id: i64, to: NaiveDate, now: i64) -> Result<(), String> {
    let mut t = get_task(db, id)
        .map_err(|e| e.to_string())?
        .ok_or("这一项已经不在了")?;
    if t.orig_day.is_empty() {
        t.orig_day = t.day.clone();
    }
    t.day = day_str(to);
    if t.orig_day == t.day {
        t.orig_day.clear();
    }
    t.status = "todo".into();
    t.source = "user".into();
    t.reminded = false;
    t.spent = 0;
    t.done_at = None;
    save_task(db, &t, now).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------- 给界面看的 ----------------

#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    #[serde(flatten)]
    pub item: Item,
    pub spent: u32,
    pub left: u32,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct Progress {
    /// 内容类：总分钟、已做分钟、条目数
    pub total_minutes: u32,
    pub done_minutes: u32,
    pub items_total: u32,
    pub items_done: u32,
    /// 0~100
    pub percent: u32,
    /// 预计哪天做完（YYYY-MM-DD）
    pub finish: String,
    /// 到截止还差多少分钟（按每天上限排也来不及）
    pub short_minutes: u32,
    /// 过了截止还没做完
    pub overdue: bool,
    /// 连续做完的天数
    pub streak: u32,
    /// 最近 7 天：有安排的天数、全做完的天数
    pub week_days: u32,
    pub week_done: u32,
}

/// 某一天的完成情况
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DayStat {
    pub day: String,
    pub planned: u32,
    pub done: u32,
    pub tasks: u32,
    pub tasks_done: u32,
    pub tasks_todo: u32,
}

/// 往后某一天的预览
#[derive(Debug, Clone, Serialize)]
pub struct Upcoming {
    pub day: String,
    pub entries: Vec<UpcomingEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpcomingEntry {
    pub plan_id: Option<i64>,
    pub label: String,
    pub title: String,
    pub minutes: u32,
    /// 已经落库的（挪过去的、提前加的），其余是推算的
    pub stored: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub plan: Plan,
    pub items: Vec<ItemView>,
    pub progress: Progress,
    pub upcoming: Vec<Upcoming>,
    /// 最近几周每天的完成情况（有安排的日子）
    pub history: Vec<DayStat>,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanSummary {
    pub plan: Plan,
    pub progress: Progress,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    #[serde(flatten)]
    pub task: Task,
    /// 计划的名字（带图标；单独的事情为空）
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayView {
    pub day: String,
    pub tasks: Vec<TaskView>,
    pub notes: Vec<Note>,
    /// 连续把每天的安排做完的天数（所有计划一起算）
    pub streak: u32,
    pub planned: u32,
    pub done: u32,
    pub upcoming: Vec<Upcoming>,
    pub plans: Vec<PlanSummary>,
}

/// 每天的完成情况（只有有安排的日子），新的在前
pub fn day_stats(db: &Connection, plan_id: Option<i64>, to: &str, limit: u32) -> Vec<DayStat> {
    let sql = format!(
        "SELECT day, COALESCE(SUM(minutes), 0), COALESCE(SUM(CASE WHEN status = 'done' THEN spent ELSE 0 END), 0),
         COUNT(*), SUM(status = 'done'), SUM(status = 'todo')
         FROM plan_tasks WHERE day <= ?1 {} GROUP BY day ORDER BY day DESC LIMIT ?2",
        if plan_id.is_some() {
            "AND plan_id = ?3"
        } else {
            "AND ?3 IS NULL"
        }
    );
    let Ok(mut stmt) = db.prepare(&sql) else {
        return Vec::new();
    };
    stmt.query_map(params![to, limit, plan_id], |r| {
        Ok(DayStat {
            day: r.get(0)?,
            planned: r.get::<_, i64>(1)?.max(0) as u32,
            done: r.get::<_, i64>(2)?.max(0) as u32,
            tasks: r.get::<_, i64>(3)?.max(0) as u32,
            tasks_done: r.get::<_, i64>(4)?.max(0) as u32,
            tasks_todo: r.get::<_, i64>(5)?.max(0) as u32,
        })
    })
    .map(|it| it.flatten().collect())
    .unwrap_or_default()
}

/// 连续做完的天数（`stats` 新的在前）。今天还没做完不算断；全跳过、没有安排的日子不算断也不算数。
pub fn streak(stats: &[DayStat], today: &str) -> u32 {
    let mut n = 0;
    for s in stats {
        let complete = s.tasks_todo == 0 && s.tasks_done > 0;
        if s.day == today {
            if complete {
                n += 1;
            }
            continue;
        }
        if complete {
            n += 1;
        } else if s.tasks_done == 0 && s.tasks_todo == 0 {
            continue;
        } else {
            break;
        }
    }
    n
}

/// 一个计划的进度和往后的推算（`horizon` 天）
pub fn plan_progress(
    db: &Connection,
    p: &Plan,
    day: NaiveDate,
    horizon: u32,
) -> (Progress, schedule::Projection) {
    let items = list_items(db, p.id).unwrap_or_default();
    let spent = item_spent(db, p.id);
    let mut g = Progress::default();
    if p.mode == Mode::Content {
        for it in &items {
            if it.status == "skipped" {
                continue;
            }
            g.items_total += 1;
            g.total_minutes += it.minutes;
            let sp = spent.get(&it.id).copied().unwrap_or(0);
            if it.status == "done" {
                g.items_done += 1;
                g.done_minutes += it.minutes;
            } else {
                g.done_minutes += sp.min(it.minutes);
            }
        }
        if g.total_minutes > 0 {
            g.percent = (g.done_minutes as u64 * 100 / g.total_minutes as u64) as u32;
        }
    }
    let d = day_str(day);
    let stats = day_stats(db, Some(p.id), &d, 400);
    g.streak = streak(&stats, &d);
    let week_start = day_str(day - Duration::days(6));
    for s in stats.iter().filter(|s| s.day >= week_start) {
        g.week_days += 1;
        if s.tasks_todo == 0 && s.tasks_done > 0 {
            g.week_done += 1;
        }
    }
    // 计划书里的线：整个计划书一起推算，取这条线的那部分
    let in_goal = match (p.status.as_str(), p.goal_id) {
        ("active", Some(gid)) => goal::get_goal(db, gid)
            .ok()
            .flatten()
            .map(|gl| goal::projection(db, &gl, day, horizon).lane(p.id)),
        _ => None,
    };
    // 往后的推算从明天开始，假设今天排好的会做完
    let proj = if let Some(pr) = in_goal {
        if p.mode == Mode::Content {
            let left: u32 = items
                .iter()
                .map(|i| item_left(i, spent.get(&i.id).copied().unwrap_or(0)))
                .sum();
            g.finish = match pr.finish {
                Some(f) => day_str(f),
                None if left == 0 && g.percent < 100 => d.clone(),
                None => String::new(),
            };
            g.short_minutes = pr.short_minutes;
            g.overdue = parse_day(&p.end_day).is_some_and(|e| e < day);
        }
        pr
    } else if p.status == "active" {
        let tomorrow = day + Duration::days(1);
        let (work, used) = work_for(db, p, &items, tomorrow);
        // 今天还没做的也占着内容：从明天算起时把它们从内容里扣掉
        let work = match work {
            Work::Content(mut left) => {
                for t in plan_tasks(db, p.id, &d, &d) {
                    if t.status == "todo"
                        && let Some(iid) = t.item_id
                        && let Some(l) = left.iter_mut().find(|l| l.item_id == iid)
                    {
                        l.minutes = l.minutes.saturating_sub(t.minutes);
                        l.started = true;
                    }
                }
                Work::Content(left)
            }
            w => w,
        };
        let pr = schedule::project(&p.shape(), &work, tomorrow, &used, horizon);
        if p.mode == Mode::Content {
            let left: u32 = match &work {
                Work::Content(l) => l.iter().map(|x| x.minutes).sum(),
                _ => 0,
            };
            g.finish = match pr.finish {
                Some(f) => day_str(f),
                None if left == 0 && g.percent < 100 => d.clone(),
                None => String::new(),
            };
            g.short_minutes = pr.short_minutes;
            g.overdue = parse_day(&p.end_day).is_some_and(|e| e < day);
        }
        pr
    } else {
        schedule::Projection::default()
    };
    (g, proj)
}

fn entries_for(label: &str, plan_id: i64, slots: &[Slot]) -> Vec<UpcomingEntry> {
    slots
        .iter()
        .map(|s| UpcomingEntry {
            plan_id: Some(plan_id),
            label: label.to_string(),
            title: slot_title(s),
            minutes: s.minutes,
            stored: false,
        })
        .collect()
}

/// 把推算的和落库的合在一起，按天排好（从 `from` 起 `days` 天）
fn merge_upcoming(
    db: &Connection,
    plans: &[(Plan, schedule::Projection)],
    from: NaiveDate,
    days: u32,
    only_plan: Option<i64>,
) -> Vec<Upcoming> {
    let to = from + Duration::days(days as i64 - 1);
    let mut map: BTreeMap<NaiveDate, Vec<UpcomingEntry>> = BTreeMap::new();
    let all: Vec<Plan> = plans.iter().map(|(p, _)| p.clone()).collect();
    let labels = goal::plan_labels(db, &all);
    for t in range_tasks(db, &day_str(from), &day_str(to)) {
        if t.status != "todo" || (only_plan.is_some() && t.plan_id != only_plan) {
            continue;
        }
        let Some(d) = parse_day(&t.day) else { continue };
        map.entry(d).or_default().push(UpcomingEntry {
            plan_id: t.plan_id,
            label: t
                .plan_id
                .and_then(|id| labels.get(&id).cloned())
                .unwrap_or_default(),
            title: t.title,
            minutes: t.minutes,
            stored: true,
        });
    }
    for (p, pr) in plans {
        for (d, slots) in &pr.days {
            if *d > to {
                break;
            }
            let label = labels.get(&p.id).cloned().unwrap_or_else(|| p.label());
            map.entry(*d)
                .or_default()
                .extend(entries_for(&label, p.id, slots));
        }
    }
    map.into_iter()
        .map(|(d, entries)| Upcoming {
            day: day_str(d),
            entries,
        })
        .collect()
}

/// 计划详情
pub fn plan_view(db: &Connection, id: i64, day: NaiveDate) -> Option<PlanView> {
    let p = get_plan(db, id).ok().flatten()?;
    let items = list_items(db, id).unwrap_or_default();
    let spent = item_spent(db, id);
    let (progress, proj) = plan_progress(db, &p, day, 120);
    let upcoming = merge_upcoming(
        db,
        &[(p.clone(), proj)],
        day + Duration::days(1),
        14,
        Some(id),
    );
    let history = day_stats(db, Some(id), &day_str(day), 35);
    Some(PlanView {
        items: items
            .into_iter()
            .map(|it| {
                let sp = spent.get(&it.id).copied().unwrap_or(0);
                let left = if p.mode == Mode::Content {
                    item_left(&it, sp)
                } else {
                    0
                };
                ItemView {
                    item: it,
                    spent: sp,
                    left,
                }
            })
            .collect(),
        progress,
        upcoming,
        history,
        notes: plan_notes(db, id, 100),
        plan: p,
    })
}

pub fn summaries(db: &Connection, day: NaiveDate) -> Vec<PlanSummary> {
    list_plans(db)
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let (progress, _) = plan_progress(db, &p, day, 120);
            PlanSummary { plan: p, progress }
        })
        .collect()
}

/// 今天（会先排好今天）
pub fn today_view(db: &Connection, day: NaiveDate, now: i64) -> TodayView {
    ensure_day(db, day, now);
    let d = day_str(day);
    let all = list_plans(db).unwrap_or_default();
    let labels = goal::plan_labels(db, &all);
    let tasks: Vec<TaskView> = day_tasks(db, &d)
        .into_iter()
        .map(|t| TaskView {
            label: t
                .plan_id
                .and_then(|id| labels.get(&id).cloned())
                .unwrap_or_default(),
            task: t,
        })
        .collect();
    let planned = tasks
        .iter()
        .filter(|t| t.task.status != "skipped")
        .map(|t| t.task.minutes)
        .sum();
    let done = tasks
        .iter()
        .filter(|t| t.task.status == "done")
        .map(|t| t.task.spent)
        .sum();
    let mut projections = Vec::new();
    let mut plans = Vec::new();
    for p in all {
        let (progress, proj) = plan_progress(db, &p, day, 7);
        if p.status == "active" {
            projections.push((p.clone(), proj));
        }
        plans.push(PlanSummary { plan: p, progress });
    }
    let upcoming = merge_upcoming(db, &projections, day + Duration::days(1), 6, None);
    TodayView {
        streak: streak(&day_stats(db, None, &d, 400), &d),
        notes: day_notes(db, &d),
        day: d,
        tasks,
        planned,
        done,
        upcoming,
        plans,
    }
}

/// 今天的安排写成一句话（播报、聊天背景、本地回答用）。没有安排返回 None。
pub fn today_brief(db: &Connection, day: NaiveDate, max: usize) -> Option<(usize, usize, String)> {
    let tasks = day_tasks(db, &day_str(day));
    let live: Vec<&Task> = tasks.iter().filter(|t| t.status != "skipped").collect();
    if live.is_empty() {
        return None;
    }
    let done = live.iter().filter(|t| t.status == "done").count();
    let todo: Vec<String> = live
        .iter()
        .filter(|t| t.status == "todo")
        .take(max)
        .map(|t| {
            let mut s = t.title.clone();
            let mut extra = Vec::new();
            if !t.start.is_empty() {
                extra.push(t.start.clone());
            }
            if t.minutes > 0 {
                extra.push(minutes_text(t.minutes));
            }
            if !extra.is_empty() {
                s.push_str(&format!("（{}）", extra.join("，")));
            }
            s
        })
        .collect();
    Some((live.len(), done, todo.join("、")))
}

/// 90 → 「1 小时 30 分钟」
pub fn minutes_text(m: u32) -> String {
    match (m / 60, m % 60) {
        (0, x) => format!("{x} 分钟"),
        (h, 0) => format!("{h} 小时"),
        (h, x) => format!("{h} 小时 {x} 分钟"),
    }
}

/// 星期几（周一 = 0）
pub fn weekday_index(d: NaiveDate) -> usize {
    d.weekday().num_days_from_monday() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    const NOW: i64 = 1_800_000_000;

    fn plan(db: &Connection, mode: Mode, week: [u32; 7], end: &str, items: &[(&str, u32)]) -> Plan {
        let p = Plan {
            title: "线性代数".into(),
            mode,
            start_day: "2026-11-09".into(),
            end_day: end.into(),
            week_minutes: week,
            remind_at: "20:00".into(),
            ..Default::default()
        }
        .sanitize(d(11, 9))
        .unwrap();
        let id = save_plan(db, &p, NOW).unwrap();
        let its: Vec<Item> = items
            .iter()
            .map(|(t, m)| Item {
                title: t.to_string(),
                minutes: *m,
                ..Default::default()
            })
            .collect();
        set_items(db, id, &its).unwrap();
        get_plan(db, id).unwrap().unwrap()
    }

    #[test]
    fn sanitize_checks_input() {
        let t = d(11, 9);
        assert!(Plan::default().sanitize(t).is_err(), "没有名字");
        let p = Plan {
            title: "  读书 ".into(),
            week_minutes: [61, 0, 0, 0, 0, 0, 999],
            off_days: vec!["2026-11-20".into(), "bad".into(), "2026-11-20".into()],
            remind_at: "25:00".into(),
            status: "weird".into(),
            ..Default::default()
        }
        .sanitize(t)
        .unwrap();
        assert_eq!(p.title, "读书");
        assert_eq!(p.start_day, "2026-11-09");
        assert_eq!(p.week_minutes, [60, 0, 0, 0, 0, 0, 720]);
        assert_eq!(p.off_days, ["2026-11-20"]);
        assert_eq!((p.remind_at.as_str(), p.status.as_str()), ("", "active"));
        let mut q = p.clone();
        q.week_minutes = [0; 7];
        assert!(q.sanitize(t).is_err());
        let mut q = p.clone();
        q.end_day = "2026-11-01".into();
        assert!(q.sanitize(t).is_err(), "截止比开始早");
        let it = Item {
            title: "第1章".into(),
            ..Default::default()
        }
        .sanitize(Mode::Content)
        .unwrap();
        assert_eq!(it.minutes, DEFAULT_ITEM_MINUTES);
    }

    #[test]
    fn arranges_today_once_and_carries_over() {
        let db = db::open_in_memory().unwrap();
        let p = plan(
            &db,
            Mode::Content,
            [60; 7],
            "",
            &[("第1章", 90), ("第2章", 40)],
        );
        ensure_day(&db, d(11, 9), NOW);
        let t = day_tasks(&db, "2026-11-09");
        assert_eq!(t.len(), 1);
        assert_eq!(
            (t[0].title.as_str(), t[0].minutes),
            ("第1章（第一部分）", 60)
        );
        assert_eq!(t[0].start, "20:00", "用计划的开始时间");
        // 再调一次不会重复排；用户删掉了今天的也不会再排回来
        ensure_day(&db, d(11, 9), NOW);
        assert_eq!(day_tasks(&db, "2026-11-09").len(), 1);
        delete_task(&db, t[0].id).unwrap();
        ensure_day(&db, d(11, 9), NOW);
        assert!(day_tasks(&db, "2026-11-09").is_empty());
        // 第二天：昨天没做，第1章从头再排
        ensure_day(&db, d(11, 10), NOW);
        let t = day_tasks(&db, "2026-11-10");
        assert_eq!(t[0].title, "第1章（第一部分）");
        // 做了 60 分钟：第三天接着做第1章，再开始第2章
        complete_task(&db, t[0].id, None, None, NOW).unwrap();
        ensure_day(&db, d(11, 11), NOW);
        let t = day_tasks(&db, "2026-11-11");
        let titles: Vec<_> = t.iter().map(|x| (x.title.as_str(), x.minutes)).collect();
        assert_eq!(titles, [("第1章（收尾）", 30), ("第2章（第一部分）", 30)]);
        let _ = p;
    }

    #[test]
    fn completing_finishes_items_and_plan() {
        let db = db::open_in_memory().unwrap();
        let p = plan(&db, Mode::Content, [120; 7], "", &[("a", 60), ("b", 30)]);
        ensure_day(&db, d(11, 9), NOW);
        let t = day_tasks(&db, "2026-11-09");
        assert_eq!(t.len(), 2);
        assert!(t.iter().all(|x| x.finishes));
        let e = complete_task(&db, t[0].id, Some(50), None, NOW).unwrap();
        assert_eq!(
            e,
            DoneEffect {
                all_today: false,
                plan_finished: false
            }
        );
        assert_eq!(list_items(&db, p.id).unwrap()[0].status, "done");
        let e = complete_task(&db, t[1].id, None, None, NOW).unwrap();
        assert_eq!(
            e,
            DoneEffect {
                all_today: true,
                plan_finished: true
            }
        );
        assert_eq!(get_plan(&db, p.id).unwrap().unwrap().status, "done");
        // 改回没做：内容和计划都恢复
        set_task_status(&db, t[1].id, "todo", NOW).unwrap();
        assert_eq!(list_items(&db, p.id).unwrap()[1].status, "todo");
        assert_eq!(get_plan(&db, p.id).unwrap().unwrap().status, "active");
    }

    #[test]
    fn partial_completion_keeps_item_open() {
        let db = db::open_in_memory().unwrap();
        let p = plan(&db, Mode::Content, [60; 7], "", &[("a", 60)]);
        ensure_day(&db, d(11, 9), NOW);
        let t = day_tasks(&db, "2026-11-09");
        // 只做了 20 分钟，还没做完
        complete_task(&db, t[0].id, Some(20), Some(false), NOW).unwrap();
        let v = plan_view(&db, p.id, d(11, 9)).unwrap();
        assert_eq!(v.items[0].left, 40);
        assert_eq!(v.progress.percent, 33);
        ensure_day(&db, d(11, 10), NOW);
        let t = day_tasks(&db, "2026-11-10");
        assert_eq!(
            (t[0].title.as_str(), t[0].minutes, t[0].finishes),
            ("a（收尾）", 40, true)
        );
        // 超时了也没做完：至少再留 15 分钟
        complete_task(&db, t[0].id, Some(60), Some(false), NOW).unwrap();
        let v = plan_view(&db, p.id, d(11, 10)).unwrap();
        assert_eq!(v.items[0].left, MIN_LEFT);
    }

    #[test]
    fn moved_task_reserves_content_and_rearrange_keeps_user_tasks() {
        let db = db::open_in_memory().unwrap();
        let p = plan(&db, Mode::Content, [60; 7], "", &[("a", 60), ("b", 60)]);
        ensure_day(&db, d(11, 9), NOW);
        let t = day_tasks(&db, "2026-11-09");
        move_task(&db, t[0].id, d(11, 10), NOW).unwrap();
        // 明天已经有挪过去的 a：明天不再排别的（已经占满），后天排 b
        let v = plan_view(&db, p.id, d(11, 9)).unwrap();
        let days: Vec<_> = v
            .upcoming
            .iter()
            .map(|u| {
                (
                    u.day.as_str(),
                    u.entries[0].title.as_str(),
                    u.entries[0].stored,
                )
            })
            .collect();
        assert_eq!(
            days,
            [("2026-11-10", "a", true), ("2026-11-11", "b", false)]
        );
        // 改了计划重排今天：用户挪过的不动
        arrange_today(&db, &p, d(11, 9), NOW).unwrap();
        assert_eq!(day_tasks(&db, "2026-11-10").len(), 1);
        let today = day_tasks(&db, "2026-11-09");
        assert_eq!(today.len(), 1, "a 被挪走了，今天排 b：{today:?}");
        assert_eq!(today[0].title, "b");
    }

    #[test]
    fn standalone_tasks_roll_over() {
        let db = db::open_in_memory().unwrap();
        let id = save_task(
            &db,
            &Task {
                day: "2026-11-08".into(),
                title: "交报告".into(),
                ..Default::default()
            },
            NOW,
        )
        .unwrap();
        ensure_day(&db, d(11, 9), NOW);
        let t = get_task(&db, id).unwrap().unwrap();
        assert_eq!(
            (t.day.as_str(), t.orig_day.as_str()),
            ("2026-11-09", "2026-11-08")
        );
    }

    #[test]
    fn habit_plan_every_day_and_ends() {
        let db = db::open_in_memory().unwrap();
        let p = plan(
            &db,
            Mode::Habit,
            [30, 30, 30, 30, 30, 0, 0],
            "2026-11-10",
            &[],
        );
        ensure_day(&db, d(11, 9), NOW);
        let t = day_tasks(&db, "2026-11-09");
        assert_eq!((t[0].title.as_str(), t[0].minutes), ("线性代数", 30));
        ensure_day(&db, d(11, 11), NOW);
        assert_eq!(
            get_plan(&db, p.id).unwrap().unwrap().status,
            "done",
            "过了截止"
        );
    }

    #[test]
    fn streaks_and_today_view() {
        let db = db::open_in_memory().unwrap();
        plan(&db, Mode::Habit, [30; 7], "", &[]);
        for day in [8u32, 9, 10] {
            ensure_day(&db, d(11, day), NOW);
        }
        // 8 号没做、9 号做了、10 号（今天）还没做
        let t9 = day_tasks(&db, "2026-11-09");
        complete_task(&db, t9[0].id, None, None, NOW).unwrap();
        let v = today_view(&db, d(11, 10), NOW);
        assert_eq!(v.streak, 1);
        assert_eq!((v.planned, v.done), (30, 0));
        assert_eq!(v.upcoming.len(), 6);
        // 今天也做了
        complete_task(&db, v.tasks[0].task.id, None, None, NOW).unwrap();
        let v = today_view(&db, d(11, 10), NOW);
        assert_eq!(v.streak, 2);
        let (n, done, _) = today_brief(&db, d(11, 10), 3).unwrap();
        assert_eq!((n, done), (1, 1));
    }

    #[test]
    fn deadline_progress_reports_behind() {
        let db = db::open_in_memory().unwrap();
        // 3 天，每天 60 分钟，内容 300 分钟
        let p = plan(&db, Mode::Content, [60; 7], "2026-11-11", &[("a", 300)]);
        ensure_day(&db, d(11, 9), NOW);
        let (g, _) = plan_progress(&db, &p, d(11, 9), 60);
        assert_eq!(g.short_minutes, 120);
        assert_eq!(g.finish, "2026-11-13");
        assert!(!g.overdue);
    }

    #[test]
    fn minutes_text_reads_well() {
        assert_eq!(minutes_text(45), "45 分钟");
        assert_eq!(minutes_text(120), "2 小时");
        assert_eq!(minutes_text(90), "1 小时 30 分钟");
    }
}
