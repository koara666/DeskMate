//! 桌宠怎么陪着做计划：
//! - 到了开始时间还没做：提醒一次（和用户设的提醒同级，勿扰挡不住；全屏时改发系统通知）。
//! - 该做计划的时候在玩游戏 / 看视频：温和地提一句（每天有上限，强度可调；勿扰、安静时段、主动说话总开关都挡得住）。
//! - 晚上还有没做完的：问一句，气泡里给选项，选了直接记下来。
//! - 专注计时：她陪着计时，计时中跑去玩游戏会被叫回来；时间到问做完没有。
//! - 勾掉一项、全部做完、整个计划完成：她说一句（`react`）。
//!
//! 判断「该不该说」的部分都是纯函数（有测试）；时间按「计划日」的分钟算（凌晨 5 点 = 0），熬夜时也不会算错。

use super::{Task, day_str, get_plan, today};
use crate::activity::classify::Category;
use crate::proactive::{self, Item, Kind};
use crate::state::AppState;
use chrono::Timelike;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

/// 监督有多「严」
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Strictness {
    /// 温和：一天最多提一次
    Gentle,
    #[default]
    Normal,
    /// 严格：提得更勤，晚上没做完还会再问一次
    Strict,
}

impl Strictness {
    /// 一天最多因为分心提醒几次
    pub fn nudges_per_day(self) -> u32 {
        match self {
            Strictness::Gentle => 1,
            Strictness::Normal => 2,
            Strictness::Strict => 4,
        }
    }
    /// 分心多久才提（分钟）
    pub fn distraction_minutes(self) -> i64 {
        match self {
            Strictness::Gentle => 25,
            Strictness::Normal => 15,
            Strictness::Strict => 8,
        }
    }
    /// 两次分心提醒至少隔多久（分钟）
    pub fn nudge_gap_minutes(self) -> i64 {
        match self {
            Strictness::Gentle => 120,
            Strictness::Normal => 45,
            Strictness::Strict => 25,
        }
    }
    /// 晚上最多问几次
    pub fn checkins(self) -> u32 {
        if self == Strictness::Strict { 2 } else { 1 }
    }
}

/// 「计划」的设置（存在设置里）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanSettings {
    /// 让她监督计划（总开关；关掉后计划照样能用，只是她不提醒）
    pub supervise: bool,
    pub strictness: Strictness,
    /// 到了开始时间还没做：提醒一次
    pub start_remind: bool,
    /// 该做计划的时候在玩游戏 / 看视频：提一句
    pub distraction: bool,
    /// 晚上问一句今天做得怎么样
    pub checkin: bool,
    /// "HH:MM"
    pub checkin_at: String,
    /// 做完一项、全部做完时她的反应
    pub celebrate: bool,
    /// 聊天时她知道今天的计划
    pub chat_context: bool,
    /// 每日播报里说今天的计划
    pub briefing: bool,
    /// 专注计时默认多少分钟
    pub focus_minutes: u32,
    /// 计划书：节点当天问达标没有
    pub milestone_check: bool,
    /// 计划书：每周自查（哪天由计划书自己定）
    pub review: bool,
    /// 节点、自查几点开始问（"HH:MM"）
    pub review_at: String,
}

impl Default for PlanSettings {
    fn default() -> Self {
        Self {
            supervise: true,
            strictness: Strictness::Normal,
            start_remind: true,
            distraction: true,
            checkin: true,
            checkin_at: "21:30".into(),
            celebrate: true,
            chat_context: true,
            briefing: true,
            focus_minutes: 25,
            milestone_check: true,
            review: true,
            review_at: "20:00".into(),
        }
    }
}

impl PlanSettings {
    pub fn sanitize(mut self) -> Self {
        if crate::proactive::parse_hm(&self.checkin_at).is_none() {
            self.checkin_at = "21:30".into();
        }
        if crate::proactive::parse_hm(&self.review_at).is_none() {
            self.review_at = "20:00".into();
        }
        self.focus_minutes = self.focus_minutes.clamp(5, 180);
        self
    }
}

/// 专注计时
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Focus {
    pub task_id: i64,
    pub title: String,
    pub started_at: i64,
    pub ends_at: i64,
    pub minutes: u32,
    /// 前几轮已经专注了多少分钟（「再来一轮」时累计）
    pub before: u32,
    #[serde(skip)]
    nudged: u32,
    #[serde(skip)]
    last_nudge: i64,
}

/// 今天的监督记录（在内存里；重启后分心提醒的次数从零算，晚上那一问记在数据库里）
#[derive(Debug, Default, Clone)]
struct DayLog {
    day: String,
    nudges: u32,
    last_nudge: i64,
    /// 「今天不做了」之后不再因为分心提醒
    quiet: bool,
    /// 上次因为计划书问（节点、自查）是什么时候：两次之间隔开一点
    goal_asked: i64,
}

/// 运行时状态
#[derive(Default)]
pub struct PlanRuntime {
    /// 计划变了：叫醒监督循环
    pub changed: Notify,
    /// 计划窗口下次打开时显示哪一页（today / new / new:<文字> / plan:<id> / settings）
    pub view: Mutex<Option<String>>,
    focus: Mutex<Option<Focus>>,
    log: Mutex<DayLog>,
    /// 监督循环上次看到的「计划日」：换天时通知窗口刷新
    seen_day: Mutex<String>,
}

impl PlanRuntime {
    pub fn focus(&self) -> Option<Focus> {
        self.focus.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn set_focus(&self, f: Option<Focus>) {
        *self.focus.lock().unwrap_or_else(|e| e.into_inner()) = f;
    }
    /// 是不是换了一天（第一次调用也算）
    fn new_day(&self, day: &str) -> bool {
        let mut g = self.seen_day.lock().unwrap_or_else(|e| e.into_inner());
        if *g == day {
            return false;
        }
        *g = day.to_string();
        true
    }
    fn log_for(&self, day: &str) -> std::sync::MutexGuard<'_, DayLog> {
        let mut g = self.log.lock().unwrap_or_else(|e| e.into_inner());
        if g.day != day {
            *g = DayLog {
                day: day.to_string(),
                ..Default::default()
            };
        }
        g
    }
}

/// 正在专注计时（调度器用：闲聊不说、关怀等一等）
pub fn focusing(st: &AppState) -> bool {
    st.plans.focus().is_some()
}

/// 计划有变化：通知各个窗口刷新、叫醒监督循环
pub fn changed(app: &AppHandle) {
    drop_stale_focus(app);
    let _ = app.emit("plans-changed", ());
    app.state::<AppState>().plans.changed.notify_one();
}

