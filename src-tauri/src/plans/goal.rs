//! 计划书：复杂的长期计划（几个月、几条线并行、按阶段变强度、钉在日历上的节点、每周固定的事、缓冲期、学习资源）。
//!
//! - 一个计划书下面有几条「线」，每条线就是一个计划（`plans.goal_id` 指过来），今天的事项、打卡、专注、监督都照旧按计划走；
//!   内容线是 content 模式，每周固定的事（联系老师、周日自查之类）是 habit 模式、按它自己的每周分钟排。
//!   原来的单个计划 `goal_id` 为空，照旧用。
//! - 每天几条线怎么分时间由 `lanes.rs` 统一排（阶段的每周分钟、缓冲期、钉住的日子、截止、份额交替）；
//!   今天的在当天第一次用到时落库，和单个计划一样。
//! - 路线图里的内容先是粗的一大块（`rough`），快排到的时候拆成具体的小块：新建时模型细化最近两三周，
//!   以后每次排今天时本地自动把两周内要做的粗块按说明里列的几项拆开（不调模型）；想按进度重新细化就点「调整下周」。
//! - 草稿（还没保存的）里新的阶段、线、节点、条目用负数当临时 id，互相引用；保存时换成真的 id。

use super::lanes::{self, BufferShape, GoalShape, Lane, LaneItem, LaneShort, LaneWork, PhaseShape};
use super::*;
use std::collections::HashSet;

