//! 节假日：本地小数据（`data/holidays.tsv`：中国官方放假安排、日本内阁府的祝日）+ 美国联邦假日的规则 + 常见节日。
//! 不联网。数据每年要更新一次（HOW_TO_ADD 5.1）。

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

const DATA: &str = include_str!("../../data/holidays.tsv");

/// 用哪个国家 / 地区的节假日
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Region {
    /// 跟着城市所在的国家走
    #[default]
    Auto,
    CN,
    JP,
    US,
    /// 只认周末
    Other,
}

impl Region {
    pub fn resolve(self, country: &str) -> Region {
        match self {
            Region::Auto => match country {
                "CN" | "HK" | "MO" | "TW" | "" => Region::CN,
                "JP" => Region::JP,
                "US" => Region::US,
                _ => Region::Other,
            },
            r => r,
        }
    }
    fn code(self) -> &'static str {
        match self {
            Region::CN => "CN",
            Region::JP => "JP",
            Region::US => "US",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayKind {
    Workday,
    Weekend,
    Holiday,
    /// 调休上班（周末但要上班）
    MakeupWorkday,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayInfo {
    pub kind: DayKind,
    /// 节假日 / 节日名（放假的节日、或者不放假的节日）
    pub name: Option<String>,
}

impl DayInfo {
    pub fn is_rest(&self) -> bool {
        matches!(self.kind, DayKind::Weekend | DayKind::Holiday)
    }
}

struct Entry {
    date: NaiveDate,
    region: &'static str,
    kind: &'static str,
    name: &'static str,
}

fn entries() -> &'static [Entry] {
    static E: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    E.get_or_init(|| {
        DATA.lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let f: Vec<&'static str> = l.split('\t').collect();
                Some(Entry {
                    date: NaiveDate::parse_from_str(f.first()?, "%Y-%m-%d").ok()?,
                    region: f.get(1)?,
                    kind: f.get(2)?,
                    name: f.get(3)?,
                })
            })
            .collect()
    })
}

fn nth_weekday(y: i32, m: u32, wd: Weekday, n: u8) -> Option<NaiveDate> {
    NaiveDate::from_weekday_of_month_opt(y, m, wd, n)
}

fn last_weekday(y: i32, m: u32, wd: Weekday) -> Option<NaiveDate> {
    (1..=5).rev().find_map(|n| nth_weekday(y, m, wd, n))
}

/// 美国联邦假日（规则计算；遇周末的「补假」不算）
fn us_holiday(d: NaiveDate) -> Option<&'static str> {
    let (y, m, day) = (d.year(), d.month(), d.day());
    let is = |x: Option<NaiveDate>| x == Some(d);
    Some(match (m, day) {
        (1, 1) => "元旦",
        (6, 19) => "六月节",
        (7, 4) => "独立日",
        (11, 11) => "退伍军人节",
        (12, 25) => "圣诞节",
        _ if is(nth_weekday(y, 1, Weekday::Mon, 3)) => "马丁·路德·金纪念日",
        _ if is(nth_weekday(y, 2, Weekday::Mon, 3)) => "总统日",
        _ if is(last_weekday(y, 5, Weekday::Mon)) => "阵亡将士纪念日",
        _ if is(nth_weekday(y, 9, Weekday::Mon, 1)) => "劳动节",
        _ if is(nth_weekday(y, 10, Weekday::Mon, 2)) => "哥伦布日",
        _ if is(nth_weekday(y, 11, Weekday::Thu, 4)) => "感恩节",
        _ => return None,
    })
}

/// 不放假的常见节日（各地都提一句）
fn festival(d: NaiveDate, region: Region) -> Option<&'static str> {
    Some(match (d.month(), d.day()) {
        (2, 14) => "情人节",
        (3, 14) if region == Region::JP => "白色情人节",
        (4, 1) => "愚人节",
        (5, 4) if region == Region::CN => "青年节",
        (6, 1) if region == Region::CN => "儿童节",
        (9, 10) if region == Region::CN => "教师节",
        (10, 31) => "万圣节",
        (12, 24) => "平安夜",
        (12, 25) if region != Region::US => "圣诞节",
        _ => return None,
    })
}

pub fn info(d: NaiveDate, region: Region) -> DayInfo {
    let weekend = matches!(d.weekday(), Weekday::Sat | Weekday::Sun);
    let code = region.code();
    let mut name: Option<String> = None;
    let mut kind = if weekend {
        DayKind::Weekend
    } else {
        DayKind::Workday
    };
    for e in entries().iter().filter(|e| e.date == d && e.region == code) {
        match e.kind {
            "off" => {
                kind = DayKind::Holiday;
                name = Some(e.name.into());
            }
            "work" => {
                kind = DayKind::MakeupWorkday;
                name = Some(e.name.into());
            }
            _ => {
                // 节日当天（放假安排未公布的年份）
                if name.is_none() {
                    name = Some(e.name.into());
                }
            }
        }
    }
    if region == Region::US
        && let Some(n) = us_holiday(d)
    {
        kind = DayKind::Holiday;
        name = Some(n.into());
    }
    if name.is_none() {
        name = festival(d, region).map(String::from);
    }
    DayInfo { kind, name }
}