/// 专注的那一项还在「今天要做」里吗（纯函数）：勾掉了、跳过了、删掉了、挪到以后去了 → 不在了
pub fn focus_alive(task: Option<&Task>, today: &str) -> bool {
    task.is_some_and(|t| t.status == "todo" && t.day.as_str() <= today)
}

/// 正在专注的那一项已经不用做了（在列表里勾掉、删掉、挪走，或者整个计划删了）：计时跟着结束，
/// 免得时间到了再问一遍「做完了吗」、把记好的时长盖掉
fn drop_stale_focus(app: &AppHandle) {
    let st = app.state::<AppState>();
    let Some(f) = st.plans.focus() else {
        return;
    };
    let task = super::get_task(&st.db(), f.task_id).ok().flatten();
    if !focus_alive(task.as_ref(), &day_str(today())) {
        st.plans.set_focus(None);
        emit_focus(app);
        log::info!(
            "专注的那一项（#{}）已经不在今天的待做里，计时结束",
            f.task_id
        );
    }
}

/// 她说一句小反应（不进聊天记录）
pub fn react(app: &AppHandle, scene: &str, vars: &[(&str, String)]) {
    let st = app.state::<AppState>();
    if !st.settings().plans.celebrate {
        return;
    }
    crate::companion::say_reaction(app, scene, vars);
}

/// 做完一项之后：心情 / 亲密度，再说一句
pub fn on_done(app: &AppHandle, title: &str, plan: &str, eff: super::DoneEffect) {
    use crate::companion::{Interaction, interact};
    interact(
        app,
        if eff.all_today {
            Interaction::PlanAllDone
        } else {
            Interaction::PlanDone
        },
    );
    let scene = if eff.plan_finished {
        "plan.finished"
    } else if eff.all_today {
        "plan.all_done"
    } else {
        "plan.done"
    };
    react(
        app,
        scene,
        &[("事项", title.to_string()), ("计划", plan.to_string())],
    );
}

/// 计划窗口顶上她的一句话（看今天的进度挑台词）
pub fn header_line(st: &AppState, total: usize, done: usize, streak: u32) -> String {
    let (scene, fallback) = if total == 0 {
        (
            "plan.header_empty",
            "今天还没有安排。想做点什么，加进来我帮你记着。".to_string(),
        )
    } else if done == total {
        ("plan.header_done", "今天的都做完啦，辛苦了！".to_string())
    } else if done > 0 {
        (
            "plan.header_some",
            format!("已经做完 {done} 项了，还剩 {} 项，加油。", total - done),
        )
    } else {
        (
            "plan.header_start",
            format!("今天有 {total} 项要做，从哪个开始？"),
        )
    };
    let vars = [
        ("数量", total.to_string()),
        ("完成", done.to_string()),
        ("剩下", (total - done.min(total)).to_string()),
        ("连续", streak.to_string()),
    ];
    st.line(scene, &vars).unwrap_or(fallback)
}

// ---------------- 纯函数：该不该说 ----------------

/// 「计划日」里的第几分钟（凌晨 5 点 = 0，第二天凌晨 4:59 = 1439）
pub fn plan_minute(minute_of_day: u32) -> u32 {
    (minute_of_day + 1440 - 300) % 1440
}

fn hm_plan_minute(hm: &str) -> Option<u32> {
    proactive::parse_hm(hm).map(plan_minute)
}

/// 到了开始时间、还没提醒过、也没过去太久（两小时内）的事项
pub fn due_starts(tasks: &[Task], now_pm: u32) -> (Vec<&Task>, Vec<&Task>) {
    let mut due = Vec::new();
    let mut stale = Vec::new();
    for t in tasks.iter().filter(|t| t.status == "todo" && !t.reminded) {
        let Some(s) = hm_plan_minute(&t.start) else {
            continue;
        };
        if s <= now_pm {
            if now_pm - s <= 120 {
                due.push(t);
            } else {
                stale.push(t);
            }
        }
    }
    (due, stale)
}

/// 「该做了还没做」的事项：开始时间过了 5 分钟；没定时间的，晚上 7 点以后算
pub fn overdue(tasks: &[Task], now_pm: u32) -> Vec<&Task> {
    let evening = plan_minute(19 * 60);
    tasks
        .iter()
        .filter(|t| t.status == "todo")
        .filter(|t| match hm_plan_minute(&t.start) {
            Some(s) => now_pm >= s + 5,
            None => now_pm >= evening,
        })
        .collect()
}

/// 分心提醒该不该说：在玩游戏 / 看视频够久了、有该做的事、今天还没提够、离上次够久（`since_last` 为空 = 今天还没提过）
pub fn nudge_due(
    strict: Strictness,
    distracted_secs: i64,
    has_overdue: bool,
    nudges: u32,
    since_last: Option<i64>,
    quiet: bool,
) -> bool {
    has_overdue
        && !quiet
        && distracted_secs >= strict.distraction_minutes() * 60
        && nudges < strict.nudges_per_day()
        && since_last.is_none_or(|s| s >= strict.nudge_gap_minutes() * 60)
}

/// 晚上那一问该不该来：到了时间、还有没做完的、今天问的次数没到（第二次至少隔一小时）
pub fn checkin_due(
    strict: Strictness,
    at_pm: u32,
    now_pm: u32,
    left: usize,
    asked: u32,
    since_ask: i64,
) -> bool {
    left > 0 && now_pm >= at_pm && asked < strict.checkins() && (asked == 0 || since_ask >= 3600)
}

// ---------------- 气泡里的选项 ----------------

/// 选了气泡里的某一项之后要做的事
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanChoice {
    /// 晚上那一问：都做完了
    CheckinAll { day: String },
    /// 打开计划窗口的某一页
    Open(String),
    /// 分心提醒：再玩一会儿（几分钟后再说）
    Snooze(u32),
    /// 今天不再因为分心提醒
    QuietToday,
    /// 专注计时到了：做完了
    FocusDone { task_id: i64, minutes: u32 },
    /// 再来一轮（`before`：前几轮一共专注了多少分钟，做完时一起记上）
    FocusMore { task_id: i64, before: u32 },
    /// 日历提醒里点了「已经搞定了」（见 `agenda.rs`）
    AgendaDone(Vec<i64>),
    /// 计划书的节点：记成达标 / 没达标（`open`：顺便打开计划窗口的哪一页）
    Milestone {
        id: i64,
        status: String,
        open: Option<String>,
    },
}