pub const MAX_GOALS: usize = 20;
pub const MAX_TRACKS: usize = 10;
pub const MAX_PHASES: usize = 12;
pub const MAX_MILESTONES: usize = 40;
/// 粗的内容在几天之内要排到了就拆开
pub const EXPAND_DAYS: i64 = 14;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Buffer {
    pub start: String,
    pub end: String,
    /// 这几天按平时的百分之几排（0 = 完全不排）
    pub percent: u32,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Resource {
    pub title: String,
    pub url: String,
    pub note: String,
    /// 挂在哪条线 / 哪个阶段上（都空 = 整个计划书的）
    pub plan_id: Option<i64>,
    pub phase_id: Option<i64>,
}

/// 先问再排：她问的问题和用户的回答
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Qa {
    pub q: String,
    pub a: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Goal {
    pub id: i64,
    pub title: String,
    pub emoji: String,
    /// 目标一句话
    pub summary: String,
    /// 用户的原话
    pub source: String,
    /// 计划书原文（可以整段改）
    pub doc: String,
    /// 她的安排思路
    pub strategy: String,
    pub start_day: String,
    pub end_day: String,
    /// 平时（不在阶段里、阶段没写时）周一到周日每天一共能用多少分钟
    pub week_minutes: [u32; 7],
    pub off_days: Vec<String>,
    pub buffers: Vec<Buffer>,
    pub resources: Vec<Resource>,
    /// 每周自查清单
    pub checklist: Vec<String>,
    pub answers: Vec<Qa>,
    pub remind_at: String,
    /// 每周哪天自查（0 = 周一 … 6 = 周日；-1 = 不自查）
    pub review_day: i64,
    pub status: String,
    pub sort: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// 粗的内容已经拆到哪天了（只读）
    pub expanded_until: String,
    #[serde(skip)]
    pub arranged_day: String,
}

impl Default for Goal {
    fn default() -> Self {
        Self {
            id: 0,
            title: String::new(),
            emoji: String::new(),
            summary: String::new(),
            source: String::new(),
            doc: String::new(),
            strategy: String::new(),
            start_day: String::new(),
            end_day: String::new(),
            week_minutes: [60, 60, 60, 60, 60, 90, 90],
            off_days: Vec::new(),
            buffers: Vec::new(),
            resources: Vec::new(),
            checklist: Vec::new(),
            answers: Vec::new(),
            remind_at: String::new(),
            review_day: 6,
            status: "active".into(),
            sort: 0,
            created_at: 0,
            updated_at: 0,
            expanded_until: String::new(),
            arranged_day: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Phase {
    pub id: i64,
    pub goal_id: i64,
    pub title: String,
    pub start_day: String,
    pub end_day: String,
    /// 这个阶段的目标
    pub objective: String,
    /// 可检验的完成标准
    pub criteria: Vec<String>,
    /// 这个阶段每天一共能用多少分钟（None = 用计划书的）
    pub week_minutes: Option<[u32; 7]>,
    /// 各条线的份额（线的 id → 份额；空 = 用每条线自己的默认份额）
    pub shares: BTreeMap<i64, u32>,
    pub sort: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Milestone {
    pub id: i64,
    pub goal_id: i64,
    pub phase_id: Option<i64>,
    pub plan_id: Option<i64>,
    pub title: String,
    pub day: String,
    /// 怎么算达标
    pub criteria: String,
    /// todo / passed / missed / skipped
    pub status: String,
    /// 结果、心得
    pub result: String,
    pub checked_at: Option<i64>,
    pub sort: i64,
}

impl Default for Milestone {
    fn default() -> Self {
        Self {
            id: 0,
            goal_id: 0,
            phase_id: None,
            plan_id: None,
            title: String::new(),
            day: String::new(),
            criteria: String::new(),
            status: "todo".into(),
            result: String::new(),
            checked_at: None,
            sort: 0,
        }
    }
}

/// 一条线：计划本身 + 它的内容
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Track {
    pub plan: Plan,
    pub items: Vec<Item>,
}

/// 整个计划书（新建、重新安排、调整下周的草稿也是这个样子）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GoalFull {
    pub goal: Goal,
    pub tracks: Vec<Track>,
    pub phases: Vec<Phase>,
    pub milestones: Vec<Milestone>,
}

fn clip_list(v: &[String], n: usize, each: usize) -> Vec<String> {
    v.iter()
        .map(|s| clip(s, each))
        .filter(|s| !s.is_empty())
        .take(n)
        .collect()
}

fn valid_url(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    l.starts_with("https://") || l.starts_with("http://")
}

fn week_sane(w: [u32; 7]) -> [u32; 7] {
    w.map(|m| (m.min(720) / 5) * 5)
}

impl Goal {
    pub fn sanitize(mut self, today: NaiveDate) -> Result<Self, String> {
        self.title = clip(&self.title, 60);
        if self.title.is_empty() {
            return Err("计划书还没有名字".into());
        }
        self.emoji = self.emoji.trim().chars().take(4).collect();
        self.summary = clip(&self.summary, 500);
        self.source = clip(&self.source, 8000);
        self.doc = clip(&self.doc, 20000);
        self.strategy = clip(&self.strategy, 3000);
        let start = parse_day(&self.start_day).unwrap_or(today);
        self.start_day = day_str(start);
        match parse_day(&self.end_day) {
            Some(e) if e < start => return Err("结束日期比开始日期还早".into()),
            Some(e) => self.end_day = day_str(e),
            None => self.end_day.clear(),
        }
        self.week_minutes = week_sane(self.week_minutes);
        let mut off: Vec<NaiveDate> = self.off_days.iter().filter_map(|d| parse_day(d)).collect();
        off.sort_unstable();
        off.dedup();
        off.truncate(400);
        self.off_days = off.into_iter().map(day_str).collect();
        self.buffers = self
            .buffers
            .iter()
            .filter_map(|b| {
                let s = parse_day(&b.start)?;
                let e = parse_day(&b.end).unwrap_or(s).max(s);
                Some(Buffer {
                    start: day_str(s),
                    end: day_str(e),
                    percent: (b.percent.min(100) / 5) * 5,
                    note: clip(&b.note, 100),
                })
            })
            .take(30)
            .collect();
        self.buffers.sort_by(|a, b| a.start.cmp(&b.start));
        self.resources = self
            .resources
            .iter()
            .filter_map(|r| {
                let url = r.url.trim();
                let url = if valid_url(url) {
                    clip(url, 500)
                } else {
                    String::new()
                };
                let title = clip(&r.title, 100);
                (!title.is_empty() || !url.is_empty()).then(|| Resource {
                    title: if title.is_empty() { url.clone() } else { title },
                    url,
                    note: clip(&r.note, 300),
                    plan_id: r.plan_id,
                    phase_id: r.phase_id,
                })
            })
            .take(60)
            .collect();
        self.checklist = clip_list(&self.checklist, 15, 200);
        self.answers = self
            .answers
            .iter()
            .map(|x| Qa {
                q: clip(&x.q, 300),
                a: clip(&x.a, 1000),
            })
            .filter(|x| !x.q.is_empty())
            .take(6)
            .collect();
        self.remind_at = self.remind_at.trim().to_string();
        if !valid_hm(&self.remind_at) {
            self.remind_at.clear();
        }
        self.review_day = self.review_day.clamp(-1, 6);
        if !matches!(
            self.status.as_str(),
            "active" | "paused" | "done" | "archived"
        ) {
            self.status = "active".into();
        }
        Ok(self)
    }

    pub fn label(&self) -> String {
        if self.emoji.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.emoji, self.title)
        }
    }
}

impl Phase {
    pub fn sanitize(mut self) -> Result<Self, String> {
        self.title = clip(&self.title, 60);
        if self.title.is_empty() {
            return Err("阶段还没有名字".into());
        }
        let s = parse_day(&self.start_day).ok_or("阶段的开始日期不对")?;
        let e = parse_day(&self.end_day).ok_or("阶段的结束日期不对")?;
        if e < s {
            return Err(format!("「{}」的结束日期比开始还早", self.title));
        }
        self.start_day = day_str(s);
        self.end_day = day_str(e);
        self.objective = clip(&self.objective, 500);
        self.criteria = clip_list(&self.criteria, 8, 200);
        self.week_minutes = self
            .week_minutes
            .map(week_sane)
            .filter(|w| w.iter().any(|m| *m > 0));
        self.shares = self
            .shares
            .into_iter()
            .map(|(k, v)| (k, v.min(100)))
            .collect();
        Ok(self)
    }

    pub fn range(&self) -> Option<(NaiveDate, NaiveDate)> {
        Some((parse_day(&self.start_day)?, parse_day(&self.end_day)?))
    }
}

impl Milestone {
    pub fn sanitize(mut self) -> Result<Self, String> {
        self.title = clip(&self.title, 80);
        if self.title.is_empty() {
            return Err("节点还没有名字".into());
        }
        self.day = day_str(parse_day(&self.day).ok_or("节点的日期不对")?);
        self.criteria = clip(&self.criteria, 500);
        self.result = clip(&self.result, 1000);
        if !matches!(
            self.status.as_str(),
            "todo" | "passed" | "missed" | "skipped"
        ) {
            self.status = "todo".into();
        }
        Ok(self)
    }
}

impl GoalFull {
    /// 检查整个计划书：数量上限、日期、互相引用的 id 都要对得上（对不上的引用清掉）
    pub fn sanitize(mut self, today: NaiveDate) -> Result<Self, String> {
        let goal = self.goal.clone().sanitize(today)?;
        if self.tracks.is_empty() {
            return Err("计划书里至少要有一条线".into());
        }
        if self.tracks.len() > MAX_TRACKS {
            return Err(format!("线太多啦（最多 {MAX_TRACKS} 条）"));
        }
        if self.phases.len() > MAX_PHASES {
            return Err(format!("阶段太多啦（最多 {MAX_PHASES} 个）"));
        }
        self.milestones.truncate(MAX_MILESTONES);
        // 没有 id 的（0）发一个临时的负数 id，免得互相认错
        let mut next = -1_000_000i64;
        let mut fresh = |id: &mut i64| {
            if *id == 0 {
                *id = next;
                next -= 1;
            }
        };
        for t in &mut self.tracks {
            fresh(&mut t.plan.id);
            for it in &mut t.items {
                fresh(&mut it.id);
            }
        }
        for p in &mut self.phases {
            fresh(&mut p.id);
        }
        for m in &mut self.milestones {
            fresh(&mut m.id);
        }
        let mut phases: Vec<Phase> = self
            .phases
            .into_iter()
            .map(|p| p.sanitize())
            .collect::<Result<_, _>>()?;
        phases.sort_by(|a, b| a.start_day.cmp(&b.start_day));
        let track_ids: HashSet<i64> = self.tracks.iter().map(|t| t.plan.id).collect();
        let phase_ids: HashSet<i64> = phases.iter().map(|p| p.id).collect();
        let ok_track = |x: Option<i64>| x.filter(|i| track_ids.contains(i));
        let ok_phase = |x: Option<i64>| x.filter(|i| phase_ids.contains(i));
        for (i, p) in phases.iter_mut().enumerate() {
            p.sort = i as i64;
            p.shares.retain(|k, _| track_ids.contains(k));
        }
        let mut tracks = Vec::new();
        for (i, t) in self.tracks.into_iter().enumerate() {
            let mut plan = t.plan;
            plan.sort = i as i64;
            if plan.mode == Mode::Content {
                // 内容线的时间由计划书统一分：日期跟着计划书
                plan.start_day = goal.start_day.clone();
                plan.end_day = goal.end_day.clone();
                plan.week_minutes = goal.week_minutes;
            } else if plan.start_day.is_empty() {
                plan.start_day = goal.start_day.clone();
            }
            if plan.status != "done" {
                plan.status = match goal.status.as_str() {
                    "paused" | "archived" => goal.status.clone(),
                    _ => "active".into(),
                };
            }
            let mut plan = plan.sanitize(today)?;
            let mut items = Vec::new();
            for it in t.items.into_iter().take(MAX_ITEMS) {
                let mut it = it.sanitize(plan.mode)?;
                it.phase_id = ok_phase(it.phase_id);
                items.push(it);
            }
            // 做完了的内容线又加了新内容：重新开始
            if plan.status == "done"
                && plan.mode == Mode::Content
                && goal.status == "active"
                && items.iter().any(|i| i.status == "todo")
            {
                plan.status = "active".into();
            }
            tracks.push(Track { plan, items });
        }
        let mut milestones = Vec::new();
        for m in self.milestones {
            let mut m = m.sanitize()?;
            m.phase_id = ok_phase(m.phase_id);
            m.plan_id = ok_track(m.plan_id);
            milestones.push(m);
        }
        milestones.sort_by(|a, b| a.day.cmp(&b.day));
        let mut goal = goal;
        for r in &mut goal.resources {
            r.plan_id = ok_track(r.plan_id);
            r.phase_id = ok_phase(r.phase_id);
        }
        Ok(GoalFull {
            goal,
            tracks,
            phases,
            milestones,
        })
    }
}

// ---------------- 数据库 ----------------

const GOAL_COLS: &str = "id, title, emoji, summary, source, doc, strategy, start_day, end_day, week_minutes, off_days, buffers, resources, checklist, answers, remind_at, review_day, status, sort, arranged_day, expanded_until, created_at, updated_at";

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

fn goal_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Goal> {
    let js = |i: usize| -> rusqlite::Result<String> { r.get(i) };
    Ok(Goal {
        id: r.get(0)?,
        title: r.get(1)?,
        emoji: r.get(2)?,
        summary: r.get(3)?,
        source: r.get(4)?,
        doc: r.get(5)?,
        strategy: r.get(6)?,
        start_day: r.get(7)?,
        end_day: r.get(8)?,
        week_minutes: serde_json::from_str(&js(9)?).unwrap_or([60; 7]),
        off_days: serde_json::from_str(&js(10)?).unwrap_or_default(),
        buffers: serde_json::from_str(&js(11)?).unwrap_or_default(),
        resources: serde_json::from_str(&js(12)?).unwrap_or_default(),
        checklist: serde_json::from_str(&js(13)?).unwrap_or_default(),
        answers: serde_json::from_str(&js(14)?).unwrap_or_default(),
        remind_at: r.get(15)?,
        review_day: r.get(16)?,
        status: r.get(17)?,
        sort: r.get(18)?,
        arranged_day: r.get(19)?,
        expanded_until: r.get(20)?,
        created_at: r.get(21)?,
        updated_at: r.get(22)?,
    })
}

/// 新建或更新计划书本身（不动 arranged_day / expanded_until），返回 id
pub fn save_goal(db: &Connection, g: &Goal, now: i64) -> rusqlite::Result<i64> {
    let (week, off, buf, res, chk, ans) = (
        json(&g.week_minutes),
        json(&g.off_days),
        json(&g.buffers),
        json(&g.resources),
        json(&g.checklist),
        json(&g.answers),
    );
    if g.id > 0 {
        db.execute(
            "UPDATE goals SET title=?2, emoji=?3, summary=?4, source=?5, doc=?6, strategy=?7, start_day=?8, end_day=?9,
             week_minutes=?10, off_days=?11, buffers=?12, resources=?13, checklist=?14, answers=?15, remind_at=?16,
             review_day=?17, status=?18, sort=?19, updated_at=?20 WHERE id=?1",
            params![
                g.id, g.title, g.emoji, g.summary, g.source, g.doc, g.strategy, g.start_day, g.end_day, week, off, buf,
                res, chk, ans, g.remind_at, g.review_day, g.status, g.sort, now
            ],
        )?;
        Ok(g.id)
    } else {
        let sort: i64 = db.query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM goals", [], |r| {
            r.get(0)
        })?;
        db.execute(
            "INSERT INTO goals(title, emoji, summary, source, doc, strategy, start_day, end_day, week_minutes, off_days,
             buffers, resources, checklist, answers, remind_at, review_day, status, sort, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19)",
            params![
                g.title, g.emoji, g.summary, g.source, g.doc, g.strategy, g.start_day, g.end_day, week, off, buf, res,
                chk, ans, g.remind_at, g.review_day, g.status, sort, now
            ],
        )?;
        Ok(db.last_insert_rowid())
    }
}

