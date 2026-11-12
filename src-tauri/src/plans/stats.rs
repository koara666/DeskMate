//! 统计页的数据：最近 16 周每天做了多少（热力图）、最近 8 周每周按计划分的分钟（堆叠柱）、几个数字、
//! 每个计划的近况、上周回顾一句话。
//!
//! 只有今天和以前的日子落库（以后的只是推算），所以统计只到今天为止。聚合全是纯函数（`build`，输入是数据库取出来的行），
//! 命令 `plan_stats` 只负责取行；连续天数的规则和 `super::streak` 一样（今天还没做完不算断，全跳过 / 没安排的日子不算断也不算数）。

use super::{DayStat, day_str, minutes_text, parse_day, today, weekday_index};
use crate::state::AppState;
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use tauri::State;

/// 热力图看几周（周一对齐，到今天为止）
pub const HEAT_WEEKS: i64 = 16;
/// 堆叠柱看几周
pub const BAR_WEEKS: i64 = 8;
/// 每个计划的小条看几周
pub const PLAN_WEEKS: usize = 4;
/// 完成率看过去多少天（不含今天）
pub const RATE_DAYS: i64 = 28;
/// 堆叠柱最多分几种计划的颜色，再多的合成「其他计划」
pub const MAX_SERIES: usize = 8;

const WEEKDAYS: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

/// 数据库里取出来的一行事项
#[derive(Debug, Clone)]
pub struct Row {
    pub day: String,
    /// None = 单独的事（不属于任何计划）
    pub plan_id: Option<i64>,
    /// 计划的分钟
    pub minutes: u32,
    /// todo / done / skipped
    pub status: String,
    /// 实际用的分钟（做完的才算）
    pub spent: u32,
}

/// 计划的名字、状态，属于哪个计划书
#[derive(Debug, Clone)]
pub struct PlanMeta {
    pub id: i64,
    pub title: String,
    pub emoji: String,
    pub status: String,
    pub goal: Option<(i64, String)>,
}

/// 热力图的一格：某一天
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HeatDay {
    pub day: String,
    /// 这天做完的分钟
    pub minutes: u32,
    /// 做完几项
    pub done: u32,
    /// 一共几项（含跳过的）
    pub total: u32,
}

/// 堆叠柱里的一种颜色：`p<计划 id>` / `other`（颜色用完后合起来的计划）/ `loose`（随手的事）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Series {
    pub key: String,
    /// 图例上写的名字（计划带图标）
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Part {
    pub key: String,
    pub minutes: u32,
}

/// 一周（周一开头）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WeekBar {
    pub start: String,
    pub minutes: u32,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Numbers {
    /// 本周（周一到今天）做完的分钟
    pub week_minutes: u32,
    pub last_week_minutes: u32,
    /// 过去 28 天（不含今天）的完成率（百分数）；那段时间一件事都没安排时为空
    pub rate: Option<u32>,
    pub rate_done: u32,
    pub rate_total: u32,
    pub streak: u32,
    pub best_streak: u32,
}

/// 一个计划（进行中 / 暂停的）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlanStat {
    pub plan_id: i64,
    pub title: String,
    pub emoji: String,
    pub status: String,
    /// 是计划书里的一条线时，属于哪个计划书
    pub goal_id: Option<i64>,
    pub goal_title: Option<String>,
    /// 最近 4 周每周做完的分钟，旧的在前
    pub weeks: Vec<u32>,
    pub total_minutes: u32,
    pub done_items: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Stats {
    pub today: String,
    /// 热力图第一格的日子（周一）；`heat` 从这天起一天一格，到今天为止，第 i 格在第 i / 7 列、第 i % 7 行
    pub heat_start: String,
    pub heat: Vec<HeatDay>,
    pub series: Vec<Series>,
    /// 最近 8 周，旧的在前，最后一个是本周（到今天为止）
    pub weeks: Vec<WeekBar>,
    pub numbers: Numbers,
    pub plans: Vec<PlanStat>,
    /// 上周回顾一句话
    pub review: String,
}

/// 某一天的汇总
#[derive(Debug, Default, Clone, Copy)]
struct DayAgg {
    planned: u32,
    done_minutes: u32,
    tasks: u32,
    done: u32,
    todo: u32,
}