pub fn on_choice(app: &AppHandle, c: &PlanChoice) {
    let st = app.state::<AppState>();
    let now = crate::memory::now_secs();
    match c {
        PlanChoice::CheckinAll { day } => {
            // 先取出来再筛：筛的时候还要查计划（不能一直拿着数据库锁）
            let tasks = super::day_tasks(&st.db(), day);
            let ids: Vec<i64> = tasks
                .into_iter()
                .filter(|t| t.status == "todo" && supervised(&st, t))
                .map(|t| t.id)
                .collect();
            for id in ids {
                let _ = super::complete_task(&st.db(), id, None, None, now);
            }
            crate::companion::interact(app, crate::companion::Interaction::PlanAllDone);
            changed(app);
        }
        PlanChoice::Open(v) => crate::desktop::open_plans_async(app, Some(v.clone())),
        PlanChoice::Snooze(m) => {
            // 这次不算次数；m 分钟后（还在玩的话）再说
            let strict = st.settings().plans.strictness;
            let mut g = st.plans.log_for(&day_str(today()));
            g.nudges = g.nudges.saturating_sub(1);
            g.last_nudge = now - strict.nudge_gap_minutes() * 60 + *m as i64 * 60;
        }
        PlanChoice::QuietToday => st.plans.log_for(&day_str(today())).quiet = true,
        PlanChoice::FocusDone { task_id, minutes } => {
            // 选项的回应就是她的反应，这里只记下来、加心情。
            // 气泡还挂着的时候用户可能已经在列表里勾掉了：那就不再记一遍（不然实际时长被盖掉、心情加两次）
            let still_todo = super::get_task(&st.db(), *task_id)
                .ok()
                .flatten()
                .is_some_and(|t| t.status == "todo");
            if !still_todo {
                return;
            }
            let eff = super::complete_task(&st.db(), *task_id, Some(*minutes), None, now);
            if let Ok(e) = eff {
                crate::companion::interact(
                    app,
                    if e.all_today {
                        crate::companion::Interaction::PlanAllDone
                    } else {
                        crate::companion::Interaction::PlanDone
                    },
                );
            }
            changed(app);
        }
        PlanChoice::FocusMore { task_id, before } => {
            // 时间到的时候计时已经清掉了，前几轮的分钟由选项自己带着
            let minutes = st.settings().plans.focus_minutes;
            let _ = start_focus(app, *task_id, minutes, *before);
        }
        PlanChoice::AgendaDone(ids) => super::agenda::on_done(app, ids),
        PlanChoice::Milestone { id, status, open } => {
            if let Err(e) = super::goal::check_milestone(&st.db(), *id, status, None, now) {
                log::warn!("记节点失败：{e}");
            }
            if let Some(v) = open {
                crate::desktop::open_plans_async(app, Some(v.clone()));
            }
            changed(app);
        }
    }
}

/// 这一项归她管不管（单独的事情都管；计划里的看那个计划开没开监督、是不是进行中）
fn supervised(st: &AppState, t: &Task) -> bool {
    match t.plan_id {
        None => true,
        Some(p) => get_plan(&st.db(), p)
            .ok()
            .flatten()
            .is_some_and(|p| p.supervise && p.status == "active"),
    }
}

fn choice(
    question: String,
    opts: Vec<(String, String, Option<PlanChoice>)>,
) -> crate::companion::Choice {
    let mut c = crate::companion::Choice {
        question,
        options: Vec::new(),
        replies: Vec::new(),
        actions: Vec::new(),
    };
    for (o, r, a) in opts {
        c.options.push(o);
        c.replies.push(r);
        c.actions.push(
            a.map(crate::companion::ChoiceAction::Plan)
                .unwrap_or_default(),
        );
    }
    c
}

fn say(st: &AppState, scene: &str, vars: &[(&str, String)], fallback: impl Into<String>) -> String {
    st.line(scene, vars).unwrap_or_else(|| fallback.into())
}

// ---------------- 专注计时 ----------------

#[derive(Debug, Clone, Serialize)]
pub struct FocusView {
    pub task_id: i64,
    pub title: String,
    pub ends_at: i64,
    pub minutes: u32,
    pub now: i64,
}

fn emit_focus(app: &AppHandle) {
    let v = app.state::<AppState>().plans.focus().map(|f| FocusView {
        task_id: f.task_id,
        title: f.title,
        ends_at: f.ends_at,
        minutes: f.minutes,
        now: crate::memory::now_secs(),
    });
    let _ = app.emit("plan-focus", v);
}

pub fn focus_view(st: &AppState) -> Option<FocusView> {
    st.plans.focus().map(|f| FocusView {
        task_id: f.task_id,
        title: f.title,
        ends_at: f.ends_at,
        minutes: f.minutes,
        now: crate::memory::now_secs(),
    })
}

/// 开始专注：她说一句，桌宠身上显示倒计时
pub fn start_focus(
    app: &AppHandle,
    task_id: i64,
    minutes: u32,
    before: u32,
) -> Result<FocusView, String> {
    let st = app.state::<AppState>();
    let t = super::get_task(&st.db(), task_id)
        .map_err(|e| e.to_string())?
        .ok_or("这一项已经不在了")?;
    if t.status != "todo" {
        return Err("这一项已经做完了".into());
    }
    let minutes = minutes.clamp(5, 180);
    let now = crate::memory::now_secs();
    st.plans.set_focus(Some(Focus {
        task_id,
        title: t.title.clone(),
        started_at: now,
        ends_at: now + minutes as i64 * 60,
        minutes,
        before,
        nudged: 0,
        last_nudge: 0,
    }));
    emit_focus(app);
    st.plans.changed.notify_one();
    crate::companion::say_reaction(
        app,
        "plan.focus_start",
        &[
            ("事项", t.title.clone()),
            ("时长", super::minutes_text(minutes)),
        ],
    );
    log::info!("开始专注 {minutes} 分钟（事项 #{task_id}）");
    focus_view(&st).ok_or_else(|| "开始失败".into())
}

/// 停止专注：`done` = 这一项做完了（按实际专注的时间记）
pub fn stop_focus(app: &AppHandle, done: bool) -> Result<(), String> {
    let st = app.state::<AppState>();
    let Some(f) = st.plans.focus() else {
        return Ok(());
    };
    st.plans.set_focus(None);
    emit_focus(app);
    if done {
        let now = crate::memory::now_secs();
        let spent = f.before + ((now - f.started_at).max(60) / 60) as u32;
        let (eff, plan) = {
            let db = st.db();
            let eff = super::complete_task(&db, f.task_id, Some(spent), None, now)?;
            let plan = super::get_task(&db, f.task_id)
                .ok()
                .flatten()
                .and_then(|t| t.plan_id)
                .and_then(|p| get_plan(&db, p).ok().flatten())
                .map(|p| p.title)
                .unwrap_or_default();
            (eff, plan)
        };
        on_done(app, &f.title, &plan, eff);
    }
    changed(app);
    Ok(())
}

