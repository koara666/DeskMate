//! 日历：提前知道的事（作业 11 月 25 日截止、12 月 1 日某节课停课、考试、交表……）。
//! 在计划窗口的「日历」页点那一天写下来；她在那天的 **7 天前、3 天前、1 天前和当天** 各说一次，
//! 时间是那一天第一次打开桌宠的时候（电脑一直开着就是换天后第一次回到电脑前）。标了「搞定了」就不再提醒。
//!
//! 和「提醒」分开：提醒管当天、到点的急事；日历管提前知道、要提前几天开始惦记的事。
//! - 中间几天没开电脑，错过的那几档不补一串：只按现在还剩几天说一次（第 5 天才打开就说「还有 5 天」，之后照常 3 天、1 天、当天）。
//! - 同一天要说的几条合成一句。
//! - 刚记下的不马上提醒（记的时候已经在 7 天内，就从下一档开始）。
//! - 和用户自己设的提醒同级（`Kind::Reminder`）：勿扰挡不住，全屏时改发系统通知。
//!
//! 「到了哪一档、该不该说、怎么说」都是纯函数（有测试）；日子按「计划日」算（凌晨 5 点换天），和计划一致。

use super::{day_str, parse_day, today};
use crate::state::AppState;
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

/// 提前几天说（从远到近）；0 = 当天
pub const AHEAD: [i64; 4] = [7, 3, 1, 0];
/// 一条最多多少字
pub const MAX_TITLE: usize = 100;
pub const MAX_NOTE: usize = 1000;
/// 日历页一次最多取多少天（一个月的格子是 6 周 = 42 天）
const MAX_RANGE_DAYS: i64 = 100;

/// 日历上的一条
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Entry {
    pub id: i64,
    /// YYYY-MM-DD
    pub day: String,
    pub title: String,
    pub note: String,
    /// 搞定了（不再提醒）
    pub done: bool,
    pub done_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// 已经说过的那一档（7 / 3 / 1 / 0），界面用不到
    #[serde(skip)]
    pub reminded: Option<i64>,
}

// ---------------- 纯函数：到了哪一档、该不该说、怎么说 ----------------

/// 还剩 `days_left` 天时落在哪一档：7 天内取「不小于还剩天数的最近一档」（还剩 5 天 → 7 天那档），
/// 已经过去、或者还早（超过 7 天）→ None
pub fn stage(days_left: i64) -> Option<i64> {
    if !(0..=AHEAD[0]).contains(&days_left) {
        return None;
    }
    AHEAD.iter().copied().filter(|t| *t >= days_left).min()
}

/// 现在该不该说（该说就返回这一档）：这一档还没说过、也没有更近的一档说过；搞定了的不说
pub fn due(days_left: i64, reminded: Option<i64>, done: bool) -> Option<i64> {
    if done {
        return None;
    }
    let s = stage(days_left)?;
    match reminded {
        Some(r) if r <= s => None,
        _ => Some(s),
    }
}

/// 刚记下（或者改了日子）时算「已经说过」哪一档：已经在 7 天内的，这一档就不说了，从下一档开始
pub fn fresh_reminded(days_left: i64) -> Option<i64> {
    stage(days_left)
}

/// 「11月25日」
pub fn md(day: NaiveDate) -> String {
    format!("{}月{}日", day.month(), day.day())
}

/// 「今天」「明天（11月25日）」「后天（11月26日）」「还有 7 天（12月1日）」
pub fn when_text(days_left: i64, day: NaiveDate) -> String {
    match days_left {
        0 => "今天".to_string(),
        1 => format!("明天（{}）", md(day)),
        2 => format!("后天（{}）", md(day)),
        n => format!("还有 {n} 天（{}）", md(day)),
    }
}

/// 她说的那一句：几条合在一起，近的在前
pub fn remind_text(items: &[(i64, NaiveDate, String)]) -> String {
    let mut v: Vec<&(i64, NaiveDate, String)> = items.iter().collect();
    v.sort_by_key(|(d, day, _)| (*d, *day));
    let parts: Vec<String> = v
        .iter()
        .map(|(d, day, t)| format!("{}「{t}」", when_text(*d, *day)))
        .collect();
    let tail = if v.iter().any(|(d, ..)| *d == 0) {
        "别忘啦。"
    } else {
        "先记在心里哦。"
    };
    format!("日历上记着：{}。{tail}", parts.join("；"))
}

