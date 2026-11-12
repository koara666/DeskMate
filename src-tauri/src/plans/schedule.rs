//! 排程器（纯函数，不碰数据库和时钟）：给定计划的「形状」（每周每天能用多少分钟、截止、请假、节奏）和还剩的内容，
//! 算出某一天该做什么，以及往后每天大概排什么、预计哪天做完、是不是来不及。
//!
//! - 内容类（content）：条目按顺序往每天里填，一条做不完就拆到第二天接着做；快做完的尾巴（不到 10 分钟）允许当天稍微多排一点收掉。
//! - 节奏 even + 有截止：每天的量 = 那天能用的分钟 × 剩余 ÷ 到截止为止能用的分钟总和（向上取到 5 分钟），也就是均匀排到截止；
//!   来不及时每天按上限排，并报告差多少。没有截止或 asap：每天按上限排。
//! - 习惯类（habit）：每个能排的日子都做；有条目时，放得下就全做，放不下就按顺序轮流。
//! - 每天的量每天重新算，所以哪天没做完，剩下的自动回到池子里，后面的日子会顺延或加量（不超过上限）。

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use std::collections::BTreeMap;

/// 拆出来的一段至少多少分钟（更短的不单独排）
pub const MIN_CHUNK: u32 = 10;
/// 为了把一条收尾，当天最多多排几分钟
pub const TAIL_SLACK: u32 = 10;
/// 一个计划一天最多排几项
pub const MAX_PER_DAY: usize = 4;
/// 往后推算最多看多少天
pub const MAX_HORIZON: u32 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Content,
    Habit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Even,
    Asap,
}

/// 计划的形状
#[derive(Debug, Clone)]
pub struct Shape {
    pub mode: Mode,
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    /// 周一到周日每天能用的分钟
    pub week: [u32; 7],
    pub off: Vec<NaiveDate>,
    pub pace: Pace,
}

impl Shape {
    /// 这一天能用多少分钟（不在计划范围、请假、那天不排 → 0）。内容类过了截止照样排（还没做完，就接着做）。
    pub fn capacity(&self, day: NaiveDate) -> u32 {
        if day < self.start || self.off.contains(&day) {
            return 0;
        }
        if self.mode == Mode::Habit && self.end.is_some_and(|e| day > e) {
            return 0;
        }
        self.week[day.weekday().num_days_from_monday() as usize]
    }

    /// [from, end] 里能用的分钟总和
    fn capacity_until_end(&self, from: NaiveDate) -> Option<u64> {
        let end = self.end?;
        let mut d = from;
        let mut sum = 0u64;
        while d <= end {
            sum += self.capacity(d) as u64;
            d += Duration::days(1);
        }
        Some(sum)
    }
}

/// 一条还没做完的内容
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Left {
    pub item_id: i64,
    pub title: String,
    /// 还剩多少分钟
    pub minutes: u32,
    /// 已经做过一部分
    pub started: bool,
}

/// 排进某一天的一项
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Slot {
    pub item_id: Option<i64>,
    pub title: String,
    pub minutes: u32,
    /// 做完这项，这条内容就完成了
    pub finishes: bool,
    /// 接着之前做的
    pub continues: bool,
}

fn round5(m: u32) -> u32 {
    m.div_ceil(5) * 5
}

/// 内容类：今天（`day`）该排多少分钟。`left_total` 是还剩的总分钟。
pub fn content_budget(shape: &Shape, day: NaiveDate, left_total: u32) -> u32 {
    let cap = shape.capacity(day);
    if cap == 0 || left_total == 0 {
        return 0;
    }
    if shape.pace == Pace::Even
        && let Some(total) = shape.capacity_until_end(day)
        && total > left_total as u64
    {
        // 均匀排到截止
        let share = (cap as u64 * left_total as u64).div_ceil(total) as u32;
        return round5(share.max(MIN_CHUNK)).min(cap);
    }
    cap
}