/// 专注时间到了：问做完没有（用户自己开的计时，和提醒同级）
fn focus_end(app: &AppHandle, f: Focus) {
    let st = app.state::<AppState>();
    st.plans.set_focus(None);
    emit_focus(app);
    let item = focus_end_item(&st, &f, crate::memory::now_secs());
    proactive::submit(app, item);
    log::info!("专注时间到（事项 #{}）", f.task_id);
}

/// 时间到了的那一问（几轮加起来一共专注了多久，由选项带着）
fn focus_end_item(st: &AppState, f: &Focus, now: i64) -> Item {
    let total = f.before + f.minutes;
    let mins = super::minutes_text(f.minutes);
    let more = st.settings().plans.focus_minutes;
    let q = say(
        st,
        "plan.focus_end",
        &[("事项", f.title.clone()), ("时长", mins.clone())],
        format!("{mins}到啦！「{}」做完了吗？", f.title),
    );
    let c = choice(
        q.clone(),
        vec![
            (
                "做完了".into(),
                say(
                    st,
                    "plan.focus_done",
                    &[("事项", f.title.clone())],
                    "太好了！记下来了，休息一下吧。",
                ),
                Some(PlanChoice::FocusDone {
                    task_id: f.task_id,
                    minutes: total,
                }),
            ),
            (
                format!("还没，再来{more}分钟"),
                say(
                    st,
                    "plan.focus_more",
                    &[("时长", super::minutes_text(more))],
                    format!("好，再陪你{}。", super::minutes_text(more)),
                ),
                Some(PlanChoice::FocusMore {
                    task_id: f.task_id,
                    before: total,
                }),
            ),
            (
                "先休息一下".into(),
                say(st, "plan.focus_rest", &[], "嗯，起来走走、喝口水。"),
                None,
            ),
        ],
    );
    let mut item = Item::new(Kind::Reminder, q, now + 2 * 3600);
    item.key = format!("plan-focus-end:{}", f.task_id);
    item.choices = Some(c);
    item
}

// ---------------- 监督循环 ----------------

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 桌宠出现、提醒补完之后再开始
        tokio::time::sleep(Duration::from_secs(8)).await;
        loop {
            tick(&app);
            let wait = {
                let st = app.state::<AppState>();
                let now = crate::memory::now_secs();
                st.plans
                    .focus()
                    .map(|f| (f.ends_at - now).clamp(1, 30))
                    .unwrap_or(30)
            };
            let st = app.state::<AppState>();
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(wait as u64)) => {}
                _ = st.plans.changed.notified() => {}
            }
        }
    });
}

/// 现在是不是在玩游戏 / 看视频（不是隐私名单里的），玩了多少秒
fn distracted(st: &AppState, now: i64) -> Option<(i64, &'static str)> {
    let c = st.activity.current()?;
    if c.private {
        return None;
    }
    let name = match c.category {
        Category::Game => "玩游戏",
        Category::Video => "看视频",
        _ => return None,
    };
    Some((now - c.since, name))
}

