//! 本地解析「半小时后提醒我收衣服」「每周五晚上提醒我倒垃圾」这类说法（不调用模型）。
//! 解析不了的交给模型（见 `reminders::model`），模型也不可用时让用户换个说法或手动填。
//!
//! 只认常见说法，宁可解析不了也不要猜错：解析结果会先复述给用户确认。

use super::{Repeat, Trigger, activity_name};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};

/// 解析结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub content: String,
    pub trigger: Trigger,
    /// At：下次提醒的本地时间；Startup：从哪天起（None = 下次开机）
    pub at: Option<NaiveDateTime>,
    pub repeat: Repeat,
    pub activity: String,
    /// 是不是「X 分钟后」这种相对时间（复述时说「30 分钟后」）
    pub relative: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 不是在请求提醒
    NotReminder,
    /// 是在请求提醒，但没听懂时间或内容
    Unclear,
    Ok(Parsed),
}

const INTENT: [&str; 6] = [
    "提醒我",
    "提醒一下我",
    "提醒下我",
    "记得提醒",
    "到时候提醒",
    "帮我设个提醒",
];

/// 这句话像不像在请求设提醒（「你提醒我了」「上次提醒我的事」这类回忆不算）
pub fn looks_like_request(text: &str) -> bool {
    let t = normalize(text);
    INTENT.iter().any(|k| t.contains(k))
        && !["提醒我了", "提醒我的", "提醒过我", "你提醒我"]
            .iter()
            .any(|x| t.contains(x))
}

/// 全角数字和冒号换成半角，去掉空白
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
            '：' => ':',
            _ => c,
        })
        .collect()
}

/// 中文 / 阿拉伯数字（0~99）
fn cn_digit(c: char) -> Option<u32> {
    match c {
        '零' | '〇' => Some(0),
        '一' => Some(1),
        '二' | '两' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    }
}

fn is_num_char(c: char) -> bool {
    c.is_ascii_digit() || cn_digit(c).is_some() || c == '十'
}

/// 解析一个数字串（「15」「十五」「二十」「两」「十」），只认 0~99
fn parse_num(s: &[char]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    if s.iter().all(|c| c.is_ascii_digit()) {
        let n: u32 = s.iter().collect::<String>().parse().ok()?;
        return (n < 1000).then_some(n);
    }
    match s.iter().position(|&c| c == '十') {
        None if s.len() == 1 => cn_digit(s[0]),
        None => None,
        Some(i) => {
            let tens = if i == 0 {
                1
            } else if i == 1 {
                cn_digit(s[0])?
            } else {
                return None;
            };
            let ones = match &s[i + 1..] {
                [] => 0,
                [c] => cn_digit(*c)?,
                _ => return None,
            };
            Some(tens * 10 + ones)
        }
    }
}

/// 从 `end` 往前取连续的数字字符，返回（起点，数值）
fn num_before(c: &[char], end: usize) -> Option<(usize, u32)> {
    let mut i = end;
    while i > 0 && is_num_char(c[i - 1]) {
        i -= 1;
    }
    if i == end {
        return None;
    }
    Some((i, parse_num(&c[i..end])?))
}

/// 从 `start` 往后取连续的数字字符，返回（终点，数值）
fn num_after(c: &[char], start: usize) -> Option<(usize, u32)> {
    let mut i = start;
    while i < c.len() && is_num_char(c[i]) {
        i += 1;
    }
    if i == start {
        return None;
    }
    Some((i, parse_num(&c[start..i])?))
}

fn starts(c: &[char], i: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    i + p.len() <= c.len() && c[i..i + p.len()] == p[..]
}

/// 找到第一个匹配的词：（位置，词，词的字数）
fn find_any<'a>(c: &[char], pats: &[&'a str]) -> Option<(usize, &'a str, usize)> {
    for i in 0..c.len() {
        for p in pats {
            if starts(c, i, p) {
                return Some((i, p, p.chars().count()));
            }
        }
    }
    None
}

type Span = (usize, usize);