pub fn get_goal(db: &Connection, id: i64) -> rusqlite::Result<Option<Goal>> {
    db.query_row(
        &format!("SELECT {GOAL_COLS} FROM goals WHERE id = ?1"),
        [id],
        goal_row,
    )
    .optional()
}

pub fn list_goals(db: &Connection) -> rusqlite::Result<Vec<Goal>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {GOAL_COLS} FROM goals ORDER BY
         CASE status WHEN 'active' THEN 0 WHEN 'paused' THEN 1 WHEN 'done' THEN 2 ELSE 3 END, sort, id"
    ))?;
    Ok(stmt.query_map([], goal_row)?.flatten().collect())
}

pub fn delete_goal(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM goals WHERE id = ?1", [id])? > 0)
}

/// 改计划书的状态，线跟着改（做完了的线不动；恢复 / 再开始时没做完的线恢复）
pub fn set_goal_status(db: &Connection, id: i64, status: &str, now: i64) -> rusqlite::Result<bool> {
    let n = db.execute(
        "UPDATE goals SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status, now],
    )?;
    if status != "done" {
        db.execute(
            "UPDATE plans SET status = ?2, updated_at = ?3 WHERE goal_id = ?1 AND status != 'done'",
            params![id, status, now],
        )?;
    }
    Ok(n > 0)
}

fn set_goal_mark(db: &Connection, id: i64, col: &str, day: &str) -> rusqlite::Result<()> {
    debug_assert!(matches!(col, "arranged_day" | "expanded_until"));
    db.execute(
        &format!("UPDATE goals SET {col} = ?2 WHERE id = ?1"),
        params![id, day],
    )?;
    Ok(())
}

/// 计划书的线（按顺序）
pub fn goal_tracks(db: &Connection, goal_id: i64) -> Vec<Plan> {
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {PLAN_COLS} FROM plans WHERE goal_id = ?1 ORDER BY sort, id"
    )) else {
        return Vec::new();
    };
    stmt.query_map([goal_id], plan_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

const PHASE_COLS: &str =
    "id, goal_id, title, start_day, end_day, objective, criteria, week_minutes, shares, sort";

fn phase_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Phase> {
    let crit: String = r.get(6)?;
    let week: String = r.get(7)?;
    let shares: String = r.get(8)?;
    Ok(Phase {
        id: r.get(0)?,
        goal_id: r.get(1)?,
        title: r.get(2)?,
        start_day: r.get(3)?,
        end_day: r.get(4)?,
        objective: r.get(5)?,
        criteria: serde_json::from_str(&crit).unwrap_or_default(),
        week_minutes: serde_json::from_str(&week).ok(),
        shares: serde_json::from_str(&shares).unwrap_or_default(),
        sort: r.get(9)?,
    })
}

pub fn list_phases(db: &Connection, goal_id: i64) -> Vec<Phase> {
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {PHASE_COLS} FROM goal_phases WHERE goal_id = ?1 ORDER BY start_day, sort, id"
    )) else {
        return Vec::new();
    };
    stmt.query_map([goal_id], phase_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

pub fn save_phase(db: &Connection, p: &Phase) -> rusqlite::Result<i64> {
    let crit = json(&p.criteria);
    let week = p.week_minutes.map(|w| json(&w)).unwrap_or_default();
    let shares = json(&p.shares);
    if p.id > 0 {
        db.execute(
            "UPDATE goal_phases SET title=?2, start_day=?3, end_day=?4, objective=?5, criteria=?6, week_minutes=?7,
             shares=?8, sort=?9 WHERE id=?1",
            params![
                p.id,
                p.title,
                p.start_day,
                p.end_day,
                p.objective,
                crit,
                week,
                shares,
                p.sort
            ],
        )?;
        Ok(p.id)
    } else {
        db.execute(
            "INSERT INTO goal_phases(goal_id, title, start_day, end_day, objective, criteria, week_minutes, shares, sort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                p.goal_id,
                p.title,
                p.start_day,
                p.end_day,
                p.objective,
                crit,
                week,
                shares,
                p.sort
            ],
        )?;
        Ok(db.last_insert_rowid())
    }
}

const MS_COLS: &str =
    "id, goal_id, phase_id, plan_id, title, day, criteria, status, result, checked_at, sort";

fn ms_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Milestone> {
    Ok(Milestone {
        id: r.get(0)?,
        goal_id: r.get(1)?,
        phase_id: r.get(2)?,
        plan_id: r.get(3)?,
        title: r.get(4)?,
        day: r.get(5)?,
        criteria: r.get(6)?,
        status: r.get(7)?,
        result: r.get(8)?,
        checked_at: r.get(9)?,
        sort: r.get(10)?,
    })
}

pub fn list_milestones(db: &Connection, goal_id: i64) -> Vec<Milestone> {
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {MS_COLS} FROM goal_milestones WHERE goal_id = ?1 ORDER BY day, sort, id"
    )) else {
        return Vec::new();
    };
    stmt.query_map([goal_id], ms_row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

pub fn get_milestone(db: &Connection, id: i64) -> rusqlite::Result<Option<Milestone>> {
    db.query_row(
        &format!("SELECT {MS_COLS} FROM goal_milestones WHERE id = ?1"),
        [id],
        ms_row,
    )
    .optional()
}

pub fn save_milestone(db: &Connection, m: &Milestone) -> rusqlite::Result<i64> {
    if m.id > 0 {
        db.execute(
            "UPDATE goal_milestones SET phase_id=?2, plan_id=?3, title=?4, day=?5, criteria=?6, status=?7, result=?8,
             checked_at=?9, sort=?10 WHERE id=?1",
            params![
                m.id,
                m.phase_id,
                m.plan_id,
                m.title,
                m.day,
                m.criteria,
                m.status,
                m.result,
                m.checked_at,
                m.sort
            ],
        )?;
        Ok(m.id)
    } else {
        db.execute(
            "INSERT INTO goal_milestones(goal_id, phase_id, plan_id, title, day, criteria, status, result, checked_at, sort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                m.goal_id,
                m.phase_id,
                m.plan_id,
                m.title,
                m.day,
                m.criteria,
                m.status,
                m.result,
                m.checked_at,
                m.sort
            ],
        )?;
        Ok(db.last_insert_rowid())
    }
}

/// 节点达标了没有（她问的时候、用户在界面上点的时候）
pub fn check_milestone(
    db: &Connection,
    id: i64,
    status: &str,
    result: Option<&str>,
    now: i64,
) -> Result<Milestone, String> {
    let mut m = get_milestone(db, id)
        .map_err(|e| e.to_string())?
        .ok_or("这个节点已经不在了")?;
    if !matches!(status, "todo" | "passed" | "missed" | "skipped") {
        return Err("不认识的状态".into());
    }
    m.status = status.into();
    m.checked_at = (status != "todo").then_some(now);
    if let Some(r) = result {
        m.result = clip(r, 1000);
    }
    save_milestone(db, &m).map_err(|e| e.to_string())?;
    Ok(m)
}

/// 读出整个计划书
pub fn load_full(db: &Connection, id: i64) -> Option<GoalFull> {
    let goal = get_goal(db, id).ok().flatten()?;
    let tracks = goal_tracks(db, id)
        .into_iter()
        .map(|plan| Track {
            items: list_items(db, plan.id).unwrap_or_default(),
            plan,
        })
        .collect();
    Some(GoalFull {
        tracks,
        phases: list_phases(db, id),
        milestones: list_milestones(db, id),
        goal,
    })
}