fn tick(app: &AppHandle) {
    let st = app.state::<AppState>();
    let now = crate::memory::now_secs();
    let day = today();
    let d = day_str(day);
    super::ensure_day(&st.db(), day, now);
    if st.plans.new_day(&d) {
        // 凌晨 5 点换了天：开着的计划窗口、桌宠跟着刷新（不然一直显示昨天的）
        let _ = app.emit("plans-changed", ());
    }

    // 专注计时
    if let Some(f) = st.plans.focus() {
        if now >= f.ends_at {
            focus_end(app, f);
        } else if let Some((secs, what)) = distracted(&st, now)
            && secs >= 120
            && f.nudged < 2
            && now - f.last_nudge >= 300
        {
            let text = say(
                &st,
                "plan.focus_distracted",
                &[("事项", f.title.clone()), ("活动", what.to_string())],
                format!("说好专心做「{}」的呢……先回来吧，我在等你。", f.title),
            );
            let mut item = Item::new(Kind::Plan, text, now + 600);
            item.key = "plan-focus-nudge".into();
            proactive::submit(app, item);
            let mut g = st.plans.focus.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(x) = g.as_mut() {
                x.nudged += 1;
                x.last_nudge = now;
            }
        }
    }

    let s = st.settings().plans;
    if !s.supervise {
        return;
    }
    // 先取出来再筛（筛的时候还要查计划，不能一直拿着数据库锁）
    let all = super::day_tasks(&st.db(), &d);
    let tasks: Vec<Task> = all.into_iter().filter(|t| supervised(&st, t)).collect();
    let local = crate::reminders::now_local();
    let now_pm = plan_minute(local.hour() * 60 + local.minute());

    // 到点提醒
    let (due, stale) = due_starts(&tasks, now_pm);
    let mark: Vec<i64> = due.iter().chain(stale.iter()).map(|t| t.id).collect();
    if s.start_remind && !due.is_empty() {
        let names: Vec<String> = due
            .iter()
            .map(|t| {
                if t.minutes > 0 {
                    format!("「{}」（{}）", t.title, super::minutes_text(t.minutes))
                } else {
                    format!("「{}」", t.title)
                }
            })
            .collect();
        let text = say(
            &st,
            "plan.start",
            &[("事项", names.join("、")), ("时间", due[0].start.clone())],
            format!("{}啦，该开始{}了。", due[0].start, names.join("、")),
        );
        let first = due[0].id;
        let mut item = Item::new(Kind::Reminder, text.clone(), now + 2 * 3600);
        item.key = format!("plan-start:{first}");
        item.choices = Some(choice(
            text,
            vec![
                (
                    "开始专注".into(),
                    say(&st, "plan.focus_ack", &[], "好！计时开始，我陪着你。"),
                    Some(PlanChoice::FocusMore {
                        task_id: first,
                        before: 0,
                    }),
                ),
                (
                    "等一下".into(),
                    say(&st, "plan.start_later", &[], "好，别拖太久哦。"),
                    None,
                ),
                (
                    "打开计划".into(),
                    say(&st, "plan.open", &[], "好，帮你打开了。"),
                    Some(PlanChoice::Open("today".into())),
                ),
            ],
        ));
        proactive::submit(app, item);
        log::info!("计划：到点提醒 {} 项", due.len());
    }
    if !mark.is_empty() {
        let db = st.db();
        for id in mark {
            let _ = db.execute("UPDATE plan_tasks SET reminded = 1 WHERE id = ?1", [id]);
        }
    }

    // 分心提醒（专注计时时由上面那段管）
    if s.distraction && st.settings().activity.enabled && st.plans.focus().is_none() {
        let late = overdue(&tasks, now_pm);
        if let Some((secs, what)) = distracted(&st, now) {
            let (nudges, last, quiet) = {
                let g = st.plans.log_for(&d);
                (g.nudges, g.last_nudge, g.quiet)
            };
            let since = (last > 0).then_some(now - last);
            if nudge_due(s.strictness, secs, !late.is_empty(), nudges, since, quiet) {
                let t = late[0];
                let text = say(
                    &st,
                    "plan.nudge",
                    &[("事项", t.title.clone()), ("活动", what.to_string())],
                    format!("在{what}呀……「{}」还没做呢，要不要先做完再玩？", t.title),
                );
                let mut item = Item::new(Kind::Plan, text.clone(), now + 30 * 60);
                item.key = "plan-nudge".into();
                item.choices = Some(choice(
                    text,
                    vec![
                        (
                            "好，这就去".into(),
                            say(&st, "plan.nudge_ok", &[], "嗯！我等你～"),
                            None,
                        ),
                        (
                            "再玩十分钟".into(),
                            say(
                                &st,
                                "plan.nudge_later",
                                &[],
                                "好吧，就十分钟哦，到时候我再叫你。",
                            ),
                            Some(PlanChoice::Snooze(10)),
                        ),
                        (
                            "今天先不做了".into(),
                            say(
                                &st,
                                "plan.nudge_skip",
                                &[],
                                "好，今天就放松一下吧，没做完的我排到后面了。",
                            ),
                            Some(PlanChoice::QuietToday),
                        ),
                    ],
                ));
                proactive::submit(app, item);
                let mut g = st.plans.log_for(&d);
                g.nudges += 1;
                g.last_nudge = now;
                log::info!("计划：分心提醒（第 {} 次）", g.nudges);
            }
        }
    }

    // 晚上问一句
    if s.checkin
        && let Some(at) = hm_plan_minute(&s.checkin_at)
    {
        let left: Vec<&Task> = tasks.iter().filter(|t| t.status == "todo").collect();
        let (asked, ts) = checkin_record(&st, &d);
        // 已经说过「今天先不做了」（分心提醒或第一次晚上那一问里选的）：严格模式也不再追问
        let quiet = st.plans.log_for(&d).quiet;
        if !quiet && checkin_due(s.strictness, at, now_pm, left.len(), asked, now - ts) {
            let names: Vec<String> = left
                .iter()
                .take(3)
                .map(|t| format!("「{}」", t.title))
                .collect();
            let q = say(
                &st,
                "plan.checkin",
                &[("剩下", left.len().to_string()), ("事项", names.join("、"))],
                format!(
                    "今天的计划怎么样了？还有 {} 项没勾：{}。",
                    left.len(),
                    names.join("、")
                ),
            );
            let mut item = Item::new(Kind::Plan, q.clone(), now + 3 * 3600);
            item.key = "plan-checkin".into();
            item.choices = Some(choice(
                q,
                vec![
                    (
                        "都做完了".into(),
                        say(&st, "plan.checkin_all", &[], "真的？好厉害！帮你都勾上了。"),
                        Some(PlanChoice::CheckinAll { day: d.clone() }),
                    ),
                    (
                        "做了一部分".into(),
                        say(
                            &st,
                            "plan.checkin_part",
                            &[],
                            "好，打开计划了，勾一下做完的那几项吧。",
                        ),
                        Some(PlanChoice::Open("today".into())),
                    ),
                    (
                        "今天先不做了".into(),
                        say(
                            &st,
                            "plan.checkin_none",
                            &[],
                            "没关系，没做完的我挪到后面的日子了。早点休息～",
                        ),
                        Some(PlanChoice::QuietToday),
                    ),
                ],
            ));
            proactive::submit(app, item);
            let _ =
                crate::settings::kv_set(&st.db(), KEY_CHECKIN, &format!("{d}|{}|{now}", asked + 1));
            log::info!("计划：晚上问一句（还剩 {} 项）", left.len());
        }
    }

    // 计划书：节点、每周自查
    goal_checks(app, &st, &s, now, day, now_pm);
}

const KEY_CHECKIN: &str = "plan_checkin";
/// 计划书的节点、自查问过了的：{"ms:<id>": 日期, "review:<计划书 id>": 日期}
const KEY_GOAL_ASKED: &str = "plan_goal_asked";