/// 把 `budget` 分钟按顺序填进今天（会改动 `left`）
pub fn fill(left: &mut [Left], budget: u32) -> Vec<Slot> {
    let mut out = Vec::new();
    let mut b = budget;
    for l in left.iter_mut() {
        if out.len() >= MAX_PER_DAY || b == 0 {
            break;
        }
        if l.minutes == 0 {
            continue;
        }
        let mut chunk = l.minutes.min(b);
        let rest = l.minutes - chunk;
        // 剩下一点点尾巴：今天顺手收掉
        if rest > 0 && rest < MIN_CHUNK && chunk + rest <= b + TAIL_SLACK {
            chunk = l.minutes;
        }
        // 太碎的一段不单独排（除非这条本来就很短）
        if chunk < MIN_CHUNK && chunk < l.minutes {
            break;
        }
        out.push(Slot {
            item_id: Some(l.item_id),
            title: l.title.clone(),
            minutes: chunk,
            finishes: chunk >= l.minutes,
            continues: l.started,
        });
        l.minutes -= chunk;
        l.started = true;
        b = b.saturating_sub(chunk);
    }
    out
}

/// 习惯类：这一天做什么。`items`：（条目 id、标题、每次分钟，0 = 用当天的量），`rot`：轮到第几条
pub fn habit_day(
    title: &str,
    items: &[(i64, String, u32)],
    cap: u32,
    rot: &mut usize,
) -> Vec<Slot> {
    if cap == 0 {
        return Vec::new();
    }
    if items.is_empty() {
        return vec![Slot {
            item_id: None,
            title: title.to_string(),
            minutes: cap,
            finishes: false,
            continues: false,
        }];
    }
    let n = items.len();
    let need: u32 = items.iter().map(|i| if i.2 == 0 { cap } else { i.2 }).sum();
    // 放得下：每天全做
    if need <= cap && n <= MAX_PER_DAY {
        return items
            .iter()
            .map(|(id, t, m)| Slot {
                item_id: Some(*id),
                title: t.clone(),
                minutes: if *m == 0 { cap } else { *m },
                finishes: false,
                continues: false,
            })
            .collect();
    }
    // 放不下：按顺序轮流，填满当天
    let mut out = Vec::new();
    let mut b = cap;
    for _ in 0..n {
        let (id, t, m) = &items[*rot % n];
        let m = if *m == 0 { cap } else { *m };
        if !out.is_empty() && (m > b || out.len() >= MAX_PER_DAY) {
            break;
        }
        out.push(Slot {
            item_id: Some(*id),
            title: t.clone(),
            minutes: m.min(cap),
            finishes: false,
            continues: false,
        });
        *rot += 1;
        b = b.saturating_sub(m);
        if b < MIN_CHUNK {
            break;
        }
    }
    out
}

/// 往后推算的结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct Projection {
    /// 每天（有安排的日子）排了什么
    pub days: Vec<(NaiveDate, Vec<Slot>)>,
    /// 预计哪天做完（内容类；推算范围内做不完 = None）
    pub finish: Option<NaiveDate>,
    /// 按每天的上限排也来不及：到截止还差多少分钟
    pub short_minutes: u32,
}

/// 要排的东西
#[derive(Debug, Clone)]
pub enum Work {
    /// 内容类：`from` 这天开始前还剩的内容
    Content(Vec<Left>),
    /// 习惯类：（条目 id、标题、每次分钟），`rot` 轮到第几条
    Habit {
        title: String,
        items: Vec<(i64, String, u32)>,
        rot: usize,
    },
}

/// 已经落库的安排在某几天占掉的分钟（今天已经排好 / 做完的、挪到以后某天的）
pub type Used = BTreeMap<NaiveDate, u32>;

fn used_on(used: &Used, day: NaiveDate) -> u32 {
    used.get(&day).copied().unwrap_or(0)
}