/// 保存整个计划书（已经 sanitize 过）。临时 id（负数、别的计划书的）换成新建的真 id；列表里没有的线、阶段、节点删掉。
/// `with_items` = false 时不动内容条目（只改了阶段、节点、参数）。返回计划书的 id。
pub fn apply_full(
    db: &Connection,
    full: &GoalFull,
    with_items: bool,
    now: i64,
) -> rusqlite::Result<i64> {
    let mut g = full.goal.clone();
    if g.id > 0 && get_goal(db, g.id)?.is_none() {
        g.id = 0;
    }
    let gid = save_goal(db, &g, now)?;
    g.id = gid;
    // 线
    let old_tracks: HashSet<i64> = goal_tracks(db, gid).iter().map(|p| p.id).collect();
    let mut tmap: HashMap<i64, i64> = HashMap::new();
    for (i, t) in full.tracks.iter().enumerate() {
        let mut p = t.plan.clone();
        let old = p.id;
        if !old_tracks.contains(&p.id) {
            p.id = 0;
        } else if let Some(o) = get_plan(db, p.id)? {
            p.created_at = o.created_at;
        }
        p.goal_id = Some(gid);
        p.sort = i as i64;
        let id = save_plan(db, &p, now)?;
        tmap.insert(old, id);
    }
    for id in &old_tracks {
        if !tmap.values().any(|v| v == id) {
            delete_plan(db, *id)?;
        }
    }
    let track = |x: Option<i64>| x.and_then(|i| tmap.get(&i).copied());
    // 阶段
    let old_phases: HashSet<i64> = list_phases(db, gid).iter().map(|p| p.id).collect();
    let mut pmap: HashMap<i64, i64> = HashMap::new();
    for (i, ph) in full.phases.iter().enumerate() {
        let mut p = ph.clone();
        let old = p.id;
        if !old_phases.contains(&p.id) {
            p.id = 0;
        }
        p.goal_id = gid;
        p.sort = i as i64;
        p.shares = ph
            .shares
            .iter()
            .filter_map(|(k, v)| tmap.get(k).map(|n| (*n, *v)))
            .collect();
        pmap.insert(old, save_phase(db, &p)?);
    }
    for id in &old_phases {
        if !pmap.values().any(|v| v == id) {
            db.execute("DELETE FROM goal_phases WHERE id = ?1", [id])?;
        }
    }
    let phase = |x: Option<i64>| x.and_then(|i| pmap.get(&i).copied());
    // 内容
    if with_items {
        for t in &full.tracks {
            let Some(pid) = tmap.get(&t.plan.id).copied() else {
                continue;
            };
            let items: Vec<Item> = t
                .items
                .iter()
                .map(|it| Item {
                    phase_id: phase(it.phase_id),
                    ..it.clone()
                })
                .collect();
            set_items(db, pid, &items)?;
        }
    }
    // 节点
    let old_ms: HashSet<i64> = list_milestones(db, gid).iter().map(|m| m.id).collect();
    let mut kept = HashSet::new();
    for (i, m) in full.milestones.iter().enumerate() {
        let mut m = m.clone();
        if !old_ms.contains(&m.id) {
            m.id = 0;
        }
        m.goal_id = gid;
        m.sort = i as i64;
        m.phase_id = phase(m.phase_id);
        m.plan_id = track(m.plan_id);
        kept.insert(save_milestone(db, &m)?);
    }
    for id in old_ms.difference(&kept) {
        db.execute("DELETE FROM goal_milestones WHERE id = ?1", [id])?;
    }
    // 资源里的引用换成真 id
    g.resources = g
        .resources
        .iter()
        .map(|r| Resource {
            plan_id: track(r.plan_id),
            phase_id: phase(r.phase_id),
            ..r.clone()
        })
        .collect();
    db.execute(
        "UPDATE goals SET resources = ?2 WHERE id = ?1",
        params![gid, json(&g.resources)],
    )?;
    Ok(gid)
}

// ---------------- 排程 ----------------

pub fn shape(g: &Goal, phases: &[Phase]) -> GoalShape {
    let start = parse_day(&g.start_day).unwrap_or_else(today);
    let mut ps: Vec<PhaseShape> = phases
        .iter()
        .filter_map(|p| {
            let (s, e) = p.range()?;
            Some(PhaseShape {
                id: p.id,
                start: s,
                end: e,
                week: p.week_minutes,
                shares: p.shares.iter().map(|(k, v)| (*k, *v)).collect(),
            })
        })
        .collect();
    ps.sort_by_key(|p| p.start);
    GoalShape {
        start,
        end: parse_day(&g.end_day),
        week: g.week_minutes,
        phases: ps,
        buffers: g
            .buffers
            .iter()
            .filter_map(|b| {
                Some(BufferShape {
                    start: parse_day(&b.start)?,
                    end: parse_day(&b.end)?,
                    percent: b.percent,
                })
            })
            .collect(),
        off: g.off_days.iter().filter_map(|d| parse_day(d)).collect(),
    }
}

/// 条目在排程里的样子：阶段的开始 / 结束也算进「最早开始」和截止
fn lane_item(
    it: &Item,
    left: schedule::Left,
    phases: &HashMap<i64, (NaiveDate, NaiveDate)>,
) -> LaneItem {
    let (after, due, pin) = it.dates();
    let ph = it.phase_id.and_then(|p| phases.get(&p)).copied();
    LaneItem {
        left,
        after: match (after, ph) {
            (Some(a), Some((s, _))) => Some(a.max(s)),
            (a, p) => a.or(p.map(|x| x.0)),
        },
        due: match (due, ph) {
            (Some(d), Some((_, e))) => Some(d.min(e)),
            (d, p) => d.or(p.map(|x| x.1)),
        },
        pin,
    }
}

fn phase_ranges(phases: &[Phase]) -> HashMap<i64, (NaiveDate, NaiveDate)> {
    phases
        .iter()
        .filter_map(|p| Some((p.id, p.range()?)))
        .collect()
}

/// 每周固定的事在排程器里的形状（加上计划书的请假日）
fn routine_shape(plan: &Plan, g: &GoalShape) -> Shape {
    let mut s = plan.shape();
    s.off.extend(g.off.iter().copied());
    s
}

/// 从数据库建排程器的输入：`from` 起落库的安排、还剩的内容、这一段已经分到的时间
fn db_lanes(
    db: &Connection,
    g: &Goal,
    shape: &GoalShape,
    phases: &[Phase],
    from: NaiveDate,
) -> Vec<Lane> {
    let ranges = phase_ranges(phases);
    let seg = day_str(shape.segment_start(from));
    let mut alloc: HashMap<i64, u64> = HashMap::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT plan_id, COALESCE(SUM(minutes), 0) FROM plan_tasks
         WHERE plan_id IN (SELECT id FROM plans WHERE goal_id = ?1) AND day >= ?2 AND day < ?3 AND status != 'skipped'
         GROUP BY plan_id",
    ) && let Ok(rows) = stmt.query_map(params![g.id, seg, day_str(from)], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    }) {
        for (p, m) in rows.flatten() {
            alloc.insert(p, m.max(0) as u64);
        }
    }
    let mut out = Vec::new();
    for p in goal_tracks(db, g.id) {
        if p.status != "active" {
            continue;
        }
        let items = list_items(db, p.id).unwrap_or_default();
        let (work, used) = work_for(db, &p, &items, from);
        let by_id: HashMap<i64, &Item> = items.iter().map(|i| (i.id, i)).collect();
        let work = match work {
            schedule::Work::Content(left) => LaneWork::Content(
                left.into_iter()
                    .filter_map(|l| by_id.get(&l.item_id).map(|it| lane_item(it, l, &ranges)))
                    .collect(),
            ),
            schedule::Work::Habit { title, items, rot } => LaneWork::Routine {
                shape: routine_shape(&p, shape),
                title,
                items,
                rot,
            },
        };
        out.push(Lane {
            id: p.id,
            share: p.share,
            work,
            alloc: alloc.get(&p.id).copied().unwrap_or(0),
            used,
        });
    }
    out
}

/// 草稿的排程器输入（不碰数据库；`spent` 是已经保存过的条目做了多少）
fn draft_lanes(full: &GoalFull, shape: &GoalShape, spent: &HashMap<i64, u32>) -> Vec<Lane> {
    let ranges = phase_ranges(&full.phases);
    full.tracks
        .iter()
        .filter(|t| t.plan.status == "active")
        .map(|t| {
            let todo = t.items.iter().filter(|i| i.status == "todo");
            let work = match t.plan.mode {
                Mode::Content => LaneWork::Content(
                    todo.map(|i| {
                        let sp = spent.get(&i.id).copied().unwrap_or(0);
                        lane_item(
                            i,
                            schedule::Left {
                                item_id: i.id,
                                title: i.title.clone(),
                                minutes: item_left(i, sp),
                                started: sp > 0,
                            },
                            &ranges,
                        )
                    })
                    .collect(),
                ),
                Mode::Habit => LaneWork::Routine {
                    shape: routine_shape(&t.plan, shape),
                    title: t.plan.title.clone(),
                    items: todo.map(|i| (i.id, i.title.clone(), i.minutes)).collect(),
                    rot: 0,
                },
            };
            Lane {
                id: t.plan.id,
                share: t.plan.share,
                work,
                alloc: 0,
                used: Used::new(),
            }
        })
        .collect()
}

/// 推算要看多少天：至少到计划书结束后两个月
fn horizon_for(g: &Goal, from: NaiveDate, min: u32) -> u32 {
    let to_end = parse_day(&g.end_day)
        .map(|e| (e - from).num_days().max(0) as u32 + 60)
        .unwrap_or(120);
    min.max(to_end).min(schedule::MAX_HORIZON)
}