fn goal_asked(st: &AppState) -> std::collections::BTreeMap<String, String> {
    crate::settings::kv_get(&st.db(), KEY_GOAL_ASKED)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

fn mark_goal_asked(st: &AppState, key: String, day: &str) {
    let mut m = goal_asked(st);
    m.insert(key, day.to_string());
    // 只留最近的（日期字符串能直接比大小）
    let cutoff = super::parse_day(day)
        .map(|d| day_str(d - chrono::Duration::days(40)))
        .unwrap_or_default();
    m.retain(|_, d| *d >= cutoff);
    let _ = crate::settings::kv_set(
        &st.db(),
        KEY_GOAL_ASKED,
        &serde_json::to_string(&m).unwrap_or_default(),
    );
}

/// 计划书这件事今天还没问过
pub fn goal_due(asked: &std::collections::BTreeMap<String, String>, key: &str, day: &str) -> bool {
    asked.get(key).is_none_or(|d| d != day)
}

/// 这周（到今天为止）的情况：（「5/8 项（3 小时）」，「专业课 3/4、真题 1/3。「真题」落下了一些。」）
pub fn review_summary(names: &HashMap<i64, String>, w: &super::goal::WeekStat) -> (String, String) {
    let done = format!(
        "{}/{} 项（{}）",
        w.tasks_done,
        w.tasks,
        super::minutes_text(w.done)
    );
    let mut parts = Vec::new();
    let mut behind = Vec::new();
    for t in w.tracks.iter().filter(|t| t.tasks > 0) {
        let name = names.get(&t.plan_id).cloned().unwrap_or_default();
        parts.push(format!("{name} {}/{}", t.tasks_done, t.tasks));
        if t.tasks_done * 2 < t.tasks {
            behind.push(format!("「{name}」"));
        }
    }
    let mut how = if parts.is_empty() {
        "这周还没有记录。".to_string()
    } else {
        format!("{}。", parts.join("、"))
    };
    if !behind.is_empty() {
        how.push_str(&format!("{}落下了一些。", behind.join("、")));
    }
    (done, how)
}

/// 计划书：节点当天问达标没有；自查那天带着这周的情况过一遍。
/// 都走调度器（Kind::Plan），一次只问一件，两件之间至少隔 15 分钟
fn goal_checks(
    app: &AppHandle,
    st: &AppState,
    s: &PlanSettings,
    now: i64,
    day: chrono::NaiveDate,
    now_pm: u32,
) {
    if !s.milestone_check && !s.review {
        return;
    }
    let Some(at) = hm_plan_minute(&s.review_at) else {
        return;
    };
    let d = day_str(day);
    if now_pm < at || now - st.plans.log_for(&d).goal_asked < 15 * 60 {
        return;
    }
    let asked = goal_asked(st);
    let goals: Vec<super::goal::Goal> = super::goal::list_goals(&st.db())
        .unwrap_or_default()
        .into_iter()
        .filter(|g| g.status == "active")
        .collect();
    if s.milestone_check {
        for g in &goals {
            let ms = super::goal::list_milestones(&st.db(), g.id);
            let Some(m) = ms.into_iter().find(|m| {
                m.status == "todo" && m.day == d && goal_due(&asked, &format!("ms:{}", m.id), &d)
            }) else {
                continue;
            };
            let crit = if m.criteria.is_empty() {
                "按你自己的感觉".to_string()
            } else {
                m.criteria.clone()
            };
            let vars = [
                ("节点", m.title.clone()),
                ("标准", crit.clone()),
                ("计划", g.title.clone()),
            ];
            let q = say(
                st,
                "plan.milestone_check",
                &vars,
                format!("今天是「{}」的节点。标准是：{crit}。达标了吗？", m.title),
            );
            let nv = [("节点", m.title.clone())];
            let c = choice(
                q.clone(),
                vec![
                    (
                        "达标了".into(),
                        say(st, "plan.milestone_yes", &nv, "太好了！记成达标了。"),
                        Some(PlanChoice::Milestone {
                            id: m.id,
                            status: "passed".into(),
                            open: None,
                        }),
                    ),
                    (
                        "还没有".into(),
                        say(
                            st,
                            "plan.milestone_no",
                            &nv,
                            "没关系，打开计划书看看要不要把后面调一调。",
                        ),
                        Some(PlanChoice::Milestone {
                            id: m.id,
                            status: "missed".into(),
                            open: Some(format!("goal:{}:m{}", g.id, m.id)),
                        }),
                    ),
                    (
                        "晚点再说".into(),
                        say(
                            st,
                            "plan.milestone_later",
                            &nv,
                            "好，想好了在计划书里点一下就行。",
                        ),
                        None,
                    ),
                ],
            );
            let mut item = Item::new(Kind::Plan, q, now + 3 * 3600);
            item.key = format!("plan-milestone:{}", m.id);
            item.choices = Some(c);
            proactive::submit(app, item);
            mark_goal_asked(st, format!("ms:{}", m.id), &d);
            st.plans.log_for(&d).goal_asked = now;
            log::info!("计划书：问节点 #{}", m.id);
            return;
        }
    }
    if s.review {
        let wd = super::weekday_index(day) as i64;
        for g in goals.iter().filter(|g| g.review_day == wd) {
            let key = format!("review:{}", g.id);
            if !goal_due(&asked, &key, &d) {
                continue;
            }
            let (mon, _) = super::goal::week_of(day);
            let (w, names) = {
                let db = st.db();
                let w = super::goal::week_stat(&db, g.id, mon, day);
                let names: HashMap<i64, String> = super::goal::goal_tracks(&db, g.id)
                    .into_iter()
                    .map(|p| (p.id, p.title))
                    .collect();
                (w, names)
            };
            mark_goal_asked(st, key, &d);
            if w.tasks == 0 {
                continue;
            }
            let (done, how) = review_summary(&names, &w);
            let list = g
                .checklist
                .iter()
                .take(2)
                .map(|c| format!("「{c}」"))
                .collect::<Vec<_>>()
                .join("、");
            let gv = [("计划", g.title.clone())];
            let q = say(
                st,
                "plan.review",
                &[
                    ("计划", g.title.clone()),
                    ("完成", done.clone()),
                    ("情况", how.clone()),
                    ("清单", list),
                ],
                format!(
                    "周末自查时间～「{}」这周做完 {done}。{how}要不要我根据这周的情况调一下下周？",
                    g.title
                ),
            );
            let c = choice(
                q.clone(),
                vec![
                    (
                        "帮我调一下下周".into(),
                        say(
                            st,
                            "plan.review_adjust",
                            &gv,
                            "好，打开了，写一句想怎么调就行。",
                        ),
                        Some(PlanChoice::Open(format!("goal:{}:adjust", g.id))),
                    ),
                    (
                        "看看自查清单".into(),
                        say(st, "plan.review_list", &gv, "打开自查清单了。"),
                        Some(PlanChoice::Open(format!("goal:{}:week", g.id))),
                    ),
                    (
                        "照原计划就好".into(),
                        say(st, "plan.review_keep", &gv, "好，下周继续加油！"),
                        None,
                    ),
                ],
            );
            let mut item = Item::new(Kind::Plan, q, now + 3 * 3600);
            item.key = format!("plan-review:{}", g.id);
            item.choices = Some(c);
            proactive::submit(app, item);
            st.plans.log_for(&d).goal_asked = now;
            log::info!("计划书：周末自查 #{}", g.id);
            return;
        }
    }
}

/// 今天晚上问过几次、上次什么时候
fn checkin_record(st: &AppState, day: &str) -> (u32, i64) {
    let Some(v) = crate::settings::kv_get(&st.db(), KEY_CHECKIN) else {
        return (0, 0);
    };
    let p: Vec<&str> = v.split('|').collect();
    match p.as_slice() {
        [d, n, ts] if *d == day => (n.parse().unwrap_or(0), ts.parse().unwrap_or(0)),
        _ => (0, 0),
    }
}

// ---------------- 聊天、播报、本地回答 ----------------

/// 聊天时给她的背景资料：今天的计划（没有计划、关掉了返回 None）
pub fn chat_note(st: &AppState) -> Option<String> {
    let s = st.settings().plans;
    if !s.chat_context {
        return None;
    }
    let day = today();
    let db = st.db();
    super::ensure_day(&db, day, crate::memory::now_secs());
    let mut parts = Vec::new();
    if let Some((n, done, todo)) = super::today_brief(&db, day, 5) {
        if todo.is_empty() {
            parts.push(format!("今天的 {n} 项都做完了"));
        } else {
            parts.push(format!("今天一共 {n} 项，做完 {done} 项；还没做：{todo}"));
        }
    }
    // 计划书：现在在哪个阶段、下一个节点
    let d = day_str(day);
    for g in super::goal::list_goals(&db)
        .unwrap_or_default()
        .into_iter()
        .filter(|g| g.status == "active")
        .take(2)
    {
        let phase = super::goal::list_phases(&db, g.id)
            .into_iter()
            .find(|p| p.start_day <= d && d <= p.end_day);
        let next = super::goal::list_milestones(&db, g.id)
            .into_iter()
            .find(|m| m.status == "todo" && m.day >= d);
        let mut s = format!("计划书「{}」", g.title);
        if let Some(p) = phase {
            s.push_str(&format!("现在是「{}」阶段（到 {}）", p.title, p.end_day));
            if !p.objective.is_empty() {
                s.push_str(&format!("，目标：{}", p.objective));
            }
        }
        if let Some(m) = next {
            s.push_str(&format!("，下一个节点是 {}「{}」", m.day, m.title));
        }
        parts.push(s);
    }
    drop(db);
    if let Some(f) = st.plans.focus() {
        let left = ((f.ends_at - crate::memory::now_secs()).max(0) + 59) / 60;
        parts.push(format!("用户正在专注做「{}」，还剩约 {left} 分钟", f.title));
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "【今天的计划】{}。（这是用户自己在「计划」里排的。用户问起时可以说说、给点鼓励；不要每句话都提，也不要催。）",
        parts.join("；")
    ))
}