/// 从 `from` 开始往后推算 `horizon` 天。内容类的 `left` 里已经扣掉了落库的安排占着的那部分，
/// 所以每天先把 `used` 加回去算这天该排多少，再减掉已经占的。
pub fn project(
    shape: &Shape,
    work: &Work,
    from: NaiveDate,
    used: &Used,
    horizon: u32,
) -> Projection {
    let mut p = Projection::default();
    let horizon = horizon.min(MAX_HORIZON);
    match work {
        Work::Content(left) => {
            let mut left: Vec<Left> = left.iter().filter(|l| l.minutes > 0).cloned().collect();
            let total: u32 = left.iter().map(|l| l.minutes).sum();
            if total == 0 {
                return p;
            }
            if let Some(cap) = shape.capacity_until_end(from) {
                let taken: u64 = used.range(from..).map(|(_, m)| *m as u64).sum();
                let cap = cap.saturating_sub(taken);
                if cap < total as u64 {
                    p.short_minutes = (total as u64 - cap) as u32;
                }
            }
            let mut day = from;
            for _ in 0..horizon {
                let remaining: u32 = left.iter().map(|l| l.minutes).sum();
                if remaining == 0 {
                    break;
                }
                let u = used_on(used, day);
                let budget = content_budget(shape, day, remaining + u).saturating_sub(u);
                let slots = fill(&mut left, budget);
                if !slots.is_empty() {
                    p.days.push((day, slots));
                }
                if left.iter().all(|l| l.minutes == 0) {
                    p.finish = Some(day);
                    break;
                }
                day += Duration::days(1);
            }
        }
        Work::Habit { title, items, rot } => {
            let mut rot = *rot;
            let mut day = from;
            for _ in 0..horizon {
                let cap = shape.capacity(day).saturating_sub(used_on(used, day));
                if cap >= MIN_CHUNK {
                    let slots = habit_day(title, items, cap, &mut rot);
                    if !slots.is_empty() {
                        p.days.push((day, slots));
                    }
                }
                day += Duration::days(1);
            }
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn shape(week: [u32; 7], end: Option<NaiveDate>, pace: Pace) -> Shape {
        Shape {
            mode: Mode::Content,
            // 2026-11-09 是星期一
            start: d(11, 9),
            end,
            week,
            off: Vec::new(),
            pace,
        }
    }

    fn items(v: &[(i64, &str, u32)]) -> Vec<Left> {
        v.iter()
            .map(|(id, t, m)| Left {
                item_id: *id,
                title: t.to_string(),
                minutes: *m,
                started: false,
            })
            .collect()
    }

    fn used(v: &[(NaiveDate, u32)]) -> Used {
        v.iter().copied().collect()
    }

    fn content(v: &[(i64, &str, u32)]) -> Work {
        Work::Content(items(v))
    }

    #[test]
    fn capacity_respects_week_off_days_and_range() {
        let mut s = shape([60, 60, 60, 60, 60, 120, 0], None, Pace::Asap);
        s.off.push(d(11, 11));
        assert_eq!(s.capacity(d(11, 8)), 0, "开始之前");
        assert_eq!(s.capacity(d(11, 9)), 60);
        assert_eq!(s.capacity(d(11, 11)), 0, "请假");
        assert_eq!(s.capacity(d(11, 14)), 120, "星期六");
        assert_eq!(s.capacity(d(11, 15)), 0, "星期日不排");
        s.mode = Mode::Habit;
        s.end = Some(d(11, 12));
        assert_eq!(s.capacity(d(11, 13)), 0, "习惯过了截止就不排");
    }

    #[test]
    fn fills_in_order_and_splits_long_items() {
        let mut left = items(&[(1, "第1章", 90), (2, "第2章", 40)]);
        let a = fill(&mut left, 60);
        assert_eq!(a.len(), 1);
        assert_eq!(
            (a[0].minutes, a[0].finishes, a[0].continues),
            (60, false, false)
        );
        let b = fill(&mut left, 60);
        assert_eq!(b.len(), 2);
        assert_eq!(
            (b[0].minutes, b[0].finishes, b[0].continues),
            (30, true, true)
        );
        assert_eq!(
            (b[1].title.as_str(), b[1].minutes, b[1].finishes),
            ("第2章", 30, false)
        );
        // 剩 10 分钟的尾巴：正常排
        let c = fill(&mut left, 60);
        assert_eq!((c[0].minutes, c[0].finishes), (10, true));
    }

    #[test]
    fn small_tail_is_finished_the_same_day() {
        let mut left = items(&[(1, "a", 65)]);
        let a = fill(&mut left, 60);
        assert_eq!(
            (a[0].minutes, a[0].finishes),
            (65, true),
            "差 5 分钟就顺手做完"
        );
        // 太碎的一段不单独排
        let mut left = items(&[(1, "a", 30), (2, "b", 100)]);
        let a = fill(&mut left, 35);
        assert_eq!(a.len(), 1, "剩 5 分钟不开始下一条");
    }

    #[test]
    fn asap_uses_full_capacity_and_estimates_finish() {
        let s = shape([60; 7], None, Pace::Asap);
        let p = project(
            &s,
            &content(&[(1, "a", 100), (2, "b", 100)]),
            d(11, 9),
            &Used::new(),
            30,
        );
        assert_eq!(p.days.len(), 4);
        assert_eq!(p.finish, Some(d(11, 12)));
        assert_eq!(p.short_minutes, 0);
    }

    #[test]
    fn even_pace_spreads_to_deadline() {
        // 10 天、每天最多 60 分钟，只有 200 分钟内容：每天 20 分钟，正好排到截止
        let s = shape([60; 7], Some(d(11, 18)), Pace::Even);
        let p = project(&s, &content(&[(1, "a", 200)]), d(11, 9), &Used::new(), 30);
        assert!(p.days.iter().all(|(_, v)| v[0].minutes == 20), "{p:?}");
        assert_eq!(p.finish, Some(d(11, 18)));
        // asap 同样的内容：4 天做完
        let s2 = shape([60; 7], Some(d(11, 18)), Pace::Asap);
        let p2 = project(&s2, &content(&[(1, "a", 200)]), d(11, 9), &Used::new(), 30);
        assert_eq!(p2.finish, Some(d(11, 12)));
    }

    #[test]
    fn behind_schedule_reports_shortage_and_keeps_going() {
        // 3 天 × 60 分钟，内容 300 分钟：差 120 分钟，过了截止接着排
        let s = shape([60; 7], Some(d(11, 11)), Pace::Even);
        let p = project(&s, &content(&[(1, "a", 300)]), d(11, 9), &Used::new(), 30);
        assert_eq!(p.short_minutes, 120);
        assert!(p.days.iter().all(|(_, v)| v[0].minutes == 60));
        assert_eq!(p.finish, Some(d(11, 13)));
    }

    #[test]
    fn today_already_used_is_subtracted() {
        let s = shape([60; 7], None, Pace::Asap);
        let p = project(
            &s,
            &content(&[(1, "a", 100)]),
            d(11, 9),
            &used(&[(d(11, 9), 45)]),
            30,
        );
        assert_eq!(p.days[0].1[0].minutes, 15);
        assert_eq!(p.days[1].1[0].minutes, 60);
        // 今天已经用满：从明天排起
        let p = project(
            &s,
            &content(&[(1, "a", 100)]),
            d(11, 9),
            &used(&[(d(11, 9), 60)]),
            30,
        );
        assert_eq!(p.days[0].0, d(11, 10));
    }

    #[test]
    fn weekends_and_off_days_are_skipped() {
        let mut s = shape([60, 60, 60, 60, 60, 0, 0], None, Pace::Asap);
        s.off.push(d(11, 10));
        let p = project(&s, &content(&[(1, "a", 300)]), d(11, 9), &Used::new(), 30);
        let days: Vec<_> = p.days.iter().map(|(x, _)| *x).collect();
        assert_eq!(days, [d(11, 9), d(11, 11), d(11, 12), d(11, 13), d(11, 16)]);
    }

    #[test]
    fn habit_all_or_rotate() {
        let mut rot = 0;
        // 放得下：每天都做两项
        let v = habit_day(
            "英语",
            &[(1, "单词".into(), 20), (2, "听力".into(), 30)],
            60,
            &mut rot,
        );
        assert_eq!(v.len(), 2);
        // 放不下：轮流
        let fit = [
            (1, "胸".into(), 60),
            (2, "腿".into(), 60),
            (3, "背".into(), 60),
        ];
        let a = habit_day("健身", &fit, 60, &mut rot);
        let b = habit_day("健身", &fit, 60, &mut rot);
        let c = habit_day("健身", &fit, 60, &mut rot);
        let e = habit_day("健身", &fit, 60, &mut rot);
        assert_eq!(
            [&a[0].title, &b[0].title, &c[0].title, &e[0].title],
            ["胸", "腿", "背", "胸"]
        );
        // 没有条目：每天做计划本身
        let v = habit_day("跑步", &[], 30, &mut rot);
        assert_eq!(
            (v[0].title.as_str(), v[0].minutes, v[0].item_id),
            ("跑步", 30, None)
        );
        let mut s = shape([30; 7], Some(d(11, 12)), Pace::Asap);
        s.mode = Mode::Habit;
        let w = Work::Habit {
            title: "跑步".into(),
            items: Vec::new(),
            rot: 0,
        };
        let p = project(&s, &w, d(11, 9), &Used::new(), 30);
        assert_eq!(p.days.len(), 4, "习惯到截止为止");
        assert_eq!(p.finish, None);
    }
}