/// 往后的推算（从明天开始，假设今天排好的会做完）
pub fn projection(
    db: &Connection,
    g: &Goal,
    day: NaiveDate,
    min_horizon: u32,
) -> lanes::GoalProjection {
    if g.status != "active" {
        return lanes::GoalProjection::default();
    }
    let phases = list_phases(db, g.id);
    let shape = shape(g, &phases);
    let tomorrow = day + Duration::days(1);
    let mut ls = db_lanes(db, g, &shape, &phases, tomorrow);
    // 今天还没做的也占着内容：从明天算起时把它们从内容里扣掉
    let d = day_str(day);
    for l in ls.iter_mut() {
        if let LaneWork::Content(items) = &mut l.work {
            for t in plan_tasks(db, l.id, &d, &d) {
                if t.status == "todo"
                    && let Some(iid) = t.item_id
                    && let Some(x) = items.iter_mut().find(|x| x.left.item_id == iid)
                {
                    x.left.minutes = x.left.minutes.saturating_sub(t.minutes);
                    x.left.started = true;
                }
            }
        }
    }
    lanes::project(&shape, &ls, tomorrow, horizon_for(g, tomorrow, min_horizon))
}

/// 排今天：删掉排程器排的、还没动过的，（先把快排到的粗内容拆开）再按现在的内容排一次
pub fn arrange_today(
    db: &Connection,
    g: &Goal,
    day: NaiveDate,
    now: i64,
) -> rusqlite::Result<usize> {
    let d = day_str(day);
    db.execute(
        "DELETE FROM plan_tasks WHERE plan_id IN (SELECT id FROM plans WHERE goal_id = ?1) AND day = ?2
         AND source = 'auto' AND status = 'todo'",
        params![g.id, d],
    )?;
    set_goal_mark(db, g.id, "arranged_day", &d)?;
    db.execute(
        "UPDATE plans SET arranged_day = ?2 WHERE goal_id = ?1",
        params![g.id, d],
    )?;
    if g.status != "active" {
        return Ok(0);
    }
    if let Err(e) = auto_expand(db, g, day) {
        log::warn!("拆开计划书的粗内容失败（#{}）：{e}", g.id);
    }
    let phases = list_phases(db, g.id);
    let shape = shape(g, &phases);
    let ls = db_lanes(db, g, &shape, &phases, day);
    let p = lanes::project(&shape, &ls, day, 1);
    let Some((first, slots)) = p.days.first() else {
        return Ok(0);
    };
    if *first != day {
        return Ok(0);
    }
    let tracks: HashMap<i64, Plan> = goal_tracks(db, g.id)
        .into_iter()
        .map(|p| (p.id, p))
        .collect();
    let base_sort: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(sort), 0) + 1 FROM plan_tasks WHERE day = ?1",
            [&d],
            |r| r.get(0),
        )
        .unwrap_or(1);
    for (i, (lane, s)) in slots.iter().enumerate() {
        let start = tracks
            .get(lane)
            .map(|p| p.remind_at.clone())
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| g.remind_at.clone());
        save_task(
            db,
            &Task {
                plan_id: Some(*lane),
                item_id: s.item_id,
                day: d.clone(),
                title: slot_title(s),
                minutes: s.minutes,
                start,
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

/// 用户改了计划书之后重排今天（开始时间已经过了的不再「到点提醒」，免得刚保存就被叫）
pub fn rearrange_today(
    db: &Connection,
    goal_id: i64,
    day: NaiveDate,
    now: i64,
) -> rusqlite::Result<usize> {
    let Some(g) = get_goal(db, goal_id)? else {
        return Ok(0);
    };
    let n = arrange_today(db, &g, day, now)?;
    let ids: HashSet<i64> = goal_tracks(db, goal_id).iter().map(|p| p.id).collect();
    for t in day_tasks(db, &day_str(day)) {
        if t.source == "auto"
            && t.status == "todo"
            && !t.reminded
            && t.plan_id.is_some_and(|p| ids.contains(&p))
            && start_passed(&t.start, now)
        {
            db.execute("UPDATE plan_tasks SET reminded = 1 WHERE id = ?1", [t.id])?;
        }
    }
    Ok(n)
}

/// 今天第一次用到时：每个进行中的计划书排好今天
pub fn ensure_day(db: &Connection, day: NaiveDate, now: i64) {
    let d = day_str(day);
    for g in list_goals(db).unwrap_or_default() {
        if g.status != "active" || g.arranged_day == d {
            continue;
        }
        if let Err(e) = arrange_today(db, &g, day, now) {
            log::warn!("排今天的计划书失败（#{}）：{e}", g.id);
        }
    }
}

/// 所有内容线都做完了：计划书标完成
pub fn check_done(db: &Connection, goal_id: i64, now: i64) -> bool {
    let tracks = goal_tracks(db, goal_id);
    let content: Vec<&Plan> = tracks.iter().filter(|p| p.mode == Mode::Content).collect();
    if !content.is_empty() && content.iter().all(|p| p.status == "done") {
        let _ = db.execute(
            "UPDATE goals SET status = 'done', updated_at = ?2 WHERE id = ?1 AND status = 'active'",
            params![goal_id, now],
        );
        return true;
    }
    false
}

// ---------------- 粗内容本地拆开 ----------------

/// 说明里列的几项（「推论、集合、损益算」「言语；非言语」）
pub fn topics(detail: &str) -> Vec<String> {
    let parts: Vec<String> = detail
        .split(['、', '，', ',', '；', ';', '/', '／', '\n'])
        .map(|s| {
            s.trim()
                .trim_matches(|c: char| matches!(c, '。' | '.' | '：' | ':' | '-' | '·' | '•'))
                .trim()
                .chars()
                .take(60)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .collect();
    if (2..=12).contains(&parts.len()) {
        parts
    } else {
        Vec::new()
    }
}

/// 把一条粗内容按说明里列的几项拆开（拆不开 = None）。已经做过一部分的：原来那条留下做过的部分并标完成。
pub fn split_rough(it: &Item, spent: u32) -> Option<Vec<Item>> {
    if !it.rough || it.status != "todo" {
        return None;
    }
    let ts = topics(&it.detail);
    if ts.is_empty() {
        return None;
    }
    let left = it.minutes.saturating_sub(spent);
    let n = ts.len() as u32;
    if left < 15 * n {
        return None;
    }
    let each = (left / n) / 5 * 5;
    let mut out = Vec::new();
    if spent > 0 {
        out.push(Item {
            minutes: spent,
            status: "done".into(),
            rough: false,
            ..it.clone()
        });
    }
    for (k, t) in ts.into_iter().enumerate() {
        let minutes = if k as u32 == n - 1 {
            left - each * (n - 1)
        } else {
            each
        };
        out.push(Item {
            id: 0,
            title: format!("{}：{t}", it.title).chars().take(100).collect(),
            detail: String::new(),
            minutes,
            rough: false,
            done_at: None,
            ..it.clone()
        });
    }
    Some(out)
}

/// 内容改过了：下次排今天时重新检查要不要拆粗内容
pub fn reset_expanded(db: &Connection, id: i64) -> rusqlite::Result<()> {
    set_goal_mark(db, id, "expanded_until", "")
}

/// 两周之内要排到的粗内容：本地拆开（不调模型）。一周最多检查一次（`expanded_until`）
pub fn auto_expand(db: &Connection, g: &Goal, day: NaiveDate) -> rusqlite::Result<usize> {
    if parse_day(&g.expanded_until).is_some_and(|u| u >= day + Duration::days(7)) {
        return Ok(0);
    }
    let until = day + Duration::days(EXPAND_DAYS);
    let mut n = 0;
    for (plan_id, ids) in soon_items(db, g, day, until, true) {
        let ids: HashSet<i64> = ids.into_keys().collect();
        let items = list_items(db, plan_id)?;
        let spent = item_spent(db, plan_id);
        // 以后的日子里落了库还没做的（挪过去的）占着这条：不拆
        let busy: HashSet<i64> = plan_tasks(db, plan_id, &day_str(day), "9999-12-31")
            .into_iter()
            .filter(|t| t.status == "todo")
            .filter_map(|t| t.item_id)
            .collect();
        let mut changed = false;
        let mut next = Vec::new();
        for it in items {
            if ids.contains(&it.id)
                && !busy.contains(&it.id)
                && let Some(mut parts) = split_rough(&it, spent.get(&it.id).copied().unwrap_or(0))
            {
                // 做过的那部分沿用原来的 id（做过的记录还连着它）；没做过的原来那条换成拆开的
                if parts[0].status == "done" {
                    parts[0].id = it.id;
                }
                next.extend(parts);
                changed = true;
                n += 1;
            } else {
                next.push(it);
            }
        }
        if changed {
            set_items(db, plan_id, &next)?;
        }
    }
    set_goal_mark(db, g.id, "expanded_until", &day_str(until))?;
    if n > 0 {
        log::info!("计划书 #{}：本地拆开了 {n} 条粗内容", g.id);
    }
    Ok(n)
}

/// 线 → （粗内容的条目 id → 这几天里排到它的分钟）
pub type Soon = BTreeMap<i64, HashMap<i64, u32>>;

/// 推算里 [.., until) 排到的粗内容
pub fn soon_in(proj: &lanes::GoalProjection, rough: &HashSet<i64>, until: NaiveDate) -> Soon {
    let mut out = Soon::new();
    for (d, slots) in &proj.days {
        if *d >= until {
            break;
        }
        for (lane, s) in slots {
            if let Some(i) = s.item_id.filter(|i| rough.contains(i)) {
                *out.entry(*lane).or_default().entry(i).or_default() += s.minutes;
            }
        }
    }
    out
}

/// 在 [day, until) 里会排到的内容（从数据库；`only_rough` = 只要粗的）
pub fn soon_items(
    db: &Connection,
    g: &Goal,
    day: NaiveDate,
    until: NaiveDate,
    only_rough: bool,
) -> Soon {
    let rough: HashSet<i64> = goal_tracks(db, g.id)
        .iter()
        .flat_map(|p| list_items(db, p.id).unwrap_or_default())
        .filter(|i| (i.rough || !only_rough) && i.status == "todo")
        .map(|i| i.id)
        .collect();
    if rough.is_empty() {
        return Soon::new();
    }
    let phases = list_phases(db, g.id);
    let shape = shape(g, &phases);
    let ls = db_lanes(db, g, &shape, &phases, day);
    let days = (until - day).num_days().max(1) as u32;
    soon_in(&lanes::project(&shape, &ls, day, days), &rough, until)
}

/// 草稿里 [开始, 开始 + days) 会排到的粗内容
pub fn draft_soon(full: &GoalFull, today: NaiveDate, days: i64) -> Soon {
    let rough: HashSet<i64> = full
        .tracks
        .iter()
        .flat_map(|t| t.items.iter())
        .filter(|i| i.rough && i.status == "todo")
        .map(|i| i.id)
        .collect();
    let shape = shape(&full.goal, &full.phases);
    let from = shape.start.max(today);
    let proj = lanes::project(
        &shape,
        &draft_lanes(full, &shape, &HashMap::new()),
        from,
        days as u32,
    );
    soon_in(&proj, &rough, from + Duration::days(days))
}

// ---------------- 给界面看的 ----------------

/// 某条线在某个阶段的量
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct Cell {
    pub plan_id: i64,
    /// 这个阶段的份额（阶段没写 = 线的默认份额）
    pub share: u32,
    pub total: u32,
    pub done: u32,
    pub items: u32,
    pub items_done: u32,
    /// 还没细化的粗内容有几条
    pub rough: u32,
    /// 预计哪天做完（空 = 推算范围内做不完 / 没有要做的）
    pub finish: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhaseView {
    pub phase: Phase,
    /// past / current / future
    pub state: String,
    pub cells: Vec<Cell>,
    pub total: u32,
    pub done: u32,
    pub percent: u32,
    pub finish: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemViz {
    pub id: i64,
    pub phase_id: Option<i64>,
    pub title: String,
    pub minutes: u32,
    pub spent: u32,
    pub status: String,
    pub rough: bool,
    pub pin_day: String,
    pub due_day: String,
    /// 预计哪天做完
    pub finish: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackViz {
    pub plan_id: i64,
    pub routine: bool,
    pub total: u32,
    pub done: u32,
    pub percent: u32,
    pub finish: String,
    /// 不属于任何阶段的内容（量）
    pub loose: Cell,
    pub items: Vec<ItemViz>,
}

/// 某一天排的：（线、标题、分钟）
pub type DaySlots = Vec<(i64, String, u32)>;

/// 可视化要的数据（保存了的计划书和草稿都用这个）
#[derive(Debug, Clone, Serialize)]
pub struct Viz {
    pub phases: Vec<PhaseView>,
    pub tracks: Vec<TrackViz>,
    pub short: Vec<LaneShort>,
    /// 钉在已经过去的日子、还没做的：（线，条目）
    pub missed: Vec<(i64, i64)>,
    pub total: u32,
    pub done: u32,
    pub percent: u32,
    /// 预计全部做完的日子
    pub finish: String,
    pub current_phase: Option<i64>,
    /// 往后两周每天排什么：（日期，[（线、标题、分钟）]）
    pub days: Vec<(String, DaySlots)>,
}

fn pct(done: u32, total: u32) -> u32 {
    if total == 0 {
        0
    } else {
        (done as u64 * 100 / total as u64).min(100) as u32
    }
}

fn item_done(it: &Item, spent: &HashMap<i64, u32>) -> u32 {
    match it.status.as_str() {
        "done" => it.minutes,
        "todo" => spent.get(&it.id).copied().unwrap_or(0).min(it.minutes),
        _ => 0,
    }
}

fn cell_of(
    t: &Track,
    phase: Option<i64>,
    share: u32,
    spent: &HashMap<i64, u32>,
    proj: &lanes::GoalProjection,
) -> Cell {
    let mut c = Cell {
        plan_id: t.plan.id,
        share,
        ..Default::default()
    };
    let mut last: Option<NaiveDate> = None;
    let mut open = false;
    for it in t
        .items
        .iter()
        .filter(|i| i.phase_id == phase && i.status != "skipped")
    {
        c.items += 1;
        c.total += it.minutes;
        c.done += item_done(it, spent);
        if it.status == "done" {
            c.items_done += 1;
            continue;
        }
        if it.rough {
            c.rough += 1;
        }
        match proj.item_finish.get(&it.id) {
            Some(f) => last = Some(last.map_or(*f, |l| l.max(*f))),
            None => open = true,
        }
    }
    if !open {
        c.finish = last.map(day_str).unwrap_or_default();
    }
    c
}

/// 算可视化的数据（纯函数）：`spent` 每条内容已经做了多少分钟，`proj` 往后的推算
pub fn viz(
    full: &GoalFull,
    spent: &HashMap<i64, u32>,
    proj: &lanes::GoalProjection,
    today: NaiveDate,
) -> Viz {
    let phases: Vec<PhaseView> = full
        .phases
        .iter()
        .map(|p| {
            let cells: Vec<Cell> = full
                .tracks
                .iter()
                .filter(|t| t.plan.mode == Mode::Content)
                .map(|t| {
                    let share = if p.shares.is_empty() {
                        t.plan.share
                    } else {
                        p.shares.get(&t.plan.id).copied().unwrap_or(0)
                    };
                    cell_of(t, Some(p.id), share, spent, proj)
                })
                .collect();
            let (s, e) = p.range().unwrap_or((today, today));
            let total = cells.iter().map(|c| c.total).sum();
            let done = cells.iter().map(|c| c.done).sum();
            let finish = if cells
                .iter()
                .any(|c| c.items > c.items_done && c.finish.is_empty())
            {
                String::new()
            } else {
                cells
                    .iter()
                    .map(|c| c.finish.clone())
                    .max()
                    .unwrap_or_default()
            };
            PhaseView {
                state: if e < today {
                    "past"
                } else if s <= today {
                    "current"
                } else {
                    "future"
                }
                .into(),
                total,
                done,
                percent: pct(done, total),
                finish,
                cells,
                phase: p.clone(),
            }
        })
        .collect();
    let tracks: Vec<TrackViz> = full
        .tracks
        .iter()
        .map(|t| {
            let routine = t.plan.mode == Mode::Habit;
            let (total, done) = if routine {
                (0, 0)
            } else {
                t.items
                    .iter()
                    .filter(|i| i.status != "skipped")
                    .fold((0, 0), |(a, b), it| {
                        (a + it.minutes, b + item_done(it, spent))
                    })
            };
            TrackViz {
                plan_id: t.plan.id,
                routine,
                total,
                done,
                percent: pct(done, total),
                finish: proj
                    .finish
                    .get(&t.plan.id)
                    .map(|d| day_str(*d))
                    .unwrap_or_default(),
                loose: cell_of(t, None, t.plan.share, spent, proj),
                items: t
                    .items
                    .iter()
                    .map(|it| ItemViz {
                        id: it.id,
                        phase_id: it.phase_id,
                        title: it.title.clone(),
                        minutes: it.minutes,
                        spent: spent.get(&it.id).copied().unwrap_or(0),
                        status: it.status.clone(),
                        rough: it.rough,
                        pin_day: it.pin_day.clone(),
                        due_day: it.due_day.clone(),
                        finish: proj
                            .item_finish
                            .get(&it.id)
                            .map(|d| day_str(*d))
                            .unwrap_or_default(),
                    })
                    .collect(),
            }
        })
        .collect();
    let total: u32 = tracks.iter().map(|t| t.total).sum();
    let done: u32 = tracks.iter().map(|t| t.done).sum();
    let open: Vec<&TrackViz> = tracks
        .iter()
        .filter(|t| !t.routine && t.total > t.done)
        .collect();
    let finish = if open.iter().any(|t| t.finish.is_empty()) {
        String::new()
    } else {
        open.iter()
            .map(|t| t.finish.clone())
            .max()
            .unwrap_or_default()
    };
    let days = proj
        .days
        .iter()
        .filter(|(d, _)| *d < today + Duration::days(15))
        .map(|(d, v)| {
            (
                day_str(*d),
                v.iter()
                    .map(|(l, s)| (*l, slot_title(s), s.minutes))
                    .collect(),
            )
        })
        .collect();
    Viz {
        current_phase: phases
            .iter()
            .find(|p| p.state == "current")
            .map(|p| p.phase.id),
        phases,
        tracks,
        short: proj.short.clone(),
        missed: proj.missed.clone(),
        total,
        done,
        percent: pct(done, total),
        finish,
        days,
    }
}

/// 草稿的推算（不碰数据库；`spent`：改的是已经保存过的计划书时，各条内容已经做了多少）
pub fn draft_viz(full: &GoalFull, today: NaiveDate, spent: &HashMap<i64, u32>) -> Viz {
    let shape = shape(&full.goal, &full.phases);
    let from = shape.start.max(today);
    let ls = draft_lanes(full, &shape, spent);
    let proj = lanes::project(&shape, &ls, from, horizon_for(&full.goal, from, 120));
    viz(full, spent, &proj, today)
}

/// 已经保存过的计划书每条内容做了多少
pub fn goal_spent(db: &Connection, full: &GoalFull) -> HashMap<i64, u32> {
    let mut spent = HashMap::new();
    for t in full.tracks.iter().filter(|t| t.plan.id > 0) {
        spent.extend(item_spent(db, t.plan.id));
    }
    spent
}

/// 某条线这周（或某几天）的完成情况
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct TrackWeek {
    pub plan_id: i64,
    pub planned: u32,
    pub done: u32,
    pub tasks: u32,
    pub tasks_done: u32,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct WeekStat {
    pub from: String,
    pub to: String,
    pub planned: u32,
    pub done: u32,
    pub tasks: u32,
    pub tasks_done: u32,
    pub tracks: Vec<TrackWeek>,
}

/// [from, to] 里每条线安排了多少、做了多少
pub fn week_stat(db: &Connection, goal_id: i64, from: NaiveDate, to: NaiveDate) -> WeekStat {
    let mut w = WeekStat {
        from: day_str(from),
        to: day_str(to),
        ..Default::default()
    };
    let mut by: HashMap<i64, TrackWeek> = HashMap::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT plan_id, COALESCE(SUM(CASE WHEN status != 'skipped' THEN minutes ELSE 0 END), 0),
         COALESCE(SUM(CASE WHEN status = 'done' THEN spent ELSE 0 END), 0), COUNT(*), COALESCE(SUM(status = 'done'), 0)
         FROM plan_tasks WHERE plan_id IN (SELECT id FROM plans WHERE goal_id = ?1) AND day >= ?2 AND day <= ?3
         GROUP BY plan_id",
    ) && let Ok(rows) = stmt.query_map(params![goal_id, w.from, w.to], |r| {
        Ok(TrackWeek {
            plan_id: r.get(0)?,
            planned: r.get::<_, i64>(1)?.max(0) as u32,
            done: r.get::<_, i64>(2)?.max(0) as u32,
            tasks: r.get::<_, i64>(3)?.max(0) as u32,
            tasks_done: r.get::<_, i64>(4)?.max(0) as u32,
        })
    }) {
        for t in rows.flatten() {
            by.insert(t.plan_id, t);
        }
    }
    for p in goal_tracks(db, goal_id) {
        let t = by.remove(&p.id).unwrap_or(TrackWeek {
            plan_id: p.id,
            ..Default::default()
        });
        w.planned += t.planned;
        w.done += t.done;
        w.tasks += t.tasks;
        w.tasks_done += t.tasks_done;
        w.tracks.push(t);
    }
    w
}

/// 这一周（周一到周日）
pub fn week_of(day: NaiveDate) -> (NaiveDate, NaiveDate) {
    let mon = day - Duration::days(weekday_index(day) as i64);
    (mon, mon + Duration::days(6))
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalView {
    pub full: GoalFull,
    pub viz: Viz,
    /// 这周的完成情况
    pub week: WeekStat,
    /// 各条线的备注（新的在前）
    pub notes: Vec<Note>,
}

pub fn goal_view(db: &Connection, id: i64, day: NaiveDate) -> Option<GoalView> {
    let full = load_full(db, id)?;
    let mut spent = HashMap::new();
    for t in &full.tracks {
        spent.extend(item_spent(db, t.plan.id));
    }
    let proj = projection(db, &full.goal, day, 120);
    let mut v = viz(&full, &spent, &proj, day);
    // 今天已经排好的也放进「往后两周」的开头
    let d = day_str(day);
    let today_slots: DaySlots = full
        .tracks
        .iter()
        .flat_map(|t| plan_tasks(db, t.plan.id, &d, &d))
        .filter(|t| t.status == "todo")
        .map(|t| (t.plan_id.unwrap_or(0), t.title, t.minutes))
        .collect();
    if !today_slots.is_empty() {
        v.days.insert(0, (d, today_slots));
    }
    let (mon, sun) = week_of(day);
    let mut notes = Vec::new();
    for t in &full.tracks {
        notes.extend(plan_notes(db, t.plan.id, 50));
    }
    notes.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    notes.truncate(100);
    Some(GoalView {
        week: week_stat(db, id, mon, sun),
        viz: v,
        full,
        notes,
    })
}

/// 左边列表里的一个计划书
#[derive(Debug, Clone, Serialize)]
pub struct GoalSummary {
    pub goal: Goal,
    pub percent: u32,
    pub tracks: Vec<i64>,
    /// 现在是哪个阶段
    pub phase: String,
    /// 下一个节点：（名字、日期）
    pub next: Option<(String, String)>,
    /// 有线来不及
    pub short: bool,
}

pub fn summaries(db: &Connection, day: NaiveDate) -> Vec<GoalSummary> {
    let d = day_str(day);
    list_goals(db)
        .unwrap_or_default()
        .into_iter()
        .map(|g| {
            let tracks = goal_tracks(db, g.id);
            let (mut total, mut done) = (0u32, 0u32);
            for p in tracks.iter().filter(|p| p.mode == Mode::Content) {
                let spent = item_spent(db, p.id);
                for it in list_items(db, p.id).unwrap_or_default() {
                    if it.status != "skipped" {
                        total += it.minutes;
                        done += item_done(&it, &spent);
                    }
                }
            }
            let phase = list_phases(db, g.id)
                .into_iter()
                .find(|p| p.start_day <= d && d <= p.end_day)
                .map(|p| p.title)
                .unwrap_or_default();
            let next = list_milestones(db, g.id)
                .into_iter()
                .find(|m| m.status == "todo" && m.day >= d)
                .map(|m| (m.title, m.day));
            let short = g.status == "active" && !projection(db, &g, day, 30).short.is_empty();
            GoalSummary {
                percent: pct(done, total),
                tracks: tracks.iter().map(|p| p.id).collect(),
                phase,
                next,
                short,
                goal: g,
            }
        })
        .collect()
}

/// 计划的名字：计划书里的线带上计划书的名字（「🎯 就业准备·笔试」）
pub fn plan_labels(db: &Connection, plans: &[Plan]) -> HashMap<i64, String> {
    let goals: HashMap<i64, Goal> = list_goals(db)
        .unwrap_or_default()
        .into_iter()
        .map(|g| (g.id, g))
        .collect();
    plans
        .iter()
        .map(|p| {
            let label = match p.goal_id.and_then(|g| goals.get(&g)) {
                Some(g) => {
                    let e = if p.emoji.is_empty() {
                        &g.emoji
                    } else {
                        &p.emoji
                    };
                    if e.is_empty() {
                        format!("{}·{}", g.title, p.title)
                    } else {
                        format!("{e} {}·{}", g.title, p.title)
                    }
                }
                None => p.label(),
            };
            (p.id, label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const NOW: i64 = 1_800_000_000;

    fn d(m: u32, day: u32) -> NaiveDate {
        let y = if m >= 9 { 2026 } else { 2027 };
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn content(id: i64, title: &str, items: Vec<Item>) -> Track {
        Track {
            plan: Plan {
                id,
                title: title.into(),
                ..Default::default()
            },
            items,
        }
    }

    fn item(id: i64, title: &str, minutes: u32, phase: i64) -> Item {
        Item {
            id,
            title: title.into(),
            minutes,
            phase_id: Some(phase),
            ..Default::default()
        }
    }

    /// 两条内容线 + 一条每周三的固定小事、两个阶段、一个节点（都是临时 id）
    fn draft() -> GoalFull {
        let mut routine = content(-3, "联系老师", Vec::new());
        routine.plan.mode = Mode::Habit;
        routine.plan.week_minutes = [0, 0, 30, 0, 0, 0, 0];
        routine.plan.end_day = "2026-12-31".into();
        GoalFull {
            goal: Goal {
                title: "考证".into(),
                emoji: "🎯".into(),
                start_day: "2026-11-09".into(),
                end_day: "2027-01-31".into(),
                week_minutes: [60; 7],
                remind_at: "20:00".into(),
                ..Default::default()
            },
            tracks: vec![
                content(
                    -1,
                    "专业课",
                    vec![
                        item(-11, "概念一", 120, -101),
                        Item {
                            rough: true,
                            detail: "概念二、概念三".into(),
                            ..item(-12, "后面的概念", 600, -102)
                        },
                    ],
                ),
                content(-2, "真题", vec![item(-21, "第一套", 90, -101)]),
                routine,
            ],
            phases: vec![
                Phase {
                    id: -101,
                    title: "打基础".into(),
                    start_day: "2026-11-09".into(),
                    end_day: "2026-11-30".into(),
                    shares: [(-1, 60), (-2, 40)].into(),
                    ..Default::default()
                },
                Phase {
                    id: -102,
                    title: "强化".into(),
                    start_day: "2026-12-01".into(),
                    end_day: "2027-01-31".into(),
                    ..Default::default()
                },
            ],
            milestones: vec![Milestone {
                id: -5001,
                title: "讲清楚概念一".into(),
                day: "2026-11-20".into(),
                plan_id: Some(-1),
                phase_id: Some(-101),
                ..Default::default()
            }],
        }
    }

    fn saved(db: &Connection) -> GoalFull {
        let full = draft().sanitize(d(11, 9)).unwrap();
        let id = apply_full(db, &full, true, NOW).unwrap();
        load_full(db, id).unwrap()
    }

    #[test]
    fn apply_maps_temporary_ids() {
        let db = db::open_in_memory().unwrap();
        let f = saved(&db);
        assert_eq!(f.tracks.len(), 3);
        assert!(
            f.tracks
                .iter()
                .all(|t| t.plan.id > 0 && t.plan.goal_id == Some(f.goal.id))
        );
        let (a, b) = (f.tracks[0].plan.id, f.tracks[1].plan.id);
        assert_eq!(
            f.phases[0].shares,
            BTreeMap::from([(a, 60), (b, 40)]),
            "份额换成真 id"
        );
        assert_eq!(f.tracks[0].items[1].phase_id, Some(f.phases[1].id));
        assert_eq!(f.milestones[0].plan_id, Some(a));
        assert_eq!(f.milestones[0].phase_id, Some(f.phases[0].id));
        // 内容线的日期跟着计划书
        assert_eq!(f.tracks[0].plan.end_day, "2027-01-31");
        // 再保存一次（删掉一条线、改名一个阶段）：沿用 id，删掉的线连同内容一起没了
        let mut g = f.clone();
        g.tracks.remove(1);
        g.phases[0].title = "基础".into();
        let g = g.sanitize(d(11, 9)).unwrap();
        apply_full(&db, &g, false, NOW).unwrap();
        let h = load_full(&db, f.goal.id).unwrap();
        assert_eq!(h.tracks.len(), 2);
        assert_eq!(h.phases[0].id, f.phases[0].id);
        assert_eq!(
            h.phases[0].shares,
            BTreeMap::from([(a, 60)]),
            "删掉的线的份额也去掉"
        );
        assert!(get_plan(&db, b).unwrap().is_none());
        assert_eq!(h.tracks[0].items.len(), 2, "with_items = false 不动内容");
    }

    #[test]
    fn arranges_today_across_tracks() {
        let db = db::open_in_memory().unwrap();
        let f = saved(&db);
        super::super::ensure_day(&db, d(11, 9), NOW);
        let t = day_tasks(&db, "2026-11-09");
        assert_eq!(t.len(), 1, "一天 60 分钟、一块：只做一条线 {t:?}");
        assert_eq!(t[0].plan_id, Some(f.tracks[0].plan.id), "份额大的先");
        assert_eq!(t[0].start, "20:00", "用计划书的开始时间");
        // 星期三：先排固定的小事
        super::super::ensure_day(&db, d(11, 11), NOW);
        let t = day_tasks(&db, "2026-11-11");
        assert!(
            t.iter()
                .any(|x| x.plan_id == Some(f.tracks[2].plan.id) && x.minutes == 30)
        );
        assert_eq!(t.iter().map(|x| x.minutes).sum::<u32>(), 60);
        // 线的进度和推算从计划书来
        let (g, _) = plan_progress(&db, &f.tracks[1].plan, d(11, 11), 60);
        assert!(!g.finish.is_empty());
        let labels = plan_labels(&db, &[f.tracks[1].plan.clone()]);
        assert_eq!(labels[&f.tracks[1].plan.id], "🎯 考证·真题");
    }

    #[test]
    fn soon_rough_items_are_split_locally() {
        let db = db::open_in_memory().unwrap();
        let mut f = draft();
        // 粗内容放到第一阶段，马上就会排到
        f.tracks[0].items[1].phase_id = Some(-101);
        let f = f.sanitize(d(11, 9)).unwrap();
        let id = apply_full(&db, &f, true, NOW).unwrap();
        let g = get_goal(&db, id).unwrap().unwrap();
        arrange_today(&db, &g, d(11, 9), NOW).unwrap();
        let items = list_items(&db, load_full(&db, id).unwrap().tracks[0].plan.id).unwrap();
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(
            titles,
            ["概念一", "后面的概念：概念二", "后面的概念：概念三"]
        );
        assert!(items.iter().all(|i| !i.rough));
        assert_eq!(items[1].minutes + items[2].minutes, 600);
        assert_eq!(
            get_goal(&db, id).unwrap().unwrap().expanded_until,
            "2026-11-23"
        );
    }

    #[test]
    fn split_keeps_done_part() {
        let it = Item {
            id: 5,
            title: "笔试".into(),
            detail: "言语、非言语".into(),
            minutes: 300,
            rough: true,
            ..Default::default()
        };
        let v = split_rough(&it, 100).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!((v[0].status.as_str(), v[0].minutes), ("done", 100));
        assert_eq!(v[1].minutes + v[2].minutes, 200);
        assert!(
            split_rough(
                &Item {
                    detail: "没有要点".into(),
                    ..it.clone()
                },
                0
            )
            .is_none()
        );
        assert_eq!(topics("推论、集合；速度算"), ["推论", "集合", "速度算"]);
    }

    #[test]
    fn milestones_week_stats_and_status() {
        let db = db::open_in_memory().unwrap();
        let f = saved(&db);
        super::super::ensure_day(&db, d(11, 9), NOW);
        let t = day_tasks(&db, "2026-11-09");
        complete_task(&db, t[0].id, Some(50), None, NOW).unwrap();
        let w = week_stat(&db, f.goal.id, d(11, 9), d(11, 15));
        assert_eq!((w.planned, w.done, w.tasks_done), (60, 50, 1));
        let m =
            check_milestone(&db, f.milestones[0].id, "passed", Some("讲给同学听了"), NOW).unwrap();
        assert_eq!((m.status.as_str(), m.checked_at), ("passed", Some(NOW)));
        let v = goal_view(&db, f.goal.id, d(11, 9)).unwrap();
        assert_eq!(v.viz.current_phase, Some(f.phases[0].id));
        assert!(v.viz.done > 0 && v.viz.percent > 0);
        assert_eq!(v.viz.phases[0].cells.len(), 2, "只有内容线有格子");
        // 暂停：线跟着暂停
        set_goal_status(&db, f.goal.id, "paused", NOW).unwrap();
        assert!(
            goal_tracks(&db, f.goal.id)
                .iter()
                .all(|p| p.status == "paused")
        );
        rearrange_today(&db, f.goal.id, d(11, 9), NOW).unwrap();
        let s = summaries(&db, d(11, 9));
        assert_eq!(s[0].phase, "打基础");
        assert_eq!(s[0].next, None, "节点已经达标");
    }

    #[test]
    fn old_plans_survive_the_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let c = Connection::open(&path).unwrap();
            for sql in &db::MIGRATIONS[..11] {
                c.execute_batch(sql).unwrap();
            }
            c.pragma_update(None, "user_version", 11).unwrap();
            c.execute(
                "INSERT INTO plans(title, start_day, created_at, updated_at) VALUES ('读书', '2026-11-01', 1, 1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO plan_items(plan_id, title, minutes) VALUES (1, '第1章', 60)",
                [],
            )
            .unwrap();
        }
        let c = db::open(&path, &dir.path().join("b")).unwrap();
        let p = get_plan(&c, 1).unwrap().unwrap();
        assert_eq!((p.title.as_str(), p.goal_id, p.share), ("读书", None, 1));
        let it = &list_items(&c, 1).unwrap()[0];
        assert_eq!((it.minutes, it.phase_id, it.rough), (60, None, false));
    }
}