/// 聊天里问「今天要做什么」：本地直接回答
pub fn today_answer(st: &AppState) -> Option<String> {
    let day = today();
    let db = st.db();
    super::ensure_day(&db, day, crate::memory::now_secs());
    let (n, done, todo) = super::today_brief(&db, day, 6)?;
    drop(db);
    Some(if todo.is_empty() {
        say(
            st,
            "plan.today_all_done",
            &[("数量", n.to_string())],
            format!("今天的 {n} 项都做完啦！"),
        )
    } else {
        say(
            st,
            "plan.today_list",
            &[("数量", (n - done).to_string()), ("事项", todo.clone())],
            format!("今天还有 {} 项：{todo}。", n - done),
        )
    })
}

/// 播报里的「今天的计划」：（共几项、做完几项、还没做的）
pub fn briefing_facts(st: &AppState) -> Option<(usize, usize, String)> {
    if !st.settings().plans.briefing {
        return None;
    }
    let day = today();
    let db = st.db();
    super::ensure_day(&db, day, crate::memory::now_secs());
    super::today_brief(&db, day, 4)
}

/// 回复里的 `[[打卡：…]]` / `[[计划：…]]` → 气泡里的按钮（用户点了才做）
pub fn chat_actions(
    st: &AppState,
    found: &[crate::markers::Marker],
) -> Vec<crate::abilities::ActionView> {
    use crate::markers::MarkerKind;
    found
        .iter()
        .filter_map(|m| match m.kind {
            MarkerKind::Checkin => Some(match find_task(st, &m.text) {
                Some(t) => crate::abilities::ActionView {
                    kind: m.kind,
                    target: t.id.to_string(),
                    label: format!("记成做完：{}", t.title),
                    error: None,
                },
                None => crate::abilities::ActionView {
                    kind: m.kind,
                    target: String::new(),
                    label: format!("打卡：{}", m.text),
                    error: Some("今天的安排里没找到对得上的一项，可以打开计划自己勾。".into()),
                },
            }),
            MarkerKind::Plan => Some(crate::abilities::ActionView {
                kind: m.kind,
                target: m.text.chars().take(500).collect(),
                label: format!("做个计划：{}", m.text.chars().take(30).collect::<String>()),
                error: None,
            }),
            _ => None,
        })
        .collect()
}

