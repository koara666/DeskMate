//! 计划书的排程器（纯函数，不碰数据库和时钟）：好几条并行的线共用每天的时间。
//!
//! - 每天能用多少：这天所在阶段自己的每周分钟（阶段没写就用计划书的）× 缓冲期的比例；请假的日子不排。
//! - 先排每周固定的事（routine 线，按它自己的每周分钟），再排钉在这天的条目（不看份额、不顺延），
//!   剩下的时间按份额分给各条内容线。
//! - 份额：按「这一段（阶段）到目前为止分到的时间」和份额比，谁欠得多先给谁。一天分成 1~3 块（每块大约 50 分钟），
//!   所以一天只做一两条线，几天下来按份额交替；一条线这天没那么多能做的，剩下的时间给下一条。
//! - 每条线里截止早的先排（没写截止的按计划书的截止），一样早的按原来的顺序；没到「最早开始」的先不排，
//!   所有线都没有能排的时候才提前做后面的内容。
//! - 来不及：某条内容到它的截止还没排完，记在那条线上（差多少分钟、最早是哪天的截止）。

use super::schedule::{self, Left, MIN_CHUNK, Shape, Slot, Used};
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use std::collections::HashMap;

/// 一天最多分成几块（也就是最多做几条内容线）
pub const MAX_BLOCKS: u32 = 3;
/// 一块大约多少分钟
pub const BLOCK: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseShape {
    pub id: i64,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// 这个阶段周一到周日每天能用的分钟（None = 用计划书的）
    pub week: Option<[u32; 7]>,
    /// 各条线的份额（空 = 用每条线自己的默认份额）
    pub shares: Vec<(i64, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferShape {
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// 这几天按平时的百分之几排（0 = 完全不排）
    pub percent: u32,
}

/// 计划书的形状
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalShape {
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    pub week: [u32; 7],
    /// 按开始日期排好
    pub phases: Vec<PhaseShape>,
    pub buffers: Vec<BufferShape>,
    pub off: Vec<NaiveDate>,
}

fn weekday(d: NaiveDate) -> usize {
    d.weekday().num_days_from_monday() as usize
}

impl GoalShape {
    pub fn phase_at(&self, day: NaiveDate) -> Option<&PhaseShape> {
        self.phases.iter().find(|p| p.start <= day && day <= p.end)
    }

    /// 这天按平时的百分之几排（几个缓冲期重叠取最小；100 = 正常）
    pub fn buffer_percent(&self, day: NaiveDate) -> u32 {
        self.buffers
            .iter()
            .filter(|b| b.start <= day && day <= b.end)
            .map(|b| b.percent.min(100))
            .min()
            .unwrap_or(100)
    }

    /// 这天一共能用多少分钟（所有线加起来）
    pub fn budget(&self, day: NaiveDate) -> u32 {
        if day < self.start || self.off.contains(&day) {
            return 0;
        }
        let w = self.phase_at(day).and_then(|p| p.week).unwrap_or(self.week);
        w[weekday(day)].min(720) * self.buffer_percent(day) / 100 / 5 * 5
    }

    /// 份额从哪天开始记账：这天所在阶段的开始；不在阶段里就是上一个阶段结束的第二天（或者计划书开始）
    pub fn segment_start(&self, day: NaiveDate) -> NaiveDate {
        if let Some(p) = self.phase_at(day) {
            return p.start.max(self.start);
        }
        self.phases
            .iter()
            .filter(|p| p.end < day)
            .map(|p| p.end + Duration::days(1))
            .max()
            .unwrap_or(self.start)
            .max(self.start)
    }

    /// 这天每条线的份额（`defaults`：每条线自己的默认份额）
    fn weight(&self, day: NaiveDate, lane: i64, default: u32) -> u32 {
        match self.phase_at(day) {
            Some(p) if !p.shares.is_empty() => p
                .shares
                .iter()
                .find(|(id, _)| *id == lane)
                .map_or(0, |(_, w)| *w),
            _ => default,
        }
    }
}

/// 内容线里还没做完的一条
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneItem {
    pub left: Left,
    /// 最早哪天开始
    pub after: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    /// 钉在哪天
    pub pin: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
pub enum LaneWork {
    /// 按内容推进的线
    Content(Vec<LaneItem>),
    /// 每周固定的事：按自己的形状（每周哪几天、多久、到哪天为止）
    Routine {
        shape: Shape,
        title: String,
        items: Vec<(i64, String, u32)>,
        rot: usize,
    },
}

#[derive(Debug, Clone)]
pub struct Lane {
    pub id: i64,
    /// 默认份额（阶段没写份额时用）
    pub share: u32,
    pub work: LaneWork,
    /// 从 `segment_start(from)` 到 `from` 前一天已经分到的分钟
    pub alloc: u64,
    /// 已经落库的安排在某几天占掉的分钟
    pub used: Used,
}

/// 某条线来不及
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaneShort {
    pub lane: i64,
    pub minutes: u32,
    /// 最早来不及的是哪天的截止
    pub by: NaiveDate,
}

#[derive(Debug, Clone, Default)]
pub struct GoalProjection {
    /// 每天（有安排的日子）排了什么：（线，那一项）
    pub days: Vec<(NaiveDate, Vec<(i64, Slot)>)>,
    /// 内容线预计哪天做完（推算范围内做不完的没有）
    pub finish: HashMap<i64, NaiveDate>,
    /// 每条内容预计哪天做完
    pub item_finish: HashMap<i64, NaiveDate>,
    pub short: Vec<LaneShort>,
    /// 钉在已经过去的日子、还没做的：（线，条目）
    pub missed: Vec<(i64, i64)>,
}

impl GoalProjection {
    /// 某条线自己的推算（给原来按单个计划显示的地方用）
    pub fn lane(&self, id: i64) -> schedule::Projection {
        schedule::Projection {
            days: self
                .days
                .iter()
                .filter_map(|(d, v)| {
                    let s: Vec<Slot> = v
                        .iter()
                        .filter(|(l, _)| *l == id)
                        .map(|(_, s)| s.clone())
                        .collect();
                    (!s.is_empty()).then_some((*d, s))
                })
                .collect(),
            finish: self.finish.get(&id).copied(),
            short_minutes: self
                .short
                .iter()
                .filter(|s| s.lane == id)
                .map(|s| s.minutes)
                .sum(),
        }
    }
}

fn used_on(used: &Used, day: NaiveDate) -> u32 {
    used.get(&day).copied().unwrap_or(0)
}

fn round5(m: u32) -> u32 {
    m.div_ceil(5) * 5
}

/// 每周固定的事在推算里的状态
struct Routine {
    shape: Shape,
    title: String,
    items: Vec<(i64, String, u32)>,
    rot: usize,
}

struct St {
    id: i64,
    share: u32,
    items: Vec<LaneItem>,
    routine: Option<Routine>,
    alloc: u64,
    used: Used,
    had_work: bool,
    short: u32,
    short_by: Option<NaiveDate>,
    /// 今天最多还能做多少（挑块之前算好）
    cap_today: u32,
}

impl St {
    fn eligible(it: &LaneItem, day: NaiveDate, relax: bool) -> bool {
        it.left.minutes > 0 && it.pin.is_none() && (relax || it.after.is_none_or(|a| a <= day))
    }
    fn can_do(&self, day: NaiveDate, relax: bool) -> u32 {
        self.items
            .iter()
            .filter(|i| Self::eligible(i, day, relax))
            .map(|i| i.left.minutes)
            .sum()
    }
    /// 把 `budget` 分钟按顺序填进这天（只填能做的）
    fn fill(&mut self, budget: u32, day: NaiveDate, relax: bool) -> Vec<Slot> {
        let idx: Vec<usize> = (0..self.items.len())
            .filter(|&i| Self::eligible(&self.items[i], day, relax))
            .collect();
        let mut tmp: Vec<Left> = idx.iter().map(|&i| self.items[i].left.clone()).collect();
        let slots = schedule::fill(&mut tmp, budget);
        for (k, &i) in idx.iter().enumerate() {
            self.items[i].left = tmp[k].clone();
        }
        slots
    }
    fn content_left(&self) -> u32 {
        self.items.iter().map(|i| i.left.minutes).sum()
    }
}

/// 在能排的线里挑这一块给谁：欠得最多的（份额 × 总共分到的 − 自己分到的），一样多的取前面的
fn pick(st: &[St], cand: &[(usize, u32)], given: &[u32], block: u32) -> Option<usize> {
    let w_sum: i128 = cand.iter().map(|(_, w)| *w as i128).sum();
    if w_sum == 0 {
        return None;
    }
    let a_sum: i128 = cand
        .iter()
        .map(|(i, _)| st[*i].alloc as i128 + given[*i] as i128)
        .sum();
    cand.iter()
        .filter(|(i, w)| *w > 0 && given[*i] < st[*i].cap_today)
        .map(|(i, w)| {
            let a = st[*i].alloc as i128 + given[*i] as i128;
            (*i, *w as i128 * (a_sum + block as i128) - a * w_sum)
        })
        .max_by(|x, y| x.1.cmp(&y.1).then(y.0.cmp(&x.0)))
        .map(|(i, _)| i)
}

/// 从 `from` 开始往后推算 `horizon` 天
pub fn project(shape: &GoalShape, lanes: &[Lane], from: NaiveDate, horizon: u32) -> GoalProjection {
    let horizon = horizon.min(schedule::MAX_HORIZON);
    let mut p = GoalProjection::default();
    let fallback = shape.end;
    let mut st: Vec<St> = lanes
        .iter()
        .map(|l| {
            let (items, routine) = match &l.work {
                LaneWork::Content(v) => {
                    let mut v: Vec<LaneItem> =
                        v.iter().filter(|i| i.left.minutes > 0).cloned().collect();
                    // 截止早的先排（稳定排序：一样早的保持原来的顺序）
                    v.sort_by_key(|i| i.due.or(fallback).unwrap_or(NaiveDate::MAX));
                    (v, None)
                }
                LaneWork::Routine {
                    shape,
                    title,
                    items,
                    rot,
                } => (
                    Vec::new(),
                    Some(Routine {
                        shape: shape.clone(),
                        title: title.clone(),
                        items: items.clone(),
                        rot: *rot,
                    }),
                ),
            };
            St {
                id: l.id,
                share: l.share,
                had_work: !items.is_empty(),
                items,
                routine,
                alloc: l.alloc,
                used: l.used.clone(),
                short: 0,
                short_by: None,
                cap_today: 0,
            }
        })
        .collect();
    // 钉在已经过去的日子的：不排了（界面上显示「错过了」）；已经过了截止还没做完的：记来不及
    for x in st.iter_mut() {
        for it in x.items.iter_mut() {
            if it.pin.is_some_and(|d| d < from) {
                p.missed.push((x.id, it.left.item_id));
                it.left.minutes = 0;
                continue;
            }
            if let Some(due) = it.due.or(fallback).filter(|d| *d < from) {
                x.short += it.left.minutes;
                x.short_by = Some(x.short_by.map_or(due, |b| b.min(due)));
            }
        }
    }
    let mut seg = shape.segment_start(from);
    let mut day = from;
    for _ in 0..horizon {
        let s = shape.segment_start(day);
        if s != seg {
            // 换阶段了：份额重新记账
            seg = s;
            for x in st.iter_mut() {
                x.alloc = 0;
            }
        }
        let mut out: Vec<(i64, Slot)> = Vec::new();
        let used_total: u32 = st.iter().map(|x| used_on(&x.used, day)).sum();
        // 1. 每周固定的事先占时间（请假、整段停排的日子也不做）
        let mut routine_new = 0u32;
        let stopped =
            shape.off.contains(&day) || shape.buffer_percent(day) == 0 || day < shape.start;
        for x in st.iter_mut() {
            let id = x.id;
            let used = used_on(&x.used, day);
            if let Some(r) = x.routine.as_mut() {
                if stopped {
                    continue;
                }
                let cap = r.shape.capacity(day).saturating_sub(used);
                if cap >= MIN_CHUNK {
                    for s in schedule::habit_day(&r.title, &r.items, cap, &mut r.rot) {
                        routine_new += s.minutes;
                        out.push((id, s));
                    }
                }
            } else {
                x.alloc += used as u64;
            }
        }
        let mut free = shape.budget(day).saturating_sub(used_total + routine_new);
        // 2. 钉在这天的
        for x in st.iter_mut() {
            for it in x.items.iter_mut() {
                if it.pin == Some(day) && it.left.minutes > 0 {
                    let m = it.left.minutes;
                    out.push((
                        x.id,
                        Slot {
                            item_id: Some(it.left.item_id),
                            title: it.left.title.clone(),
                            minutes: m,
                            finishes: true,
                            continues: it.left.started,
                        },
                    ));
                    it.left.minutes = 0;
                    free = free.saturating_sub(m);
                    x.alloc += m as u64;
                }
            }
        }
        // 3. 剩下的按份额分
        if free >= MIN_CHUNK {
            let mut relax = false;
            let mut cand: Vec<(usize, u32)> = st
                .iter()
                .enumerate()
                .filter(|(_, x)| x.routine.is_none() && x.can_do(day, false) > 0)
                .map(|(i, x)| (i, shape.weight(day, x.id, x.share)))
                .filter(|(_, w)| *w > 0)
                .collect();
            if cand.is_empty() {
                // 这个阶段该做的都做完了：提前做后面的
                relax = true;
                cand = st
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| x.routine.is_none() && x.can_do(day, true) > 0)
                    .map(|(i, x)| (i, x.share.max(1)))
                    .collect();
            }
            for (i, _) in &cand {
                st[*i].cap_today = st[*i].can_do(day, relax);
            }
            if !cand.is_empty() {
                let n = ((free + BLOCK / 2) / BLOCK).clamp(1, MAX_BLOCKS);
                let block = round5(free.div_ceil(n));
                let mut given = vec![0u32; st.len()];
                let mut rest = free;
                while rest >= MIN_CHUNK {
                    let b = block.min(rest);
                    let Some(i) = pick(&st, &cand, &given, b) else {
                        break;
                    };
                    // 这条线今天没那么多能做的：只给它能做的，剩下的接着挑
                    let g = b.min(st[i].cap_today - given[i]);
                    given[i] += g;
                    rest -= g;
                }
                // 按分到的填；还剩的（太碎没排上的）给今天还没排的线
                let mut leftover = rest;
                for (i, _) in &cand {
                    let m = given[*i];
                    if m == 0 {
                        continue;
                    }
                    let slots = st[*i].fill(m, day, relax);
                    let got: u32 = slots.iter().map(|s| s.minutes).sum();
                    leftover += m.saturating_sub(got);
                    st[*i].alloc += got as u64;
                    let id = st[*i].id;
                    out.extend(slots.into_iter().map(|s| (id, s)));
                }
                if leftover >= MIN_CHUNK {
                    let mut order: Vec<usize> = cand
                        .iter()
                        .map(|(i, _)| *i)
                        .filter(|i| given[*i] == 0)
                        .collect();
                    order.sort_by_key(|i| std::cmp::Reverse(st[*i].can_do(day, relax)));
                    for i in order {
                        if leftover < MIN_CHUNK {
                            break;
                        }
                        let slots = st[i].fill(leftover, day, relax);
                        let got: u32 = slots.iter().map(|s| s.minutes).sum();
                        leftover = leftover.saturating_sub(got);
                        st[i].alloc += got as u64;
                        let id = st[i].id;
                        out.extend(slots.into_iter().map(|s| (id, s)));
                    }
                }
            }
        }
        for (_, s) in &out {
            if s.finishes
                && let Some(i) = s.item_id
            {
                p.item_finish.entry(i).or_insert(day);
            }
        }
        // 4. 今天是截止、还没排完的：来不及
        for x in st.iter_mut() {
            for it in &x.items {
                if it.left.minutes > 0 && it.due.or(fallback) == Some(day) {
                    x.short += it.left.minutes;
                    x.short_by = Some(x.short_by.map_or(day, |b| b.min(day)));
                }
            }
            if x.had_work && x.routine.is_none() && x.content_left() == 0 {
                p.finish.entry(x.id).or_insert(day);
            }
        }
        if !out.is_empty() {
            p.days.push((day, out));
        }
        let content_done = st
            .iter()
            .all(|x| x.routine.is_some() || x.content_left() == 0);
        let routines_done = st.iter().all(|x| match &x.routine {
            None => true,
            Some(r) => r.shape.end.is_some_and(|e| e < day),
        });
        if content_done && routines_done {
            break;
        }
        day += Duration::days(1);
    }
    p.short = st
        .iter()
        .filter(|x| x.short > 0)
        .map(|x| LaneShort {
            lane: x.id,
            minutes: x.short,
            by: x.short_by.unwrap_or(from),
        })
        .collect();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        let y = if m >= 9 { 2026 } else { 2027 };
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// 2026-11-09（星期一）开始
    fn shape(week: [u32; 7]) -> GoalShape {
        GoalShape {
            start: d(11, 9),
            end: Some(d(3, 31)),
            week,
            phases: Vec::new(),
            buffers: Vec::new(),
            off: Vec::new(),
        }
    }

    fn item(id: i64, minutes: u32) -> LaneItem {
        LaneItem {
            left: Left {
                item_id: id,
                title: format!("条目{id}"),
                minutes,
                started: false,
            },
            after: None,
            due: None,
            pin: None,
        }
    }

    fn content(id: i64, share: u32, items: Vec<LaneItem>) -> Lane {
        Lane {
            id,
            share,
            work: LaneWork::Content(items),
            alloc: 0,
            used: Used::new(),
        }
    }

    fn routine(id: i64, week: [u32; 7], end: Option<NaiveDate>) -> Lane {
        Lane {
            id,
            share: 0,
            work: LaneWork::Routine {
                shape: Shape {
                    mode: schedule::Mode::Habit,
                    start: d(11, 9),
                    end,
                    week,
                    off: Vec::new(),
                    pace: schedule::Pace::Asap,
                },
                title: "联系老师".into(),
                items: Vec::new(),
                rot: 0,
            },
            alloc: 0,
            used: Used::new(),
        }
    }

    fn on(p: &GoalProjection, day: NaiveDate) -> Vec<(i64, u32)> {
        p.days
            .iter()
            .find(|(x, _)| *x == day)
            .map(|(_, v)| v.iter().map(|(l, s)| (*l, s.minutes)).collect())
            .unwrap_or_default()
    }

    fn total(p: &GoalProjection, lane: i64, from: NaiveDate, to: NaiveDate) -> u32 {
        p.days
            .iter()
            .filter(|(x, _)| *x >= from && *x <= to)
            .flat_map(|(_, v)| v.iter())
            .filter(|(l, _)| *l == lane)
            .map(|(_, s)| s.minutes)
            .sum()
    }

    #[test]
    fn budget_follows_phase_buffer_and_off_days() {
        let mut s = shape([60, 60, 60, 60, 60, 120, 120]);
        s.phases.push(PhaseShape {
            id: 1,
            start: d(12, 1),
            end: d(12, 31),
            week: Some([90; 7]),
            shares: Vec::new(),
        });
        s.buffers.push(BufferShape {
            start: d(12, 14),
            end: d(12, 20),
            percent: 50,
        });
        s.off.push(d(11, 10));
        assert_eq!(s.budget(d(11, 8)), 0, "开始之前");
        assert_eq!(s.budget(d(11, 9)), 60);
        assert_eq!(s.budget(d(11, 10)), 0, "请假");
        assert_eq!(s.budget(d(11, 14)), 120, "周末");
        assert_eq!(s.budget(d(12, 2)), 90, "阶段自己的时间");
        assert_eq!(s.budget(d(12, 15)), 45, "缓冲期少排一半");
        assert_eq!(s.segment_start(d(12, 5)), d(12, 1));
        assert_eq!(s.segment_start(d(11, 20)), d(11, 9));
        assert_eq!(s.segment_start(d(1, 5)), d(1, 1), "上一个阶段结束的第二天");
    }

    #[test]
    fn routines_take_time_first() {
        let s = shape([60; 7]);
        let lanes = vec![
            content(1, 1, vec![item(10, 6000)]),
            routine(2, [0, 0, 30, 0, 0, 0, 0], Some(d(11, 30))),
        ];
        let p = project(&s, &lanes, d(11, 9), 30);
        assert_eq!(on(&p, d(11, 9)), [(1, 60)]);
        assert_eq!(on(&p, d(11, 11)), [(2, 30), (1, 30)], "星期三先排固定的事");
        assert_eq!(on(&p, d(12, 2)), [(1, 60)], "固定的事过了日期就不排了");
    }

    #[test]
    fn pinned_items_stay_on_their_day() {
        let s = shape([60; 7]);
        let mut pinned = item(11, 90);
        pinned.pin = Some(d(11, 12));
        let lanes = vec![content(1, 1, vec![item(10, 6000), pinned.clone()])];
        let p = project(&s, &lanes, d(11, 9), 10);
        assert_eq!(
            on(&p, d(11, 12)),
            [(1, 90)],
            "钉住的那天只做它（超过每天的时间也排）"
        );
        assert_eq!(p.item_finish.get(&11), Some(&d(11, 12)));
        // 那天过去了还没做：不顺延，记成错过
        let p = project(&s, &lanes, d(11, 13), 10);
        assert_eq!(p.missed, [(1, 11)]);
        assert!(
            p.days
                .iter()
                .all(|(_, v)| v.iter().all(|(_, s)| s.item_id != Some(11)))
        );
    }

    #[test]
    fn earlier_due_goes_first() {
        let s = shape([60; 7]);
        let mut a = item(10, 120);
        a.due = Some(d(12, 31));
        let mut b = item(11, 60);
        b.due = Some(d(11, 20));
        let p = project(&s, &[content(1, 1, vec![a, b])], d(11, 9), 10);
        assert_eq!(p.days[0].1[0].1.item_id, Some(11));
        assert_eq!(p.item_finish.get(&11), Some(&d(11, 9)));
        assert_eq!(p.finish.get(&1), Some(&d(11, 11)));
    }

    #[test]
    fn shares_alternate_across_days() {
        let s = shape([60; 7]);
        let lanes = vec![
            content(1, 1, vec![item(10, 6000)]),
            content(2, 1, vec![item(20, 6000)]),
        ];
        let p = project(&s, &lanes, d(11, 9), 6);
        let who: Vec<i64> = p.days.iter().map(|(_, v)| v[0].0).collect();
        assert_eq!(who, [1, 2, 1, 2, 1, 2], "一天一条线，轮流来");
        // 50 / 30 / 20：一个月下来大致按份额分
        let lanes = vec![
            content(1, 50, vec![item(10, 9000)]),
            content(2, 30, vec![item(20, 9000)]),
            content(3, 20, vec![item(30, 9000)]),
        ];
        let p = project(&s, &lanes, d(11, 9), 30);
        let (a, b, c) = (
            total(&p, 1, d(11, 9), d(12, 8)),
            total(&p, 2, d(11, 9), d(12, 8)),
            total(&p, 3, d(11, 9), d(12, 8)),
        );
        assert_eq!(a + b + c, 1800);
        assert!(
            (840..=960).contains(&a) && (480..=600).contains(&b) && (300..=420).contains(&c),
            "{a} {b} {c}"
        );
        // 一天 90 分钟：分成两块，两条线各做一会儿
        let p = project(&shape([90; 7]), &lanes, d(11, 9), 1);
        assert_eq!(on(&p, d(11, 9)).len(), 2);
    }

    #[test]
    fn history_counts_toward_shares() {
        let s = shape([60; 7]);
        let mut a = content(1, 1, vec![item(10, 6000)]);
        a.alloc = 120;
        let b = content(2, 1, vec![item(20, 6000)]);
        let p = project(&s, &[a, b], d(11, 11), 2);
        let who: Vec<i64> = p.days.iter().map(|(_, v)| v[0].0).collect();
        assert_eq!(who, [2, 2], "这个阶段 A 已经做过两天了，先补 B");
    }

    #[test]
    fn phases_switch_shares_and_hold_later_work() {
        let mut s = shape([60; 7]);
        s.phases = vec![
            PhaseShape {
                id: 1,
                start: d(11, 9),
                end: d(11, 15),
                week: None,
                shares: vec![(1, 1)],
            },
            PhaseShape {
                id: 2,
                start: d(11, 16),
                end: d(11, 30),
                week: Some([30; 7]),
                shares: vec![(2, 1)],
            },
        ];
        let mut later = item(20, 600);
        later.after = Some(d(11, 16));
        let lanes = vec![
            content(1, 1, vec![item(10, 180)]),
            content(2, 1, vec![later]),
        ];
        let p = project(&s, &lanes, d(11, 9), 20);
        assert_eq!(
            total(&p, 2, d(11, 9), d(11, 11)),
            0,
            "第二阶段的内容不提前（A 还有得做）"
        );
        assert_eq!(total(&p, 1, d(11, 9), d(11, 11)), 180);
        assert_eq!(
            on(&p, d(11, 12)),
            [(2, 60)],
            "这个阶段的都做完了：提前做后面的"
        );
        assert_eq!(on(&p, d(11, 17)), [(2, 30)], "第二阶段每天 30 分钟");
    }

    #[test]
    fn leftover_goes_to_the_next_lane() {
        let s = shape([120; 7]);
        let lanes = vec![
            content(1, 1, vec![item(10, 20)]),
            content(2, 1, vec![item(20, 6000)]),
        ];
        let p = project(&s, &lanes, d(11, 9), 1);
        assert_eq!(on(&p, d(11, 9)), [(1, 20), (2, 100)]);
    }

    #[test]
    fn shortage_is_reported_per_lane() {
        let s = shape([60; 7]);
        let mut a = item(10, 300);
        a.due = Some(d(11, 11));
        let lanes = vec![content(1, 1, vec![a]), content(2, 1, vec![item(20, 60)])];
        let p = project(&s, &lanes, d(11, 9), 30);
        assert_eq!(
            p.short,
            [LaneShort {
                lane: 1,
                minutes: 180,
                by: d(11, 11)
            }],
            "A 截止前三天只排到两天（有一天给了 B）"
        );
        assert_eq!(p.lane(1).short_minutes, 180);
        assert_eq!(p.lane(2).days.len(), 1);
    }

    /// 和本机样例同结构的虚构例子：几条线、按阶段变强度、缓冲周、每周固定的事、钉住的日子、阶段截止
    #[test]
    fn long_multi_track_goal() {
        let mut s = shape([60, 60, 60, 60, 60, 120, 120]);
        s.phases = vec![
            PhaseShape {
                id: 1,
                start: d(11, 9),
                end: d(12, 31),
                week: None,
                shares: vec![(1, 40), (2, 30), (3, 30)],
            },
            PhaseShape {
                id: 2,
                start: d(1, 1),
                end: d(2, 28),
                week: Some([90, 90, 90, 90, 90, 150, 150]),
                shares: vec![(1, 20), (2, 40), (3, 20), (4, 20)],
            },
            PhaseShape {
                id: 3,
                start: d(3, 1),
                end: d(3, 31),
                week: None,
                shares: vec![(4, 100)],
            },
        ];
        s.buffers.push(BufferShape {
            start: d(1, 11),
            end: d(1, 17),
            percent: 0,
        });
        let phased = |base: i64, per: &[(u32, NaiveDate, NaiveDate)]| -> Vec<LaneItem> {
            per.iter()
                .enumerate()
                .map(|(k, (m, a, due))| {
                    let mut it = item(base + k as i64, *m);
                    it.after = Some(*a);
                    it.due = Some(*due);
                    it
                })
                .collect()
        };
        let p1 = (d(11, 9), d(12, 31));
        let p2 = (d(1, 1), d(2, 28));
        let p3 = (d(3, 1), d(3, 31));
        let mut apply = item(40, 120);
        apply.pin = Some(d(3, 2));
        let mut l4 = phased(41, &[(600, p2.0, p2.1), (900, p3.0, p3.1)]);
        l4.push(apply);
        let lanes = vec![
            content(1, 1, phased(10, &[(1200, p1.0, p1.1), (600, p2.0, p2.1)])),
            content(2, 1, phased(20, &[(900, p1.0, p1.1), (1200, p2.0, p2.1)])),
            content(3, 1, phased(30, &[(900, p1.0, p1.1), (600, p2.0, p2.1)])),
            content(4, 1, l4),
            routine(5, [0, 0, 0, 0, 0, 0, 30], Some(d(2, 28))),
        ];
        let p = project(&s, &lanes, d(11, 9), 200);
        assert!(
            p.days.iter().all(|(x, _)| *x < d(1, 11) || *x > d(1, 17)),
            "缓冲周整周不排（固定的事也停）"
        );
        // 第四条线的内容第二阶段才开始：前三条线第一阶段的都做完了，才提前做
        let last = [10, 20, 30]
            .iter()
            .filter_map(|i| p.item_finish.get(i))
            .max()
            .copied()
            .unwrap();
        assert!(last < p1.1, "第一阶段提前做完：{last}");
        assert_eq!(total(&p, 4, p1.0, last - Duration::days(1)), 0);
        assert!(total(&p, 4, last, p1.1) > 0, "做完了就提前做后面的");
        assert_eq!(on(&p, d(3, 2)).first(), Some(&(4, 120)), "钉住的投递日");
        assert!(p.days.iter().any(|(x, v)| x.weekday() == chrono::Weekday::Sun
            && v.iter().any(|(l, _)| *l == 5)));
        // 第一阶段一共 3000 分钟，能用的约 3240 分钟：不缺时间，三条线都在阶段内做完
        assert!(
            p.short.iter().all(|x| x.by > p1.1),
            "第一阶段不缺时间：{:?}",
            p.short
        );
        for lane in [1, 2, 3] {
            assert!(
                p.item_finish.get(&(lane * 10)).is_some_and(|f| *f <= p1.1),
                "第 {lane} 条线第一阶段做完：{:?}",
                p.item_finish.get(&(lane * 10))
            );
        }
        assert!(p.finish.contains_key(&4));
    }
}