fn clip(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

impl Entry {
    /// 检查、整理用户填的：内容不能空，日子要对
    pub fn sanitize(mut self) -> Result<Self, String> {
        self.title = clip(&self.title, MAX_TITLE);
        self.note = clip(&self.note, MAX_NOTE);
        if self.title.is_empty() {
            return Err("写一下这天有什么事吧".into());
        }
        let d = parse_day(&self.day).ok_or("日子不对")?;
        self.day = day_str(d);
        Ok(self)
    }
}

// ---------------- 数据库 ----------------

const COLS: &str = "id, day, title, note, done, done_at, reminded, created_at, updated_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        day: r.get(1)?,
        title: r.get(2)?,
        note: r.get(3)?,
        done: r.get::<_, i64>(4)? != 0,
        done_at: r.get(5)?,
        reminded: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

pub fn get(db: &Connection, id: i64) -> rusqlite::Result<Option<Entry>> {
    db.query_row(
        &format!("SELECT {COLS} FROM agenda WHERE id = ?1"),
        [id],
        row,
    )
    .optional()
}

/// [from, to] 里的（按日子、先记的在前）
pub fn range(db: &Connection, from: &str, to: &str) -> Vec<Entry> {
    let Ok(mut stmt) = db.prepare(&format!(
        "SELECT {COLS} FROM agenda WHERE day >= ?1 AND day <= ?2 ORDER BY day, id"
    )) else {
        return Vec::new();
    };
    stmt.query_map(params![from, to], row)
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
}

/// 新记一条或者改一条（`today` 是计划日）。新记的、改了日子的，按现在还剩几天重新算「说过哪一档」
pub fn save(db: &Connection, e: &Entry, today: NaiveDate, now: i64) -> Result<Entry, String> {
    let e = e.clone().sanitize()?;
    let day = parse_day(&e.day).ok_or("日子不对")?;
    let left = (day - today).num_days();
    let id = if e.id > 0 {
        let old = get(db, e.id)
            .map_err(|x| x.to_string())?
            .ok_or("这一条已经不在了")?;
        let reminded = if old.day == e.day {
            old.reminded
        } else {
            fresh_reminded(left)
        };
        db.execute(
            "UPDATE agenda SET day = ?2, title = ?3, note = ?4, reminded = ?5, updated_at = ?6 WHERE id = ?1",
            params![e.id, e.day, e.title, e.note, reminded, now],
        )
        .map_err(|x| x.to_string())?;
        e.id
    } else {
        db.execute(
            "INSERT INTO agenda(day, title, note, done, reminded, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?5)",
            params![e.day, e.title, e.note, fresh_reminded(left), now],
        )
        .map_err(|x| x.to_string())?;
        db.last_insert_rowid()
    };
    get(db, id)
        .map_err(|x| x.to_string())?
        .ok_or_else(|| "保存失败".into())
}

pub fn delete(db: &Connection, id: i64) -> rusqlite::Result<bool> {
    Ok(db.execute("DELETE FROM agenda WHERE id = ?1", [id])? > 0)
}

/// 标成搞定了 / 改回没搞定
pub fn set_done(db: &Connection, id: i64, done: bool, now: i64) -> rusqlite::Result<bool> {
    Ok(db.execute(
        "UPDATE agenda SET done = ?2, done_at = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, done as i64, done.then_some(now), now],
    )? > 0)
}

/// 今天该说的：（这一条，还剩几天，这一档）
pub fn due_now(db: &Connection, today: NaiveDate) -> Vec<(Entry, i64, i64)> {
    let to = day_str(today + Duration::days(AHEAD[0]));
    range(db, &day_str(today), &to)
        .into_iter()
        .filter_map(|e| {
            let left = (parse_day(&e.day)? - today).num_days();
            let s = due(left, e.reminded, e.done)?;
            Some((e, left, s))
        })
        .collect()
}

fn mark_reminded(db: &Connection, id: i64, stage: i64) {
    if let Err(e) = db.execute(
        "UPDATE agenda SET reminded = ?2 WHERE id = ?1",
        params![id, stage],
    ) {
        log::warn!("记下日历提醒过了失败：{e}");
    }
}

// ---------------- 她说出来 ----------------

/// 用户在不在电脑前（几分钟内动过键盘鼠标）：「打开桌宠的时候 / 回到电脑前」才说
fn user_here() -> bool {
    crate::daily::system_idle_secs() < 300
}

/// 看一次：有该说的就合成一句说出来，并记下这几条说到了哪一档
pub fn check(app: &AppHandle) {
    let st = app.state::<AppState>();
    let day = today();
    let items = due_now(&st.db(), day);
    if items.is_empty() || !user_here() {
        return;
    }
    let text = remind_text(
        &items
            .iter()
            .filter_map(|(e, left, _)| Some((*left, parse_day(&e.day)?, e.title.clone())))
            .collect::<Vec<_>>(),
    );
    let ids: Vec<i64> = items.iter().map(|(e, ..)| e.id).collect();
    let first_day = items
        .iter()
        .map(|(e, ..)| e.day.clone())
        .min()
        .unwrap_or_default();
    use super::supervise::PlanChoice;
    use crate::companion::{Choice, ChoiceAction};
    let mut c = Choice {
        question: text.clone(),
        options: vec!["知道了".into()],
        replies: vec!["嗯，到时候我再提醒你。".into()],
        actions: vec![ChoiceAction::None],
    };
    if ids.len() == 1 {
        c.options.push("已经搞定了".into());
        c.replies
            .push("哦！已经弄好啦，那这条我就不再提了。".into());
        c.actions
            .push(ChoiceAction::Plan(PlanChoice::AgendaDone(ids.clone())));
    }
    c.options.push("打开日历".into());
    c.replies.push("好，帮你打开了。".into());
    c.actions.push(ChoiceAction::Plan(PlanChoice::Open(format!(
        "calendar:{first_day}"
    ))));
    let now = crate::memory::now_secs();
    let mut item =
        crate::proactive::Item::new(crate::proactive::Kind::Reminder, text, now + 6 * 3600);
    item.key = format!("agenda:{}", day_str(day));
    item.choices = Some(c);
    crate::proactive::submit(app, item);
    {
        let db = st.db();
        for (e, _, s) in &items {
            mark_reminded(&db, e.id, *s);
        }
    }
    log::info!("日历：提醒了 {} 条", items.len());
}

/// 气泡里点了「已经搞定了」
pub fn on_done(app: &AppHandle, ids: &[i64]) {
    let st = app.state::<AppState>();
    let now = crate::memory::now_secs();
    {
        let db = st.db();
        for id in ids {
            let _ = set_done(&db, *id, true, now);
        }
    }
    super::supervise::changed(app);
}

/// 后台：启动后等桌宠出来、开场白说完再看，之后每分钟看一次（换天、回到电脑前都会接上）
pub fn spawn(app: AppHandle) {
    if crate::paths::test_quiet() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        loop {
            check(&app);
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// 聊天时给她的背景资料：接下来 7 天日历上的事（关掉「聊天时她知道今天的计划」就不给）
pub fn chat_note(st: &AppState) -> Option<String> {
    if !st.settings().plans.chat_context {
        return None;
    }
    let day = today();
    let list = range(
        &st.db(),
        &day_str(day),
        &day_str(day + Duration::days(AHEAD[0])),
    );
    let parts: Vec<String> = list
        .iter()
        .filter(|e| !e.done)
        .take(8)
        .filter_map(|e| {
            let d = parse_day(&e.day)?;
            Some(format!(
                "{}「{}」",
                when_text((d - day).num_days(), d),
                e.title
            ))
        })
        .collect();
    (!parts.is_empty()).then(|| {
        format!(
            "【日历】用户在日历上记着接下来几天的事：{}。（用户问起最近有什么事、什么截止时可以说；不要每句话都提。）",
            parts.join("；")
        )
    })
}

// ---------------- 命令（计划窗口） ----------------

type CmdResult<T> = Result<T, String>;

/// 日历页：[from, to] 里的
#[tauri::command]
pub fn agenda_range(state: State<'_, AppState>, from: String, to: String) -> CmdResult<Vec<Entry>> {
    let a = parse_day(&from).ok_or("日子不对")?;
    let b = parse_day(&to).ok_or("日子不对")?;
    if b < a || (b - a).num_days() > MAX_RANGE_DAYS {
        return Err("一次取的日子太多了".into());
    }
    Ok(range(&state.db(), &day_str(a), &day_str(b)))
}

/// 记一条 / 改一条
#[tauri::command]
pub fn agenda_save(app: AppHandle, entry: Entry) -> CmdResult<Entry> {
    let saved = save(
        &app.state::<AppState>().db(),
        &entry,
        today(),
        crate::memory::now_secs(),
    )?;
    super::supervise::changed(&app);
    Ok(saved)
}

#[tauri::command]
pub fn agenda_delete(app: AppHandle, id: i64) -> CmdResult<()> {
    delete(&app.state::<AppState>().db(), id).map_err(|e| e.to_string())?;
    super::supervise::changed(&app);
    Ok(())
}

/// 搞定了 / 改回没搞定
#[tauri::command]
pub fn agenda_done(app: AppHandle, id: i64, done: bool) -> CmdResult<()> {
    let ok = set_done(
        &app.state::<AppState>().db(),
        id,
        done,
        crate::memory::now_secs(),
    )
    .map_err(|e| e.to_string())?;
    if !ok {
        return Err("这一条已经不在了".into());
    }
    super::supervise::changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    #[test]
    fn stages_follow_7_3_1_0() {
        assert_eq!(stage(8), None, "还早");
        assert_eq!(stage(7), Some(7));
        assert_eq!(stage(5), Some(7), "7 天那档");
        assert_eq!(stage(3), Some(3));
        assert_eq!(stage(2), Some(3));
        assert_eq!(stage(1), Some(1));
        assert_eq!(stage(0), Some(0));
        assert_eq!(stage(-1), None, "已经过去了");
    }

    /// 从 `start` 天前每天打开一次（`open` 里是打开电脑的那几天还剩几天），返回哪几天说了
    fn simulate(open: &[i64], created_left: Option<i64>) -> Vec<i64> {
        let mut reminded = created_left.and_then(fresh_reminded);
        let mut said = Vec::new();
        for &left in open {
            if let Some(s) = due(left, reminded, false) {
                said.push(left);
                reminded = Some(s);
            }
        }
        said
    }

    #[test]
    fn says_once_per_stage_every_day_open() {
        let every_day: Vec<i64> = (0..=10).rev().collect();
        assert_eq!(simulate(&every_day, None), [7, 3, 1, 0]);
        // 同一天打开好几次：只说一次
        assert_eq!(simulate(&[7, 7, 7, 3, 3, 1, 0, 0], None), [7, 3, 1, 0]);
    }

    #[test]
    fn missed_days_are_not_replayed() {
        // 第 5 天才打开：说「还有 5 天」，之后照常 3、1、当天
        assert_eq!(simulate(&[5, 4, 3, 2, 1, 0], None), [5, 3, 1, 0]);
        // 只在还剩 2 天时打开一次：说一次就够了，不会连着补 7、3 两档
        assert_eq!(simulate(&[2, 0], None), [2, 0]);
        // 当天才第一次打开
        assert_eq!(simulate(&[0], None), [0]);
    }

    #[test]
    fn just_added_is_not_announced_right_away() {
        // 记下时还剩 5 天（7 天那档里）：记的当天不说，到 3 天那档再说
        assert_eq!(simulate(&[5, 4, 3, 2, 1, 0], Some(5)), [3, 1, 0]);
        // 记的就是今天的事：今天不说
        assert_eq!(simulate(&[0], Some(0)), Vec::<i64>::new());
        // 记的是一个月以后的：照常 7、3、1、当天
        let every_day: Vec<i64> = (0..=30).rev().collect();
        assert_eq!(simulate(&every_day, Some(30)), [7, 3, 1, 0]);
    }

    #[test]
    fn done_stops_reminding() {
        assert_eq!(due(3, Some(7), true), None);
        assert_eq!(due(3, Some(7), false), Some(3));
    }

    #[test]
    fn texts() {
        let day = d(11, 25);
        assert_eq!(when_text(0, day), "今天");
        assert_eq!(when_text(1, day), "明天（11月25日）");
        assert_eq!(when_text(2, day), "后天（11月25日）");
        assert_eq!(when_text(7, day), "还有 7 天（11月25日）");
        assert_eq!(
            remind_text(&[(7, d(11, 25), "数据结构作业截止".into())]),
            "日历上记着：还有 7 天（11月25日）「数据结构作业截止」。先记在心里哦。"
        );
        // 几条合成一句，近的在前；有今天的就说「别忘啦」
        assert_eq!(
            remind_text(&[
                (3, d(11, 21), "交实验报告".into()),
                (0, d(11, 18), "线代课停一次".into()),
            ]),
            "日历上记着：今天「线代课停一次」；还有 3 天（11月21日）「交实验报告」。别忘啦。"
        );
    }

    #[test]
    fn sanitize_rules() {
        let e = Entry {
            day: "2026-11-25".into(),
            title: "  数据结构作业截止  ".into(),
            ..Default::default()
        };
        assert_eq!(e.sanitize().unwrap().title, "数据结构作业截止");
        let empty = Entry {
            day: "2026-11-25".into(),
            title: "   ".into(),
            ..Default::default()
        };
        assert!(empty.sanitize().is_err());
        let bad = Entry {
            day: "11.25".into(),
            title: "作业".into(),
            ..Default::default()
        };
        assert!(bad.sanitize().is_err());
    }

    fn entry(day: &str, title: &str) -> Entry {
        Entry {
            day: day.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    #[test]
    fn database_flow() {
        let db = crate::db::open_in_memory().unwrap();
        let today = d(11, 12);
        // 一个月以后、5 天以后、今天
        let far = save(&db, &entry("2026-12-12", "考试"), today, 1).unwrap();
        let near = save(&db, &entry("2026-11-17", "交作业"), today, 1).unwrap();
        let now = save(&db, &entry("2026-11-12", "体育课改到下午"), today, 1).unwrap();
        assert_eq!(far.reminded, None);
        assert_eq!(near.reminded, Some(7), "已经在 7 天内：这一档不再说");
        assert_eq!(now.reminded, Some(0));
        assert!(due_now(&db, today).is_empty(), "刚记下的不马上提醒");
        // 两天后（还剩 3 天）：交作业到了 3 天那档
        let later = d(11, 14);
        let v = due_now(&db, later);
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].0.id, v[0].1, v[0].2), (near.id, 3, 3));
        mark_reminded(&db, near.id, 3);
        assert!(due_now(&db, later).is_empty(), "说过就不再说");
        // 改了日子：重新算
        let moved = save(
            &db,
            &Entry {
                day: "2026-11-30".into(),
                ..near.clone()
            },
            later,
            2,
        )
        .unwrap();
        assert_eq!(moved.reminded, None, "挪到 16 天后：从 7 天那档重新开始");
        // 只改了字：说到哪一档不变
        let renamed = save(
            &db,
            &Entry {
                title: "交数据结构作业".into(),
                ..moved.clone()
            },
            later,
            3,
        )
        .unwrap();
        assert_eq!(renamed.reminded, None);
        assert_eq!(renamed.title, "交数据结构作业");
        // 搞定了：不再提醒
        let soon = d(11, 23);
        assert_eq!(due_now(&db, soon).len(), 1);
        assert!(set_done(&db, moved.id, true, 4).unwrap());
        assert!(due_now(&db, soon).is_empty());
        assert!(get(&db, moved.id).unwrap().unwrap().done_at.is_some());
        // 取一段日子、删掉
        let all = range(&db, "2026-11-01", "2026-12-31");
        assert_eq!(
            all.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            ["体育课改到下午", "交数据结构作业", "考试"]
        );
        assert!(delete(&db, far.id).unwrap());
        assert_eq!(range(&db, "2026-11-01", "2026-12-31").len(), 2);
        // 改一条不在了的
        assert!(
            save(
                &db,
                &Entry {
                    id: 999,
                    ..entry("2026-11-20", "x")
                },
                today,
                5
            )
            .is_err()
        );
    }

    #[test]
    fn chat_note_lists_next_week() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        assert!(chat_note(&st).is_none());
        let t = today();
        for (n, title) in [(0, "体育课改到下午"), (3, "交实验报告"), (20, "考试")] {
            save(
                &st.db(),
                &entry(&day_str(t + Duration::days(n)), title),
                t,
                0,
            )
            .unwrap();
        }
        let note = chat_note(&st).unwrap();
        assert!(note.contains("今天「体育课改到下午」"), "{note}");
        assert!(note.contains("「交实验报告」"), "{note}");
        assert!(!note.contains("考试"), "20 天以后的不放进来：{note}");
        st.update_settings(|s| s.plans.chat_context = false);
        assert!(chat_note(&st).is_none(), "关掉了就不给");
    }
}