/// 模型写了 `[[打卡：事项]]`：在今天没做完的里找对得上的那一项
pub fn find_task(st: &AppState, name: &str) -> Option<Task> {
    let key: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    if key.is_empty() {
        return None;
    }
    let tasks: Vec<Task> = super::day_tasks(&st.db(), &day_str(today()))
        .into_iter()
        .filter(|t| t.status == "todo")
        .collect();
    let norm = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    // 名字互相包含的优先；否则按共同的字数挑最像的（至少一半的字对得上）
    if let Some(t) = tasks.iter().find(|t| {
        let n = norm(&t.title);
        n.contains(&key) || key.contains(&n)
    }) {
        return Some(t.clone());
    }
    let mut best: Option<(usize, &Task)> = None;
    for t in &tasks {
        let n = norm(&t.title);
        let mut counts: HashMap<char, usize> = HashMap::new();
        for c in n.chars() {
            *counts.entry(c).or_default() += 1;
        }
        let common = key
            .chars()
            .filter(|c| {
                counts.get_mut(c).is_some_and(|x| {
                    if *x > 0 {
                        *x -= 1;
                        true
                    } else {
                        false
                    }
                })
            })
            .count();
        if common * 2 >= key.chars().count() && best.is_none_or(|(b, _)| common > b) {
            best = Some((common, t));
        }
    }
    best.map(|(_, t)| t.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: i64, start: &str, status: &str, reminded: bool) -> Task {
        Task {
            id,
            title: format!("事项{id}"),
            start: start.into(),
            status: status.into(),
            reminded,
            minutes: 30,
            ..Default::default()
        }
    }

    #[test]
    fn plan_minutes_wrap_at_five() {
        assert_eq!(plan_minute(5 * 60), 0);
        assert_eq!(plan_minute(4 * 60 + 59), 1439);
        assert!(
            plan_minute(23 * 60 + 30) < plan_minute(60),
            "凌晨一点在晚上十一点半之后"
        );
    }

    #[test]
    fn start_reminders() {
        let tasks = vec![
            t(1, "20:00", "todo", false),
            t(2, "20:00", "todo", true),
            t(3, "20:00", "done", false),
            t(4, "", "todo", false),
            t(5, "17:00", "todo", false),
            t(6, "23:00", "todo", false),
        ];
        let now = plan_minute(20 * 60 + 3);
        let (due, stale) = due_starts(&tasks, now);
        assert_eq!(due.iter().map(|t| t.id).collect::<Vec<_>>(), [1]);
        assert_eq!(
            stale.iter().map(|t| t.id).collect::<Vec<_>>(),
            [5],
            "过去三个小时的就不提了"
        );
        // 凌晨一点：晚上十一点的那项还在两小时内
        let (due, _) = due_starts(&tasks, plan_minute(60));
        assert!(due.iter().any(|t| t.id == 6));
    }

    #[test]
    fn overdue_tasks() {
        let tasks = vec![
            t(1, "20:00", "todo", false),
            t(2, "", "todo", false),
            t(3, "", "done", false),
        ];
        let ids = |v: Vec<&Task>| v.iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(
            ids(overdue(&tasks, plan_minute(20 * 60 + 2))),
            [2],
            "定了 8 点的刚到点不算；没定时间的晚上七点后算"
        );
        assert_eq!(overdue(&tasks, plan_minute(20 * 60 + 10)).len(), 2);
        assert_eq!(overdue(&tasks, plan_minute(15 * 60)).len(), 0, "下午还早");
    }

    #[test]
    fn nudge_rules() {
        let n = Strictness::Normal;
        assert!(nudge_due(n, 16 * 60, true, 0, None, false));
        assert!(!nudge_due(n, 10 * 60, true, 0, None, false), "才玩十分钟");
        assert!(!nudge_due(n, 16 * 60, false, 0, None, false), "没有该做的");
        assert!(
            !nudge_due(n, 16 * 60, true, 1, Some(20 * 60), false),
            "离上次太近"
        );
        assert!(nudge_due(n, 16 * 60, true, 1, Some(50 * 60), false));
        assert!(
            !nudge_due(n, 16 * 60, true, 2, Some(99 * 60), false),
            "今天提够了"
        );
        assert!(!nudge_due(n, 16 * 60, true, 0, None, true), "说了今天不做");
        // 「再玩十分钟」：不算次数，但要隔开
        assert!(!nudge_due(n, 16 * 60, true, 0, Some(60), false));
        assert!(
            !nudge_due(Strictness::Gentle, 16 * 60, true, 0, None, false),
            "温和：玩久一点才说"
        );
    }

    #[test]
    fn checkin_rules() {
        let at = plan_minute(21 * 60 + 30);
        let n = Strictness::Normal;
        assert!(
            !checkin_due(n, at, plan_minute(21 * 60), 2, 0, 0),
            "还没到时间"
        );
        assert!(checkin_due(n, at, plan_minute(21 * 60 + 31), 2, 0, 0));
        assert!(
            checkin_due(n, at, plan_minute(60), 2, 0, 0),
            "过了午夜还算今天"
        );
        assert!(
            !checkin_due(n, at, plan_minute(22 * 60), 0, 0, 0),
            "都做完了不问"
        );
        assert!(
            !checkin_due(n, at, plan_minute(22 * 60), 2, 1, 7200),
            "适中：只问一次"
        );
        assert!(checkin_due(
            Strictness::Strict,
            at,
            plan_minute(23 * 60),
            2,
            1,
            3700
        ));
        assert!(!checkin_due(
            Strictness::Strict,
            at,
            plan_minute(23 * 60),
            2,
            1,
            600
        ));
    }

    #[test]
    fn focus_ends_when_task_leaves_today() {
        let today = "2026-11-11";
        let mut x = t(1, "", "todo", false);
        x.day = today.into();
        assert!(focus_alive(Some(&x), today));
        assert!(!focus_alive(None, today), "删掉了");
        let done = Task {
            status: "done".into(),
            ..x.clone()
        };
        assert!(!focus_alive(Some(&done), today), "在列表里勾掉了");
        let skipped = Task {
            status: "skipped".into(),
            ..x.clone()
        };
        assert!(!focus_alive(Some(&skipped), today));
        let moved = Task {
            day: "2026-11-12".into(),
            ..x.clone()
        };
        assert!(!focus_alive(Some(&moved), today), "推到明天了");
        let late = Task {
            day: "2026-11-10".into(),
            ..x
        };
        assert!(focus_alive(Some(&late), today), "跨过凌晨 5 点的那一轮照常");
    }

    #[test]
    fn focus_rounds_add_up() {
        // 「再来一轮」：时间到时计时已经清掉了，第二轮要由选项带上前面几轮的分钟，
        // 第二轮结束时「做完了」记的是几轮加起来
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        let round = |before: u32| Focus {
            task_id: 7,
            title: "第3章".into(),
            started_at: 0,
            ends_at: 25 * 60,
            minutes: 25,
            before,
            nudged: 0,
            last_nudge: 0,
        };
        let actions = |f: &Focus| {
            let item = focus_end_item(&st, f, 0);
            assert_eq!(item.kind, Kind::Reminder);
            item.choices.unwrap().actions
        };
        use crate::companion::ChoiceAction;
        let first = actions(&round(0));
        assert_eq!(
            first[1],
            ChoiceAction::Plan(PlanChoice::FocusMore {
                task_id: 7,
                before: 25
            })
        );
        let second = actions(&round(25));
        assert_eq!(
            second[0],
            ChoiceAction::Plan(PlanChoice::FocusDone {
                task_id: 7,
                minutes: 50
            })
        );
        assert_eq!(second[2], ChoiceAction::None, "先休息一下：什么都不做");
    }

    #[test]
    fn goal_review_helpers() {
        use super::super::goal::{TrackWeek, WeekStat};
        let mut asked = std::collections::BTreeMap::new();
        assert!(goal_due(&asked, "ms:1", "2026-11-09"));
        asked.insert("ms:1".to_string(), "2026-11-09".to_string());
        assert!(!goal_due(&asked, "ms:1", "2026-11-09"), "今天问过了");
        assert!(goal_due(&asked, "ms:1", "2026-11-10"));
        let w = WeekStat {
            tasks: 8,
            tasks_done: 5,
            done: 180,
            tracks: vec![
                TrackWeek {
                    plan_id: 1,
                    tasks: 4,
                    tasks_done: 4,
                    ..Default::default()
                },
                TrackWeek {
                    plan_id: 2,
                    tasks: 4,
                    tasks_done: 1,
                    ..Default::default()
                },
                TrackWeek {
                    plan_id: 3,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let names = HashMap::from([
            (1, "专业课".to_string()),
            (2, "真题".to_string()),
            (3, "联系老师".to_string()),
        ]);
        let (done, how) = review_summary(&names, &w);
        assert_eq!(done, "5/8 项（3 小时）");
        assert_eq!(how, "专业课 4/4、真题 1/4。「真题」落下了一些。");
        let s = PlanSettings {
            review_at: "25:00".into(),
            ..Default::default()
        }
        .sanitize();
        assert_eq!(s.review_at, "20:00");
    }

    #[test]
    fn finds_task_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        let day = day_str(today());
        for title in ["第3章 特征值（第一部分）", "背单词", "交实验报告"] {
            super::super::save_task(
                &st.db(),
                &Task {
                    day: day.clone(),
                    title: title.into(),
                    ..Default::default()
                },
                0,
            )
            .unwrap();
        }
        assert_eq!(
            find_task(&st, "特征值").unwrap().title,
            "第3章 特征值（第一部分）"
        );
        assert_eq!(find_task(&st, "实验报告").unwrap().title, "交实验报告");
        assert_eq!(find_task(&st, "单词背完了").unwrap().title, "背单词");
        assert!(find_task(&st, "健身").is_none());
        let a = today_answer(&st).unwrap();
        assert!(a.contains("背单词"), "{a}");
        assert!(chat_note(&st).unwrap().contains("还没做"));
    }
}