impl DayAgg {
    fn add(&mut self, r: &Row) {
        self.planned += r.minutes;
        self.tasks += 1;
        match r.status.as_str() {
            "done" => {
                self.done += 1;
                self.done_minutes += r.spent;
            }
            "todo" => self.todo += 1,
            _ => {}
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct PlanAgg {
    weeks: [u32; PLAN_WEEKS],
    total: u32,
    done: u32,
}

fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Duration::days(weekday_index(d) as i64)
}

/// 从旧到新走一遍：(现在连续几天, 最长连续几天)。规则和 `super::streak` 一样
fn runs(asc: &[DayStat], today: &str) -> (u32, u32) {
    let (mut n, mut best) = (0, 0);
    for s in asc {
        if s.tasks_todo == 0 && s.tasks_done > 0 {
            n += 1;
            best = best.max(n);
        } else if s.day == today || (s.tasks_done == 0 && s.tasks_todo == 0) {
            // 今天还没做完、全跳过：不算断也不算数
        } else {
            n = 0;
        }
    }
    (n, best)
}

/// 上周做得最多的那一块
enum Top {
    Plan(String),
    Loose,
}

fn clip_title(s: &str) -> String {
    if s.chars().count() > 12 {
        format!("{}…", s.chars().take(12).collect::<String>())
    } else {
        s.to_string()
    }
}

/// 上周回顾：做了多久、和前一周比、哪天最多、哪个计划最多；没有记录时说一句温和的话
fn review_text(
    last: u32,
    prev: u32,
    by_weekday: &[u32; 7],
    top: Option<Top>,
    sources: usize,
) -> String {
    if last == 0 {
        return if prev == 0 {
            "上周还没有做完的记录，这周从一件小事开始就好。".into()
        } else {
            format!(
                "上周没有做完的记录（前一周做了 {}），这周先从一件小事开始，慢慢找回节奏。",
                minutes_text(prev)
            )
        };
    }
    let mut s = format!("上周做了 {}", minutes_text(last));
    if prev > 0 {
        match last.cmp(&prev) {
            std::cmp::Ordering::Greater => {
                s += &format!("，比前一周多 {}", minutes_text(last - prev));
            }
            std::cmp::Ordering::Less => {
                s += &format!("，比前一周少 {}", minutes_text(prev - last));
            }
            std::cmp::Ordering::Equal => s += "，和前一周一样多",
        }
    }
    let (mut best_i, mut best) = (0, 0);
    for (i, m) in by_weekday.iter().enumerate() {
        if *m > best {
            (best_i, best) = (i, *m);
        }
    }
    if best > 0 {
        s += &format!(
            "；做得最多的是{}（{}）",
            WEEKDAYS[best_i],
            minutes_text(best)
        );
    }
    // 只有一块的时候说「做得最多」没有意思
    if sources > 1 {
        match top {
            Some(Top::Plan(t)) => s += &format!("；「{}」做得最多", clip_title(&t)),
            Some(Top::Loose) => s += "；随手的事占得最多",
            None => {}
        }
    }
    s.push('。');
    s
}

/// 把数据库里的行算成统计页要的东西。`plans` 是全部计划（顺序随意，`plans` 字段按这个顺序给）。
pub fn build(rows: &[Row], plans: &[PlanMeta], today: NaiveDate) -> Stats {
    let this_monday = monday_of(today);
    let known: HashMap<i64, &PlanMeta> = plans.iter().map(|p| (p.id, p)).collect();
    // 只看今天和以前的（以后的不会落库，万一有也不算）
    let rows: Vec<(NaiveDate, &Row)> = rows
        .iter()
        .filter_map(|r| parse_day(&r.day).filter(|d| *d <= today).map(|d| (d, r)))
        .collect();
    let mut by_day: BTreeMap<NaiveDate, DayAgg> = BTreeMap::new();
    for (d, r) in &rows {
        by_day.entry(*d).or_default().add(r);
    }
    let minutes_between = |from: NaiveDate, to: NaiveDate| -> u32 {
        by_day.range(from..=to).map(|(_, a)| a.done_minutes).sum()
    };

    // 热力图
    let heat_start = this_monday - Duration::days(7 * (HEAT_WEEKS - 1));
    let heat: Vec<HeatDay> = (0..)
        .map(|i| heat_start + Duration::days(i))
        .take_while(|d| *d <= today)
        .map(|d| {
            let a = by_day.get(&d).copied().unwrap_or_default();
            HeatDay {
                day: day_str(d),
                minutes: a.done_minutes,
                done: a.done,
                total: a.tasks,
            }
        })
        .collect();

    // 堆叠柱：先定哪些计划有自己的颜色（做得最多的 8 个，按 id 排固定顺序，颜色不会因为排名变化而换），其余的合成「其他计划」
    let bar_start = this_monday - Duration::days(7 * (BAR_WEEKS - 1));
    let mut totals: BTreeMap<i64, u32> = BTreeMap::new();
    for (d, r) in &rows {
        if *d >= bar_start
            && r.status == "done"
            && r.spent > 0
            && let Some(id) = r.plan_id.filter(|id| known.contains_key(id))
        {
            *totals.entry(id).or_default() += r.spent;
        }
    }
    let mut ranked: Vec<(i64, u32)> = totals.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut slots: Vec<i64> = ranked.iter().take(MAX_SERIES).map(|x| x.0).collect();
    slots.sort_unstable();
    let key_of = |plan_id: Option<i64>| -> String {
        match plan_id {
            None => "loose".into(),
            Some(id) if slots.contains(&id) => format!("p{id}"),
            Some(_) => "other".into(),
        }
    };
    let mut cell: HashMap<(usize, String), u32> = HashMap::new();
    for (d, r) in &rows {
        if *d >= bar_start && r.status == "done" && r.spent > 0 {
            let week = ((*d - bar_start).num_days() / 7) as usize;
            *cell.entry((week, key_of(r.plan_id))).or_default() += r.spent;
        }
    }
    let mut series: Vec<Series> = slots
        .iter()
        .filter_map(|id| known.get(id))
        .map(|p| Series {
            key: format!("p{}", p.id),
            label: format!("{} {}", p.emoji, p.title).trim().to_string(),
        })
        .collect();
    if cell.keys().any(|(_, k)| k == "other") {
        series.push(Series {
            key: "other".into(),
            label: "其他计划".into(),
        });
    }
    if cell.keys().any(|(_, k)| k == "loose") {
        series.push(Series {
            key: "loose".into(),
            label: "随手的事".into(),
        });
    }
    let weeks: Vec<WeekBar> = (0..BAR_WEEKS as usize)
        .map(|w| {
            let parts: Vec<Part> = series
                .iter()
                .filter_map(|s| {
                    let m = cell.get(&(w, s.key.clone())).copied().unwrap_or(0);
                    (m > 0).then(|| Part {
                        key: s.key.clone(),
                        minutes: m,
                    })
                })
                .collect();
            WeekBar {
                start: day_str(bar_start + Duration::days(7 * w as i64)),
                minutes: parts.iter().map(|p| p.minutes).sum(),
                parts,
            }
        })
        .collect();

    // 数字
    let last_monday = this_monday - Duration::days(7);
    let week_minutes = minutes_between(this_monday, today);
    let last_week_minutes = minutes_between(last_monday, this_monday - Duration::days(1));
    let prev_week_minutes = minutes_between(
        last_monday - Duration::days(7),
        last_monday - Duration::days(1),
    );
    let (mut rate_done, mut rate_total) = (0, 0);
    for (d, r) in &rows {
        if *d >= today - Duration::days(RATE_DAYS) && *d < today {
            rate_total += 1;
            if r.status == "done" {
                rate_done += 1;
            }
        }
    }
    let day_stats: Vec<DayStat> = by_day
        .iter()
        .map(|(d, a)| DayStat {
            day: day_str(*d),
            planned: a.planned,
            done: a.done_minutes,
            tasks: a.tasks,
            tasks_done: a.done,
            tasks_todo: a.todo,
        })
        .collect();
    let today_text = day_str(today);
    let newest_first: Vec<DayStat> = day_stats.iter().rev().cloned().collect();
    let streak = super::streak(&newest_first, &today_text);
    let (_, best) = runs(&day_stats, &today_text);
    let numbers = Numbers {
        week_minutes,
        last_week_minutes,
        rate: (rate_total > 0).then(|| (rate_done * 100 + rate_total / 2) / rate_total),
        rate_done,
        rate_total,
        streak,
        best_streak: best.max(streak),
    };

    // 每个计划
    let mut per_plan: HashMap<i64, PlanAgg> = HashMap::new();
    for (d, r) in &rows {
        if r.status != "done" {
            continue;
        }
        let Some(id) = r.plan_id else { continue };
        let a = per_plan.entry(id).or_default();
        a.total += r.spent;
        a.done += 1;
        let ago = (this_monday - monday_of(*d)).num_days() / 7;
        if (0..PLAN_WEEKS as i64).contains(&ago) {
            a.weeks[PLAN_WEEKS - 1 - ago as usize] += r.spent;
        }
    }
    let plan_stats: Vec<PlanStat> = plans
        .iter()
        .filter(|p| p.status == "active" || p.status == "paused")
        .map(|p| {
            let a = per_plan.get(&p.id).copied().unwrap_or_default();
            PlanStat {
                plan_id: p.id,
                title: p.title.clone(),
                emoji: p.emoji.clone(),
                status: p.status.clone(),
                goal_id: p.goal.as_ref().map(|g| g.0),
                goal_title: p.goal.as_ref().map(|g| g.1.clone()),
                weeks: a.weeks.to_vec(),
                total_minutes: a.total,
                done_items: a.done,
            }
        })
        .collect();

    // 上周回顾
    let mut by_weekday = [0u32; 7];
    let mut by_source: BTreeMap<Option<i64>, u32> = BTreeMap::new();
    for (d, r) in &rows {
        if *d >= last_monday && *d < this_monday && r.status == "done" && r.spent > 0 {
            by_weekday[weekday_index(*d)] += r.spent;
            let source = r.plan_id.filter(|id| known.contains_key(id));
            *by_source.entry(source).or_default() += r.spent;
        }
    }
    // 单独的事排在最后（BTreeMap 里 None 最小，所以先取有计划的）
    let mut top: Option<(Option<i64>, u32)> = None;
    for (k, m) in by_source.iter().filter(|(k, _)| k.is_some()) {
        if top.is_none_or(|t| *m > t.1) {
            top = Some((*k, *m));
        }
    }
    if let Some(m) = by_source.get(&None)
        && top.is_none_or(|t| *m > t.1)
    {
        top = Some((None, *m));
    }
    let top = top.map(|(k, _)| match k.and_then(|id| known.get(&id)) {
        Some(p) => Top::Plan(p.title.clone()),
        None => Top::Loose,
    });
    let review = review_text(
        last_week_minutes,
        prev_week_minutes,
        &by_weekday,
        top,
        by_source.len(),
    );

    Stats {
        today: today_text,
        heat_start: day_str(heat_start),
        heat,
        series,
        weeks,
        numbers,
        plans: plan_stats,
        review,
    }
}

/// 从数据库取出行和计划，交给 `build`
pub fn load(db: &Connection, today: NaiveDate) -> Stats {
    let rows: Vec<Row> = db
        .prepare("SELECT day, plan_id, minutes, status, spent FROM plan_tasks WHERE day <= ?1")
        .and_then(|mut stmt| {
            stmt.query_map([day_str(today)], |r| {
                Ok(Row {
                    day: r.get(0)?,
                    plan_id: r.get(1)?,
                    minutes: r.get::<_, i64>(2)?.max(0) as u32,
                    status: r.get(3)?,
                    spent: r.get::<_, i64>(4)?.max(0) as u32,
                })
            })
            .map(|it| it.flatten().collect())
        })
        .unwrap_or_default();
    let goals: HashMap<i64, String> = super::goal::list_goals(db)
        .unwrap_or_default()
        .into_iter()
        .map(|g| (g.id, g.title))
        .collect();
    let plans: Vec<PlanMeta> = super::list_plans(db)
        .unwrap_or_default()
        .into_iter()
        .map(|p| PlanMeta {
            goal: p
                .goal_id
                .and_then(|g| goals.get(&g).map(|t| (g, t.clone()))),
            id: p.id,
            title: p.title,
            emoji: p.emoji,
            status: p.status,
        })
        .collect();
    build(&rows, &plans, today)
}

/// 统计页：最近几周做了多少
#[tauri::command]
pub fn plan_stats(state: State<'_, AppState>) -> Stats {
    load(&state.db(), today())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::plans::{Plan, Task, save_plan, save_task};

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    /// 2026-11-09 是周一
    const MON: u32 = 9;

    fn row(day: NaiveDate, plan: Option<i64>, status: &str, spent: u32) -> Row {
        Row {
            day: day_str(day),
            plan_id: plan,
            minutes: 30,
            status: status.into(),
            spent,
        }
    }

    fn done(day: NaiveDate, plan: Option<i64>, spent: u32) -> Row {
        row(day, plan, "done", spent)
    }

    fn meta(id: i64, title: &str, status: &str) -> PlanMeta {
        PlanMeta {
            id,
            title: title.into(),
            emoji: "📚".into(),
            status: status.into(),
            goal: None,
        }
    }

    #[test]
    fn heat_aligns_to_monday_and_stops_at_today() {
        // 今天是周一：本周只有一格，往前 15 周整整齐齐
        let s = build(&[], &[], d(11, MON));
        assert_eq!(s.heat_start, "2026-07-27");
        assert_eq!(s.heat.len(), 15 * 7 + 1);
        assert_eq!(s.heat.last().unwrap().day, "2026-11-09");
        assert_eq!(parse_day(&s.heat[0].day).map(weekday_index), Some(0));
        // 今天是周日：本周七格都在
        let s = build(&[], &[], d(11, 15));
        assert_eq!(s.heat_start, "2026-07-27");
        assert_eq!(s.heat.len(), 16 * 7);
        assert_eq!(s.heat.last().unwrap().day, "2026-11-15");
        // 周三：本周三格
        let s = build(&[], &[], d(11, 11));
        assert_eq!(s.heat.len(), 15 * 7 + 3);
        // 柱子：8 周，最后一周从本周一开始
        assert_eq!(s.weeks.len(), 8);
        assert_eq!(s.weeks[7].start, "2026-11-09");
        assert_eq!(s.weeks[0].start, "2026-09-21");
    }

    #[test]
    fn week_boundaries_follow_monday() {
        // 今天周日：周一到周日都算本周；上周日（11-8）算上周
        let today = d(11, 15);
        let rows = [
            done(d(11, 8), None, 50),
            done(d(11, 9), None, 20),
            done(d(11, 15), None, 10),
            done(d(11, 2), None, 5),
            done(d(11, 1), None, 7),
        ];
        let s = build(&rows, &[], today);
        assert_eq!(s.numbers.week_minutes, 30);
        assert_eq!(s.numbers.last_week_minutes, 55);
        assert_eq!(s.weeks[7].minutes, 30);
        assert_eq!(s.weeks[6].minutes, 55);
        assert_eq!(s.weeks[5].minutes, 7);
        // 今天周一：本周只有今天；今天以后的日子（万一有）不算
        let s = build(&rows, &[], d(11, 9));
        assert_eq!(s.numbers.week_minutes, 20);
        assert_eq!(s.numbers.last_week_minutes, 55);
        assert_eq!(s.heat.last().unwrap().minutes, 20);
    }

    #[test]
    fn no_data_is_gentle() {
        let s = build(&[], &[meta(1, "线代", "active")], d(11, MON));
        assert!(s.heat.iter().all(|h| h.minutes == 0 && h.total == 0));
        assert!(s.series.is_empty());
        assert!(s.weeks.iter().all(|w| w.minutes == 0 && w.parts.is_empty()));
        assert_eq!(s.numbers.rate, None);
        assert_eq!(
            (
                s.numbers.streak,
                s.numbers.best_streak,
                s.numbers.week_minutes
            ),
            (0, 0, 0)
        );
        assert_eq!(s.review, "上周还没有做完的记录，这周从一件小事开始就好。");
        // 进行中的计划照样列出来，只是全是零
        assert_eq!(s.plans.len(), 1);
        assert_eq!(s.plans[0].weeks, [0, 0, 0, 0]);
        assert_eq!((s.plans[0].total_minutes, s.plans[0].done_items), (0, 0));
    }

    #[test]
    fn only_loose_tasks() {
        let today = d(11, 11);
        let rows = [
            done(d(11, 3), None, 40),
            done(d(11, 4), None, 20),
            row(d(11, 5), None, "skipped", 0),
        ];
        let s = build(&rows, &[], today);
        assert_eq!(
            s.series,
            [Series {
                key: "loose".into(),
                label: "随手的事".into()
            }]
        );
        let last = &s.weeks[6];
        assert_eq!(last.start, "2026-11-02");
        assert_eq!(
            last.parts,
            [Part {
                key: "loose".into(),
                minutes: 60
            }]
        );
        assert!(s.plans.is_empty());
        // 上周全是随手的事：只有一块，不说「哪个最多」
        assert_eq!(s.review, "上周做了 1 小时；做得最多的是周二（40 分钟）。");
    }

    #[test]
    fn skipped_and_undone_count_in_rate() {
        let today = d(11, 30);
        let rows = [
            done(d(11, 29), Some(1), 30),
            done(d(11, 28), Some(1), 30),
            done(d(11, 27), None, 30),
            row(d(11, 27), Some(1), "skipped", 0),
            row(d(11, 26), Some(1), "todo", 0),
            // 今天不算，29 天前也不算
            done(d(11, 30), Some(1), 30),
            row(d(11, 1), Some(1), "todo", 0),
        ];
        let s = build(&rows, &[meta(1, "线代", "active")], today);
        assert_eq!((s.numbers.rate_done, s.numbers.rate_total), (3, 5));
        assert_eq!(s.numbers.rate, Some(60));
        // 刚好 28 天前（11-2）算在里面
        let rows = [done(d(11, 2), None, 10), row(d(11, 2), None, "skipped", 0)];
        let s = build(&rows, &[], today);
        assert_eq!((s.numbers.rate_done, s.numbers.rate_total), (1, 2));
        assert_eq!(s.numbers.rate, Some(50));
    }

    #[test]
    fn heat_cell_counts_all_items() {
        let today = d(11, 11);
        let rows = [
            done(d(11, 10), Some(1), 45),
            done(d(11, 10), Some(2), 35),
            row(d(11, 10), Some(2), "skipped", 0),
            row(d(11, 10), None, "todo", 0),
        ];
        let s = build(&rows, &[], today);
        let h = s.heat.iter().find(|h| h.day == "2026-11-10").unwrap();
        assert_eq!((h.minutes, h.done, h.total), (80, 2, 4));
    }

    #[test]
    fn streaks_follow_the_streak_rules() {
        let today = d(11, 11);
        let m =
            |day: u32, st: &str| row(d(11, day), Some(1), st, if st == "done" { 30 } else { 0 });
        // 11-1..3 做完；4 全跳过（不算断）；5..6 做完；7 有没做的（断）；8..10 做完；今天还没做完（不算断）
        let rows = vec![
            m(1, "done"),
            m(2, "done"),
            m(3, "done"),
            m(4, "skipped"),
            m(5, "done"),
            m(6, "done"),
            m(7, "done"),
            m(7, "todo"),
            m(8, "done"),
            m(9, "done"),
            m(10, "done"),
            m(11, "todo"),
        ];
        let s = build(&rows, &[], today);
        assert_eq!(s.numbers.streak, 3);
        assert_eq!(s.numbers.best_streak, 5);
        // 今天做完了：连续天数加一
        let mut rows = rows;
        rows.pop();
        rows.push(m(11, "done"));
        let s = build(&rows, &[], today);
        assert_eq!((s.numbers.streak, s.numbers.best_streak), (4, 5));
        // 当前连续比以前的长：最长就是现在的
        let mut rows: Vec<Row> = (1..=11).map(|x| m(x, "done")).collect();
        rows.push(m(1, "todo"));
        let s = build(&rows, &[], today);
        assert_eq!((s.numbers.streak, s.numbers.best_streak), (10, 10));
    }

    /// 和数据库里真正用的 `streak`（连续天数）算出来的一样
    #[test]
    fn streak_matches_the_real_one() {
        let db = db::open_in_memory().unwrap();
        let pid = save_plan(
            &db,
            &Plan {
                title: "线代".into(),
                start_day: "2026-10-01".into(),
                ..Default::default()
            }
            .sanitize(d(10, 1))
            .unwrap(),
            1,
        )
        .unwrap();
        let add = |day: NaiveDate, plan: Option<i64>, status: &str| {
            save_task(
                &db,
                &Task {
                    plan_id: plan,
                    day: day_str(day),
                    title: "x".into(),
                    minutes: 30,
                    status: status.into(),
                    spent: if status == "done" { 30 } else { 0 },
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        };
        // 一个乱一点的月份：做完、全跳过、半做完、什么都没做、没安排
        let plan = [
            (1, "done"),
            (2, "done"),
            (3, "skipped"),
            (4, "done"),
            (4, "skipped"),
            (6, "done"),
            (6, "todo"),
            (7, "todo"),
            (8, "done"),
            (9, "done"),
            (11, "done"),
            (12, "done"),
            (13, "skipped"),
        ];
        for (day, st) in plan {
            add(d(10, day), Some(pid), st);
        }
        add(d(10, 12), None, "done");
        // 每个「今天」都对一遍：今天做完 / 没做完 / 没有安排
        for today_day in 1..=15 {
            let today = d(10, today_day);
            let real = crate::plans::streak(
                &crate::plans::day_stats(&db, None, &day_str(today), 400),
                &day_str(today),
            );
            let s = load(&db, today);
            assert_eq!(s.numbers.streak, real, "今天 10-{today_day:02}");
            assert!(s.numbers.best_streak >= real);
            // 从旧到新走一遍的算法也一样
            let mut asc = crate::plans::day_stats(&db, None, &day_str(today), 400);
            asc.reverse();
            assert_eq!(
                runs(&asc, &day_str(today)).0,
                real,
                "今天 10-{today_day:02}"
            );
        }
    }

    #[test]
    fn week_review_sentence() {
        let today = d(11, 11); // 周三；上周 = 11-2 .. 11-8，前一周 = 10-26 .. 11-1
        let plans = [meta(1, "线代", "active"), meta(2, "英语", "active")];
        let rows = [
            // 上周：周一 60（线代）、周三 120（线代 90 + 英语 30）、周五 80（随手 80）= 260
            done(d(11, 2), Some(1), 60),
            done(d(11, 4), Some(1), 90),
            done(d(11, 4), Some(2), 30),
            done(d(11, 6), None, 80),
            // 前一周 220
            done(d(10, 27), Some(2), 220),
        ];
        let s = build(&rows, &plans, today);
        assert_eq!(
            s.review,
            "上周做了 4 小时 20 分钟，比前一周多 40 分钟；做得最多的是周三（2 小时）；「线代」做得最多。"
        );
        // 比前一周少
        let rows = [done(d(11, 2), Some(1), 60), done(d(10, 27), Some(1), 150)];
        assert_eq!(
            build(&rows, &plans, today).review,
            "上周做了 1 小时，比前一周少 1 小时 30 分钟；做得最多的是周一（1 小时）。"
        );
        // 和前一周一样多
        let rows = [done(d(11, 2), Some(1), 60), done(d(10, 27), Some(1), 60)];
        assert_eq!(
            build(&rows, &plans, today).review,
            "上周做了 1 小时，和前一周一样多；做得最多的是周一（1 小时）。"
        );
        // 前一周没有记录就不比；做得最多的是随手的事
        let rows = [done(d(11, 2), Some(1), 60), done(d(11, 3), None, 90)];
        assert_eq!(
            build(&rows, &plans, today).review,
            "上周做了 2 小时 30 分钟；做得最多的是周二（1 小时 30 分钟）；随手的事占得最多。"
        );
        // 上周没做，前一周做了
        let rows = [done(d(10, 27), Some(1), 60)];
        assert_eq!(
            build(&rows, &plans, today).review,
            "上周没有做完的记录（前一周做了 1 小时），这周先从一件小事开始，慢慢找回节奏。"
        );
        // 这周做的不算上周
        let rows = [done(d(11, 10), Some(1), 60)];
        assert_eq!(
            build(&rows, &plans, today).review,
            "上周还没有做完的记录，这周从一件小事开始就好。"
        );
    }

    #[test]
    fn stacked_bars_split_by_plan_and_fold_the_tail() {
        let today = d(11, MON);
        // 10 个计划，每个在本周做一点：做得最多的 8 个有自己的颜色，剩下两个合成「其他计划」
        let plans: Vec<PlanMeta> = (1..=10)
            .map(|i| meta(i, &format!("计划{i}"), "active"))
            .collect();
        let mut rows: Vec<Row> = (1..=10)
            .map(|i| done(today, Some(i), 10 + i as u32))
            .collect();
        rows.push(done(d(11, 2), None, 25));
        rows.push(done(d(11, 2), Some(99), 5)); // 已经不在的计划也并进「其他」
        let s = build(&rows, &plans, today);
        // 最少的两个（1、2）并进「其他」，剩下的按 id 排
        let keys: Vec<&str> = s.series.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "p3", "p4", "p5", "p6", "p7", "p8", "p9", "p10", "other", "loose"
            ]
        );
        assert_eq!(s.series[0].label, "📚 计划3");
        let now = &s.weeks[7];
        assert_eq!(now.minutes, (1..=10).map(|i| 10 + i).sum::<u32>());
        let other = now.parts.iter().find(|p| p.key == "other").unwrap();
        assert_eq!(other.minutes, 11 + 12);
        // 上周：随手的事 25 + 已删的计划 5
        let prev = &s.weeks[6];
        assert_eq!(
            prev.parts,
            [
                Part {
                    key: "other".into(),
                    minutes: 5
                },
                Part {
                    key: "loose".into(),
                    minutes: 25
                }
            ]
        );
        // 柱子上的段加起来就是这一周
        for w in &s.weeks {
            assert_eq!(w.minutes, w.parts.iter().map(|p| p.minutes).sum::<u32>());
        }
    }

    #[test]
    fn plan_rows_show_recent_weeks_and_totals() {
        let today = d(11, 11);
        let mut lines = meta(3, "代数线", "paused");
        lines.goal = Some((7, "考证准备".into()));
        let plans = [meta(1, "线代", "active"), meta(2, "旧计划", "done"), lines];
        let rows = [
            done(d(11, 10), Some(1), 40), // 本周
            done(d(11, 3), Some(1), 30),  // 上周
            done(d(10, 14), Some(1), 20), // 4 周前：不在小条里，但算总数
            row(d(11, 10), Some(1), "todo", 0),
            done(d(11, 4), Some(2), 99),
            done(d(11, 5), Some(3), 15),
        ];
        let s = build(&rows, &plans, today);
        assert_eq!(s.plans.len(), 2, "做完的计划不列");
        let p = &s.plans[0];
        assert_eq!(p.weeks, [0, 0, 30, 40]);
        assert_eq!((p.total_minutes, p.done_items), (90, 3));
        let q = &s.plans[1];
        assert_eq!(
            (q.goal_id, q.goal_title.as_deref()),
            (Some(7), Some("考证准备"))
        );
        assert_eq!(q.weeks, [0, 0, 15, 0]);
        assert_eq!(q.status, "paused");
    }

    #[test]
    fn stats_page_can_be_opened_from_elsewhere() {
        assert!(crate::desktop::valid_plans_view("stats"));
        assert!(!crate::desktop::valid_plans_view("stat"));
    }

    #[test]
    fn load_reads_the_database() {
        let db = db::open_in_memory().unwrap();
        let pid = save_plan(
            &db,
            &Plan {
                title: "线代".into(),
                emoji: "📐".into(),
                start_day: "2026-11-02".into(),
                ..Default::default()
            }
            .sanitize(d(11, 2))
            .unwrap(),
            1,
        )
        .unwrap();
        for (day, plan, st, spent) in [
            (d(11, 10), Some(pid), "done", 45),
            (d(11, 10), None, "done", 15),
            (d(11, 10), Some(pid), "todo", 0),
            (d(11, 11), Some(pid), "done", 30),
        ] {
            save_task(
                &db,
                &Task {
                    plan_id: plan,
                    day: day_str(day),
                    title: "x".into(),
                    minutes: 30,
                    status: st.into(),
                    spent,
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        }
        // 今天是 11-10：11-11 的（万一有）不算
        let s = load(&db, d(11, 10));
        assert_eq!(s.numbers.week_minutes, 60);
        assert_eq!(s.plans[0].title, "线代");
        assert_eq!(s.plans[0].total_minutes, 45);
        assert_eq!(s.series[0].label, "📐 线代");
        let h = s.heat.last().unwrap();
        assert_eq!(
            (h.day.as_str(), h.minutes, h.done, h.total),
            ("2026-11-10", 60, 2, 3)
        );
    }
}