/// 「30 分钟后」「一个半小时后」「半小时后」「两天后」→（范围，分钟数）
fn find_relative(c: &[char]) -> Option<(Span, i64)> {
    const UNITS: [(&str, i64); 8] = [
        ("个半小时", 60),
        ("个半钟头", 60),
        ("个小时", 60),
        ("个钟头", 60),
        ("小时", 60),
        ("钟头", 60),
        ("分钟", 1),
        ("天", 1440),
    ];
    const AFTER: [&str; 4] = ["以后", "之后", "过后", "后"];
    for i in 0..c.len() {
        for (unit, mul) in UNITS {
            if !starts(c, i, unit) {
                continue;
            }
            let end_unit = i + unit.chars().count();
            let Some(after) = AFTER.iter().find(|a| starts(c, end_unit, a)) else {
                continue;
            };
            let end = end_unit + after.chars().count();
            let half = unit.contains('半');
            // 「半小时」「半个小时」
            if i >= 1 && c[i - 1] == '半' && !half {
                return Some(((i - 1, end), mul / 2));
            }
            if starts(c, i.saturating_sub(2), "半个") && i >= 2 && !half {
                return Some(((i - 2, end), mul / 2));
            }
            let (s, n) = num_before(c, i)?;
            let total = n as i64 * mul + if half { 30 } else { 0 };
            return (total > 0 && total <= 60 * 24 * 60).then_some(((s, end), total));
        }
        if starts(c, i, "一刻钟") {
            let end = i + 3;
            if let Some(after) = AFTER.iter().find(|a| starts(c, end, a)) {
                return Some(((i, end + after.chars().count()), 15));
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaySpec {
    /// 今天起第几天
    Offset(i64),
    /// 周几（下周 = true）
    Weekday(Weekday, bool),
    MonthDay(u32, u32),
    /// 「X 号」：这个月或下个月
    Day(u32),
}

fn weekday_of(ch: char) -> Option<Weekday> {
    Some(match ch {
        '一' | '1' => Weekday::Mon,
        '二' | '2' => Weekday::Tue,
        '三' | '3' => Weekday::Wed,
        '四' | '4' => Weekday::Thu,
        '五' | '5' => Weekday::Fri,
        '六' | '6' => Weekday::Sat,
        '日' | '天' | '7' => Weekday::Sun,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Period {
    Morning,
    Forenoon,
    Noon,
    Afternoon,
    Dusk,
    Evening,
    Midnight,
}

/// 找日子：今天 / 明天 / 后天 / 下周五 / 周三 / 10 月 30 日 / 15 号；顺带「今晚」「明早」这种带时段的
fn find_day(c: &[char]) -> Option<(Span, DaySpec, Option<Period>)> {
    const WORDS: [(&str, i64, Option<Period>); 10] = [
        ("大后天", 3, None),
        ("后天", 2, None),
        ("明天", 1, None),
        ("明早", 1, Some(Period::Morning)),
        ("明晚", 1, Some(Period::Evening)),
        ("明儿", 1, None),
        ("今天", 0, None),
        ("今晚", 0, Some(Period::Evening)),
        ("今早", 0, Some(Period::Morning)),
        ("今儿", 0, None),
    ];
    for i in 0..c.len() {
        for (w, off, p) in WORDS {
            if starts(c, i, w) {
                return Some(((i, i + w.chars().count()), DaySpec::Offset(off), p));
            }
        }
        for pre in [
            "下周",
            "下星期",
            "下礼拜",
            "这周",
            "本周",
            "周",
            "星期",
            "礼拜",
        ] {
            if starts(c, i, pre) {
                let j = i + pre.chars().count();
                if let Some(wd) = c.get(j).and_then(|&ch| weekday_of(ch)) {
                    return Some((
                        (i, j + 1),
                        DaySpec::Weekday(wd, pre.starts_with('下')),
                        None,
                    ));
                }
            }
        }
        // 「10月30日」「10月30号」
        if c[i] == '月'
            && let Some((s, m)) = num_before(c, i)
            && let Some((e, d)) = num_after(c, i + 1)
            && c.get(e).is_some_and(|&x| x == '日' || x == '号')
            && (1..=12).contains(&m)
            && (1..=31).contains(&d)
        {
            return Some(((s, e + 1), DaySpec::MonthDay(m, d), None));
        }
        // 「15号」（前面不是「月」）
        if c[i] == '号'
            && let Some((s, d)) = num_before(c, i)
            && (s == 0 || c[s - 1] != '月')
            && (1..=31).contains(&d)
        {
            return Some(((s, i + 1), DaySpec::Day(d), None));
        }
    }
    None
}

fn find_period(c: &[char]) -> Option<(Span, Period)> {
    const P: [(&str, Period); 12] = [
        ("早上", Period::Morning),
        ("早晨", Period::Morning),
        ("清早", Period::Morning),
        ("上午", Period::Forenoon),
        ("中午", Period::Noon),
        ("下午", Period::Afternoon),
        ("傍晚", Period::Dusk),
        ("晚上", Period::Evening),
        ("夜里", Period::Evening),
        ("凌晨", Period::Midnight),
        ("半夜", Period::Midnight),
        ("晚", Period::Evening),
    ];
    for i in 0..c.len() {
        for (w, p) in P {
            if starts(c, i, w) {
                return Some(((i, i + w.chars().count()), p));
            }
        }
    }
    None
}

/// 「8点」「8点半」「8点15」「8点15分」「8点一刻」「20:30」→（范围，时，分）
fn find_clock(c: &[char]) -> Option<(Span, u32, u32)> {
    for i in 0..c.len() {
        if c[i] == ':'
            && let Some((s, h)) = num_before(c, i)
            && let Some((e, m)) = num_after(c, i + 1)
            && h < 24
            && m < 60
        {
            return Some(((s, e), h, m));
        }
        if (c[i] == '点' || c[i] == '时')
            && let Some((s, h)) = num_before(c, i)
            && h <= 24
        {
            let j = i + 1;
            if starts(c, j, "半") {
                return Some(((s, j + 1), h % 24, 30));
            }
            if starts(c, j, "一刻") {
                return Some(((s, j + 2), h % 24, 15));
            }
            if starts(c, j, "三刻") {
                return Some(((s, j + 2), h % 24, 45));
            }
            if let Some((e, m)) = num_after(c, j)
                && m < 60
            {
                let end = if c.get(e) == Some(&'分') { e + 1 } else { e };
                return Some(((s, end), h % 24, m));
            }
            // 「时」单独出现（比如「到时」）不算：前面必须是数字，上面已经保证
            return Some(((s, j), h % 24, 0));
        }
    }
    None
}

/// 每天 / 工作日 / 每周X(X…) / 每月X号
fn find_repeat(c: &[char]) -> Option<(Span, Repeat, Option<Period>)> {
    for i in 0..c.len() {
        for (w, p) in [
            ("每天", None),
            ("每日", None),
            ("天天", None),
            ("每晚", Some(Period::Evening)),
            ("每天晚上", Some(Period::Evening)),
            ("每早", Some(Period::Morning)),
        ] {
            if starts(c, i, w) {
                return Some(((i, i + w.chars().count()), Repeat::Daily, p));
            }
        }
        for w in ["每个工作日", "工作日", "周一到周五"] {
            if starts(c, i, w) {
                return Some(((i, i + w.chars().count()), Repeat::Weekdays, None));
            }
        }
        for pre in ["每个星期", "每星期", "每礼拜", "每周"] {
            if starts(c, i, pre) {
                let mut j = i + pre.chars().count();
                let mut days = Vec::new();
                while j < c.len() {
                    if let Some(wd) = weekday_of(c[j]) {
                        let n = wd.number_from_monday() as u8;
                        if !days.contains(&n) {
                            days.push(n);
                        }
                        j += 1;
                    } else if matches!(c[j], '、' | '和' | ',' | '，' | '周') {
                        j += 1;
                    } else if starts(c, j, "星期") {
                        j += 2;
                    } else {
                        break;
                    }
                }
                if days.is_empty() {
                    return None;
                }
                days.sort_unstable();
                return Some(((i, j), Repeat::Weekly { days }, None));
            }
        }
        for pre in ["每个月", "每月"] {
            if starts(c, i, pre) {
                let j = i + pre.chars().count();
                if let Some((e, d)) = num_after(c, j)
                    && c.get(e).is_some_and(|&x| x == '号' || x == '日')
                    && (1..=31).contains(&d)
                {
                    return Some(((i, e + 1), Repeat::Monthly { day: d as u8 }, None));
                }
            }
        }
    }
    None
}

/// 「下次开机时」「明天开机的时候」「打开电脑时」
fn find_startup(c: &[char]) -> Option<Span> {
    let (i, _, n) = find_any(c, &["开机", "打开电脑", "开电脑", "启动电脑", "电脑启动"])?;
    let mut s = i;
    if s >= 2 && starts(c, s - 2, "下次") {
        s -= 2;
    }
    let mut e = i + n;
    for tail in ["的时候", "时候", "以后", "后", "时"] {
        if starts(c, e, tail) {
            e += tail.chars().count();
            break;
        }
    }
    Some((s, e))
}

/// 「下次(我)开始打游戏的时候」→（范围，活动）
fn find_activity(c: &[char]) -> Option<(Span, &'static str)> {
    const A: [(&str, &str); 9] = [
        ("打游戏", "game"),
        ("玩游戏", "game"),
        ("写代码", "coding"),
        ("敲代码", "coding"),
        ("看视频", "video"),
        ("追剧", "video"),
        ("看剧", "video"),
        ("听歌", "music"),
        ("开会", "meeting"),
    ];
    for i in 0..c.len() {
        for (w, act) in A {
            if !starts(c, i, w) {
                continue;
            }
            let e = i + w.chars().count();
            // 必须是「……的时候 / 时」，否则「提醒我去打游戏」会被误认成触发条件
            let tail = ["的时候", "时候", "时"].iter().find(|t| starts(c, e, t))?;
            let mut s = i;
            for pre in ["开始", "我", "下次", "再"] {
                if s >= pre.chars().count() && starts(c, s - pre.chars().count(), pre) {
                    s -= pre.chars().count();
                }
            }
            for pre in ["开始", "我", "下次"] {
                if s >= pre.chars().count() && starts(c, s - pre.chars().count(), pre) {
                    s -= pre.chars().count();
                }
            }
            return Some(((s, e + tail.chars().count()), act));
        }
    }
    None
}

fn hour_with_period(h: u32, p: Option<Period>) -> u32 {
    match p {
        Some(Period::Afternoon | Period::Dusk | Period::Evening) if h < 12 => h + 12,
        Some(Period::Noon) if h < 6 => h + 12,
        Some(Period::Midnight) if h == 12 => 0,
        _ => h,
    }
}

fn default_time(p: Option<Period>) -> (u32, u32) {
    match p {
        Some(Period::Morning) => (8, 0),
        Some(Period::Forenoon) => (10, 0),
        Some(Period::Noon) => (12, 0),
        Some(Period::Afternoon) => (15, 0),
        Some(Period::Dusk) => (18, 0),
        Some(Period::Evening) => (20, 0),
        Some(Period::Midnight) => (0, 0),
        None => (9, 0),
    }
}

/// 某个日子对应的日期（今天起）
fn resolve_day(spec: DaySpec, today: NaiveDate) -> Option<NaiveDate> {
    match spec {
        DaySpec::Offset(n) => Some(today + Duration::days(n)),
        DaySpec::Weekday(wd, next_week) => {
            let cur = today.weekday().num_days_from_monday() as i64;
            let want = wd.num_days_from_monday() as i64;
            let mut diff = want - cur;
            if next_week {
                // 「下周三」= 下一个自然周的周三
                diff += 7;
            } else if diff < 0 {
                diff += 7;
            }
            Some(today + Duration::days(diff))
        }
        DaySpec::MonthDay(m, d) => {
            let this = NaiveDate::from_ymd_opt(today.year(), m, d)?;
            if this < today {
                NaiveDate::from_ymd_opt(today.year() + 1, m, d)
            } else {
                Some(this)
            }
        }
        DaySpec::Day(d) => {
            let this = NaiveDate::from_ymd_opt(today.year(), today.month(), d);
            match this {
                Some(x) if x >= today => Some(x),
                _ => {
                    let (y, m) = if today.month() == 12 {
                        (today.year() + 1, 1)
                    } else {
                        (today.year(), today.month() + 1)
                    };
                    NaiveDate::from_ymd_opt(y, m, d)
                }
            }
        }
    }
}

/// 去掉时间说法和提醒关键词以后剩下的就是内容
fn extract_content(c: &[char], spans: &[Span]) -> String {
    let removed: Vec<bool> = (0..c.len())
        .map(|i| spans.iter().any(|&(s, e)| i >= s && i < e))
        .collect();
    let kept: String = c
        .iter()
        .enumerate()
        .map(|(i, &ch)| if removed[i] { '\u{1}' } else { ch })
        .collect();
    // 关键词后面那段优先（「半小时后提醒我收衣服」）；后面是空的再用前面的（「收衣服的事半小时后提醒我」）
    let (before, after) = INTENT
        .iter()
        .find_map(|k| {
            kept.find(k)
                .map(|i| (kept[..i].to_string(), kept[i + k.len()..].to_string()))
        })
        .unwrap_or((String::new(), kept.clone()));
    let clean = |s: &str| -> String {
        let s: String = s.replace('\u{1}', "");
        let mut t = s.as_str();
        loop {
            let before = t;
            t = t.trim_matches(|c: char| {
                matches!(
                    c,
                    '，' | ','
                        | '。'
                        | '.'
                        | '！'
                        | '!'
                        | '～'
                        | '~'
                        | '、'
                        | '？'
                        | '?'
                        | '的'
                        | '：'
                        | ':'
                )
            });
            for p in [
                "一下", "下", "要", "去", "该", "得", "记得", "请", "帮我", "你", "在", "就", "再",
            ] {
                if let Some(r) = t.strip_prefix(p) {
                    t = r;
                }
            }
            for p in ["哦", "啊", "吧", "呀", "呢", "哈", "嘛", "喔", "啦", "了"] {
                if let Some(r) = t.strip_suffix(p) {
                    t = r;
                }
            }
            if t == before {
                break;
            }
        }
        t.chars().take(100).collect()
    };
    let a = clean(&after);
    if !a.is_empty() {
        return a;
    }
    clean(&before)
}

/// 下一次（严格晚于 `after`）满足重复规则的时间
pub fn next_occurrence(repeat: &Repeat, time: NaiveTime, after: NaiveDateTime) -> NaiveDateTime {
    let mut d = after.date();
    for _ in 0..800 {
        let dt = d.and_time(time);
        let ok = match repeat {
            Repeat::None | Repeat::Daily => true,
            Repeat::Weekdays => d.weekday().num_days_from_monday() < 5,
            Repeat::Weekly { days } => days.contains(&(d.weekday().number_from_monday() as u8)),
            Repeat::Monthly { day } => {
                // 这个月没有那一天（比如 31 号）就用最后一天
                let last = last_day_of_month(d);
                d.day() == (*day as u32).min(last)
            }
        };
        if ok && dt > after {
            return dt;
        }
        d += Duration::days(1);
    }
    after + Duration::days(1)
}

fn last_day_of_month(d: NaiveDate) -> u32 {
    let (y, m) = if d.month() == 12 {
        (d.year() + 1, 1)
    } else {
        (d.year(), d.month() + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1)
        .map(|x| (x - Duration::days(1)).day())
        .unwrap_or(28)
}

/// 解析一句话（`now` 是本地时间）
pub fn parse(text: &str, now: NaiveDateTime) -> Outcome {
    if !looks_like_request(text) {
        return Outcome::NotReminder;
    }
    let s = normalize(text);
    let c: Vec<char> = s.chars().collect();
    let mut spans: Vec<Span> = Vec::new();

    // 1. 进入某种活动时（P3 接上活动感知）
    if let Some((sp, act)) = find_activity(&c) {
        spans.push(sp);
        let content = extract_content(&c, &spans);
        if content.is_empty() {
            return Outcome::Unclear;
        }
        return Outcome::Ok(Parsed {
            content,
            trigger: Trigger::Activity,
            at: None,
            repeat: Repeat::None,
            activity: act.into(),
            relative: false,
        });
    }

    let day = find_day(&c);
    if let Some((sp, ..)) = day {
        spans.push(sp);
    }
    let repeat = find_repeat(&c);
    if let Some((sp, ..)) = &repeat {
        spans.push(*sp);
    }

    // 2. 开机时
    if let Some(sp) = find_startup(&c) {
        spans.push(sp);
        let content = extract_content(&c, &spans);
        if content.is_empty() {
            return Outcome::Unclear;
        }
        let date = day.and_then(|(_, d, _)| resolve_day(d, now.date()));
        let rep = match &repeat {
            Some((_, Repeat::Daily, _)) => Repeat::Daily,
            _ => Repeat::None,
        };
        return Outcome::Ok(Parsed {
            content,
            trigger: Trigger::Startup,
            at: date
                .filter(|d| *d > now.date())
                .map(|d| d.and_time(NaiveTime::MIN)),
            repeat: rep,
            activity: String::new(),
            relative: false,
        });
    }

    // 3. 「X 分钟后」
    if let Some((sp, minutes)) = find_relative(&c) {
        spans.push(sp);
        let content = extract_content(&c, &spans);
        if content.is_empty() {
            return Outcome::Unclear;
        }
        let at = now + Duration::minutes(minutes);
        return Outcome::Ok(Parsed {
            content,
            trigger: Trigger::At,
            at: Some(at),
            repeat: Repeat::None,
            activity: String::new(),
            relative: true,
        });
    }

    // 4. 某天某时 / 每天某时
    let period = find_period(&c);
    if let Some((sp, _)) = period {
        spans.push(sp);
    }
    let clock = find_clock(&c);
    if let Some((sp, ..)) = clock {
        spans.push(sp);
    }
    if day.is_none() && repeat.is_none() && period.is_none() && clock.is_none() {
        return Outcome::Unclear;
    }
    let content = extract_content(&c, &spans);
    if content.is_empty() {
        return Outcome::Unclear;
    }
    let p = period
        .map(|(_, p)| p)
        .or(day.and_then(|(_, _, p)| p))
        .or(repeat.as_ref().and_then(|(_, _, p)| *p));
    let (h, m) = match clock {
        Some((_, h, m)) => (hour_with_period(h, p), m),
        None => default_time(p),
    };
    let Some(time) = NaiveTime::from_hms_opt(h, m, 0) else {
        return Outcome::Unclear;
    };
    if let Some((_, rep, _)) = repeat {
        return Outcome::Ok(Parsed {
            content,
            trigger: Trigger::At,
            at: Some(next_occurrence(&rep, time, now)),
            repeat: rep,
            activity: String::new(),
            relative: false,
        });
    }
    let at = match day {
        Some((_, d, _)) => match resolve_day(d, now.date()) {
            Some(date) => date.and_time(time),
            None => return Outcome::Unclear,
        },
        None => {
            // 只说了几点：取最近的将来那个（没说上午下午时，12 点以内的也可能是下午）
            let today = now.date().and_time(time);
            if today > now {
                today
            } else if p.is_none() && h < 12 && today + Duration::hours(12) > now {
                today + Duration::hours(12)
            } else {
                today + Duration::days(1)
            }
        }
    };
    if at <= now {
        return Outcome::Unclear;
    }
    Outcome::Ok(Parsed {
        content,
        trigger: Trigger::At,
        at: Some(at),
        repeat: Repeat::None,
        activity: String::new(),
        relative: false,
    })
}

const WD: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];

/// 给用户看的「什么时候」：「30 分钟后（15:42）」「明天 9:00」「每周五 20:00」「下次开机时」
pub fn describe(
    trigger: Trigger,
    at: Option<NaiveDateTime>,
    repeat: &Repeat,
    activity: &str,
    relative: bool,
    now: NaiveDateTime,
) -> String {
    match trigger {
        Trigger::Activity => format!("下次你开始{}时", activity_name(activity)),
        Trigger::Startup => {
            let prefix = match at {
                Some(d) if d.date() == now.date() + Duration::days(1) => "明天".to_string(),
                Some(d) if d.date() > now.date() => {
                    format!("{}月{}日", d.month(), d.day())
                }
                _ => String::new(),
            };
            if *repeat == Repeat::Daily {
                "每天开机时".into()
            } else if prefix.is_empty() {
                "下次开机时".into()
            } else {
                format!("{prefix}开机时")
            }
        }
        Trigger::At => {
            let Some(at) = at else {
                return String::new();
            };
            let hm = format!("{}:{:02}", at.hour(), at.minute());
            match repeat {
                Repeat::Daily => format!("每天 {hm}"),
                Repeat::Weekdays => format!("工作日每天 {hm}"),
                Repeat::Weekly { days } => {
                    let names: Vec<&str> = days
                        .iter()
                        .filter_map(|d| WD.get((*d as usize).wrapping_sub(1)).copied())
                        .collect();
                    format!("每周{} {hm}", names.join("、"))
                }
                Repeat::Monthly { day } => format!("每月 {day} 号 {hm}"),
                Repeat::None => {
                    // 四舍五入到分钟（「30 分钟后」的提醒时间取整到分，现在可能是 15:12:30）
                    let mins = ((at - now).num_seconds() + 30) / 60;
                    if relative && mins < 24 * 60 {
                        if mins < 60 {
                            format!("{} 分钟后（{hm}）", mins.max(1))
                        } else if mins % 60 == 0 {
                            format!("{} 小时后（{hm}）", mins / 60)
                        } else {
                            format!("{} 小时 {} 分钟后（{hm}）", mins / 60, mins % 60)
                        }
                    } else if at.date() == now.date() {
                        format!("今天 {hm}")
                    } else if at.date() == now.date() + Duration::days(1) {
                        format!("明天 {hm}")
                    } else if at.date() == now.date() + Duration::days(2) {
                        format!("后天 {hm}")
                    } else {
                        let wd = WD[at.weekday().num_days_from_monday() as usize];
                        format!("{}月{}日（周{wd}）{hm}", at.month(), at.day())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-28 是星期三
    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 28)
            .unwrap()
            .and_hms_opt(15, 12, 30)
            .unwrap()
    }

    fn dt(m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    /// 相对时间：从 15:12:30 算起，秒数保留
    fn rel(m: u32, d: u32, h: u32, min: u32) -> Option<NaiveDateTime> {
        Some(dt(m, d, h, min) + Duration::seconds(30))
    }

    fn ok(text: &str) -> Parsed {
        match parse(text, now()) {
            Outcome::Ok(p) => p,
            other => panic!("「{text}」应该能解析，实际 {other:?}"),
        }
    }

    #[test]
    fn numbers() {
        let n = |s: &str| parse_num(&s.chars().collect::<Vec<_>>());
        assert_eq!(n("15"), Some(15));
        assert_eq!(n("十五"), Some(15));
        assert_eq!(n("二十"), Some(20));
        assert_eq!(n("二十三"), Some(23));
        assert_eq!(n("十"), Some(10));
        assert_eq!(n("两"), Some(2));
        assert_eq!(n("一二三"), None);
    }

    #[test]
    fn intent_detection() {
        assert!(looks_like_request("半小时后提醒我收衣服"));
        assert!(looks_like_request("记得提醒我吃药"));
        assert!(!looks_like_request("你提醒我了，谢谢"));
        assert!(!looks_like_request("上次提醒我的事做完了"));
        assert!(!looks_like_request("今天天气不错"));
        assert_eq!(parse("今天天气不错", now()), Outcome::NotReminder);
    }

    #[test]
    fn relative_times() {
        let p = ok("半小时后提醒我收衣服");
        assert_eq!(p.content, "收衣服");
        assert_eq!(p.at, rel(10, 28, 15, 42));
        assert!(p.relative);
        assert_eq!(ok("30分钟后提醒我喝水").at, rel(10, 28, 15, 42));
        assert_eq!(ok("三十分钟以后提醒我喝水").at, rel(10, 28, 15, 42));
        assert_eq!(ok("两个小时后提醒我出门").at, rel(10, 28, 17, 12));
        assert_eq!(ok("一个半小时后提醒我关火").at, rel(10, 28, 16, 42));
        assert_eq!(ok("一刻钟后提醒我看锅").at, rel(10, 28, 15, 27));
        assert_eq!(ok("提醒我20分钟后去取快递").content, "取快递");
        assert_eq!(ok("提醒我20分钟后去取快递").at, rel(10, 28, 15, 32));
        assert_eq!(ok("3天后提醒我还书").at, rel(10, 31, 15, 12));
    }

    #[test]
    fn day_and_clock() {
        let p = ok("明天早上8点提醒我交作业");
        assert_eq!(
            (p.content.as_str(), p.at),
            ("交作业", Some(dt(10, 29, 8, 0)))
        );
        assert_eq!(ok("明天下午3点半提醒我开会").at, Some(dt(10, 29, 15, 30)));
        assert_eq!(ok("今晚9点提醒我洗衣服").at, Some(dt(10, 28, 21, 0)));
        assert_eq!(
            ok("提醒我明天交作业").at,
            Some(dt(10, 29, 9, 0)),
            "只说日子：早上 9 点"
        );
        assert_eq!(ok("提醒我明天交作业").content, "交作业");
        assert_eq!(ok("周五晚上提醒我倒垃圾").at, Some(dt(10, 30, 20, 0)));
        assert_eq!(ok("下周一上午10点提醒我体检").at, Some(dt(11, 2, 10, 0)));
        assert_eq!(ok("11月3号提醒我交房租").at, Some(dt(11, 3, 9, 0)));
        assert_eq!(ok("10月30日20:30提醒我看比赛").at, Some(dt(10, 30, 20, 30)));
        assert_eq!(ok("晚上8点15分提醒我吃药").at, Some(dt(10, 28, 20, 15)));
        // 只说几点：现在 15:12，「5点」= 今天 17:00；「3点」= 今天已过、下午也过了 → 明天 3:00
        assert_eq!(ok("5点提醒我下班").at, Some(dt(10, 28, 17, 0)));
        assert_eq!(ok("3点提醒我起床").at, Some(dt(10, 29, 3, 0)));
        assert_eq!(ok("20:00提醒我打电话").at, Some(dt(10, 28, 20, 0)));
    }

    #[test]
    fn repeats() {
        let p = ok("每周五晚上提醒我倒垃圾");
        assert_eq!(p.content, "倒垃圾");
        assert_eq!(p.repeat, Repeat::Weekly { days: vec![5] });
        assert_eq!(p.at, Some(dt(10, 30, 20, 0)));
        let p = ok("每天早上7点半提醒我吃早饭");
        assert_eq!(
            (p.repeat.clone(), p.at),
            (Repeat::Daily, Some(dt(10, 29, 7, 30)))
        );
        assert_eq!(ok("工作日每天9点提醒我打卡").repeat, Repeat::Weekdays);
        assert_eq!(ok("工作日每天9点提醒我打卡").at, Some(dt(10, 29, 9, 0)));
        assert_eq!(
            ok("每周一三五晚上提醒我健身").repeat,
            Repeat::Weekly {
                days: vec![1, 3, 5]
            }
        );
        assert_eq!(ok("每周一三五晚上提醒我健身").at, Some(dt(10, 28, 20, 0)));
        let p = ok("每月15号提醒我还信用卡");
        assert_eq!(
            (p.repeat.clone(), p.at),
            (Repeat::Monthly { day: 15 }, Some(dt(11, 15, 9, 0)))
        );
    }

    #[test]
    fn startup_and_activity() {
        let p = ok("明天开机时提醒我交作业");
        assert_eq!(p.trigger, Trigger::Startup);
        assert_eq!(p.content, "交作业");
        assert_eq!(p.at, Some(dt(10, 29, 0, 0)));
        let p = ok("下次开机的时候提醒我备份照片");
        assert_eq!((p.trigger, p.at), (Trigger::Startup, None));
        assert_eq!(p.content, "备份照片");
        let p = ok("下次我开始打游戏的时候提醒我先写作业");
        assert_eq!(p.trigger, Trigger::Activity);
        assert_eq!(p.activity, "game");
        assert_eq!(p.content, "先写作业");
        // 「提醒我去打游戏」不是活动触发
        assert_eq!(parse("提醒我去打游戏", now()), Outcome::Unclear);
    }

    #[test]
    fn unclear_requests() {
        assert_eq!(
            parse("提醒我收衣服", now()),
            Outcome::Unclear,
            "没说什么时候"
        );
        assert_eq!(
            parse("半小时后提醒我", now()),
            Outcome::Unclear,
            "没说提醒什么"
        );
    }

    #[test]
    fn next_occurrence_rules() {
        let t = NaiveTime::from_hms_opt(20, 0, 0).unwrap();
        // 周三 15:12 之后
        assert_eq!(next_occurrence(&Repeat::Daily, t, now()), dt(10, 28, 20, 0));
        assert_eq!(
            next_occurrence(&Repeat::Daily, t, dt(10, 28, 20, 0)),
            dt(10, 29, 20, 0)
        );
        assert_eq!(
            next_occurrence(&Repeat::Weekdays, t, dt(10, 30, 21, 0)),
            dt(11, 2, 20, 0),
            "周五晚上之后 → 下周一"
        );
        assert_eq!(
            next_occurrence(&Repeat::Monthly { day: 31 }, t, dt(11, 1, 0, 0)),
            dt(11, 30, 20, 0),
            "11 月没有 31 号 → 30 号"
        );
    }

    #[test]
    fn describe_when() {
        let n = now();
        assert_eq!(
            describe(
                Trigger::At,
                Some(dt(10, 28, 15, 42)),
                &Repeat::None,
                "",
                true,
                n
            ),
            "30 分钟后（15:42）"
        );
        assert_eq!(
            describe(
                Trigger::At,
                Some(dt(10, 28, 17, 12)),
                &Repeat::None,
                "",
                true,
                n
            ),
            "2 小时后（17:12）"
        );
        assert_eq!(
            describe(
                Trigger::At,
                Some(dt(10, 29, 8, 0)),
                &Repeat::None,
                "",
                false,
                n
            ),
            "明天 8:00"
        );
        assert_eq!(
            describe(
                Trigger::At,
                Some(dt(11, 3, 9, 0)),
                &Repeat::None,
                "",
                false,
                n
            ),
            "11月3日（周二）9:00"
        );
        assert_eq!(
            describe(
                Trigger::At,
                Some(dt(10, 30, 20, 0)),
                &Repeat::Weekly { days: vec![1, 5] },
                "",
                false,
                n
            ),
            "每周一、五 20:00"
        );
        assert_eq!(
            describe(
                Trigger::Startup,
                Some(dt(10, 29, 0, 0)),
                &Repeat::None,
                "",
                false,
                n
            ),
            "明天开机时"
        );
        assert_eq!(
            describe(Trigger::Startup, None, &Repeat::None, "", false, n),
            "下次开机时"
        );
        assert_eq!(
            describe(Trigger::Activity, None, &Repeat::None, "game", false, n),
            "下次你开始玩游戏时"
        );
    }
}