/// 接下来 `days` 天内最近的一个放假的节日：（几天后，名字）
pub fn upcoming(today: NaiveDate, region: Region, days: i64) -> Option<(i64, String)> {
    (1..=days).find_map(|n| {
        let d = today + Duration::days(n);
        let i = info(d, region);
        // 放假的节日（不只是周末）；连着放的假只报第一天
        let prev = info(d - Duration::days(1), region);
        (i.kind == DayKind::Holiday && !(prev.kind == DayKind::Holiday && prev.name == i.name))
            .then(|| (n, i.name.unwrap_or_default()))
    })
}

/// 有没有这一年的官方数据（没有的话放假安排可能不准，提示里说一声）
pub fn has_data(year: i32, region: Region) -> bool {
    match region {
        Region::CN | Region::JP => entries()
            .iter()
            .any(|e| e.region == region.code() && e.date.year() == year && e.kind != "fest"),
        _ => true,
    }
}

/// 放假时推荐的活动类型（只说类型，不推荐具体的店）
pub fn leisure_ideas(rainy: bool, cold: bool, hot: bool) -> &'static [&'static str] {
    if rainy {
        &[
            "在家打游戏",
            "看一部电影",
            "去商场或博物馆逛逛",
            "泡杯热饮看书",
        ]
    } else if cold {
        &[
            "吃顿热乎的火锅",
            "去室内展览看看",
            "在家窝着打游戏",
            "泡温泉",
        ]
    } else if hot {
        &[
            "去有空调的地方看展",
            "傍晚出门散步",
            "吃点冰的",
            "在家看电影",
        ]
    } else {
        &[
            "去公园走走",
            "出门拍照",
            "和朋友约饭",
            "骑车兜风",
            "逛逛书店",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn china_2026_official_schedule() {
        let i = info(d(2026, 10, 1), Region::CN);
        assert_eq!(
            (i.kind, i.name.as_deref()),
            (DayKind::Holiday, Some("国庆节"))
        );
        // 9 月 20 日是周日，但要调休上班
        let i = info(d(2026, 9, 20), Region::CN);
        assert_eq!(i.kind, DayKind::MakeupWorkday);
        assert!(!i.is_rest());
        // 普通周三
        assert_eq!(info(d(2026, 10, 28), Region::CN).kind, DayKind::Workday);
        assert_eq!(info(d(2026, 10, 31), Region::CN).kind, DayKind::Weekend);
        assert_eq!(
            info(d(2026, 10, 31), Region::CN).name.as_deref(),
            Some("万圣节")
        );
        assert!(has_data(2026, Region::CN));
        assert!(!has_data(2027, Region::CN), "2027 年的放假安排还没公布");
        // 2027 年只有节日当天
        assert_eq!(
            info(d(2027, 2, 6), Region::CN).name.as_deref(),
            Some("春节")
        );
    }

    #[test]
    fn japan_and_us() {
        let i = info(d(2026, 11, 3), Region::JP);
        assert_eq!(
            (i.kind, i.name.as_deref()),
            (DayKind::Holiday, Some("文化日"))
        );
        assert_eq!(
            info(d(2026, 11, 26), Region::US).name.as_deref(),
            Some("感恩节")
        );
        assert_eq!(
            info(d(2026, 5, 25), Region::US).name.as_deref(),
            Some("阵亡将士纪念日")
        );
        assert_eq!(info(d(2026, 10, 28), Region::US).kind, DayKind::Workday);
        assert_eq!(
            info(d(2026, 10, 1), Region::Other).kind,
            DayKind::Workday,
            "只认周末"
        );
    }

    #[test]
    fn upcoming_holiday() {
        // 9 月 22 日（周二）：三天后是中秋（25 日）
        assert_eq!(
            upcoming(d(2026, 9, 22), Region::CN, 7),
            Some((3, "中秋节".into()))
        );
        // 连休中间不再报同一个节日
        let next = upcoming(d(2026, 10, 1), Region::CN, 5);
        assert_eq!(next, None);
        assert_eq!(
            upcoming(d(2026, 10, 28), Region::JP, 7),
            Some((6, "文化日".into()))
        );
    }

    #[test]
    fn region_auto() {
        assert_eq!(Region::Auto.resolve("JP"), Region::JP);
        assert_eq!(Region::Auto.resolve("HK"), Region::CN);
        assert_eq!(Region::Auto.resolve("FR"), Region::Other);
        assert_eq!(Region::US.resolve("CN"), Region::US);
    }
}
