//! 没有模型（或模型调用失败）时的本地理解：从用户写的计划里认出内容条目和约束。
//! 只求「大差不差能排」：编号列表、「第1-8章」这类范围、「共300页」「3000个单词」、「(2小时)」这类时长、
//! 「每天1小时」「周末2小时」「周一三五」「12月底前」「一个月内」「晚上8点」。认不出的照样能在界面上手动改。

use super::Mode;
use chrono::{Datelike, Duration, NaiveDate};

/// 认出来的东西
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hints {
    pub title: String,
    pub emoji: String,
    /// 认不出时 None（交给调用方决定）
    pub mode: Option<Mode>,
    pub week_minutes: Option<[u32; 7]>,
    pub end_day: Option<NaiveDate>,
    pub remind_at: Option<String>,
    /// 内容条目：（标题、分钟，0 = 没说）
    pub items: Vec<(String, u32)>,
    /// 说了总共要多少时间（分钟）
    pub total_minutes: Option<u32>,
    /// 说了每一部分要多久（「每章2小时」）
    pub per_item: Option<u32>,
    /// 写了明确的时间范围（「2026年11月9日〜2027年6月30日」）时的开始日
    pub start_day: Option<NaiveDate>,
}

const SEP: &[char] = &['，', ',', '。', '.', '；', ';', '！', '!', '？', '?', '\n'];
/// 「几到几」的连接符
const RANGE_SEP: &[&str] = &["〜", "～", "~", "-", "–", "—", "到", "至"];

fn cn_val(c: char) -> Option<u32> {
    Some(match c {
        '零' | '〇' => 0,
        '一' => 1,
        '二' | '两' => 2,
        '三' => 3,
        '四' => 4,
        '五' => 5,
        '六' => 6,
        '七' => 7,
        '八' => 8,
        '九' => 9,
        _ => return None,
    })
}

fn is_cn_num(c: char) -> bool {
    cn_val(c).is_some() || matches!(c, '十' | '百' | '千')
}

fn is_num(c: char) -> bool {
    c.is_ascii_digit() || is_cn_num(c)
}

/// 中文数字（到 9999）：「十五」「二十」「三百」「一千二」「一百零五」
fn parse_cn(s: &[char]) -> Option<u32> {
    let mut total = 0u32;
    let mut cur: Option<u32> = None;
    let mut last_unit = 1u32;
    let mut zero = false;
    for &c in s {
        if let Some(v) = cn_val(c) {
            if v == 0 {
                zero = true;
            }
            cur = Some(v);
        } else {
            let unit = match c {
                '十' => 10,
                '百' => 100,
                '千' => 1000,
                _ => return None,
            };
            total += cur.unwrap_or(1) * unit;
            cur = None;
            last_unit = unit;
        }
    }
    if let Some(v) = cur {
        // 「一千二」= 1200，「三百五」= 350；「一百零五」= 105
        total += if last_unit >= 100 && !zero {
            v * (last_unit / 10)
        } else {
            v
        };
    }
    Some(total)
}

/// 从 `i` 开始读一个数（阿拉伯数字可带小数；中文数字；「一个半」= 1.5）。返回（数值，结束位置）
fn num_at(c: &[char], i: usize) -> Option<(f64, usize)> {
    let mut j = i;
    let mut v = if c.get(i).is_some_and(|x| x.is_ascii_digit()) {
        while j < c.len()
            && (c[j].is_ascii_digit()
                || (c[j] == '.' && c.get(j + 1).is_some_and(|x| x.is_ascii_digit())))
        {
            j += 1;
        }
        c[i..j].iter().collect::<String>().parse::<f64>().ok()?
    } else {
        while j < c.len() && is_cn_num(c[j]) {
            j += 1;
        }
        if j == i {
            return None;
        }
        parse_cn(&c[i..j])? as f64
    };
    if c.get(j) == Some(&'个') && c.get(j + 1) == Some(&'半') {
        v += 0.5;
        j += 2;
    } else if c.get(j) == Some(&'个') {
        j += 1;
    } else if c.get(j) == Some(&'半') {
        v += 0.5;
        j += 1;
    }
    Some((v, j))
}

fn starts(c: &[char], i: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    i + p.len() <= c.len()
        && c[i..i + p.len()]
            .iter()
            .zip(&p)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn starts_any(c: &[char], i: usize, pats: &[&str]) -> Option<usize> {
    pats.iter()
        .find(|p| starts(c, i, p))
        .map(|p| p.chars().count())
}

/// 在 `i` 处的一段时长：「1小时」「一个半小时」「90分钟」「1.5h」「30min」「半小时」「一刻钟」。返回（分钟，结束位置）
fn duration_at(c: &[char], i: usize) -> Option<(u32, usize)> {
    if let Some(n) = starts_any(c, i, &["半小时", "半个小时", "半个钟头"]) {
        return Some((30, i + n));
    }
    if let Some(n) = starts_any(c, i, &["一刻钟"]) {
        return Some((15, i + n));
    }
    let (mut v, mut j) = num_at(c, i)?;
    // 「1〜2小时」「30-45分钟」：取中间
    if let Some(n) = starts_any(c, j, RANGE_SEP)
        && let Some((v2, j2)) = num_at(c, j + n)
        && v2 > v
    {
        v = (v + v2) / 2.0;
        j = j2;
    }
    let mut k = j;
    while c.get(k) == Some(&' ') {
        k += 1;
    }
    if let Some(n) = starts_any(c, k, &["小时", "钟头", "hours", "hour", "hrs", "hr", "h"]) {
        // 「h」后面紧跟字母的不算（比如 hello）
        if c.get(k + n).is_some_and(|x| x.is_ascii_alphabetic()) {
            return None;
        }
        return Some(((v * 60.0).round() as u32, k + n));
    }
    if let Some(n) = starts_any(c, k, &["分钟", "minutes", "minute", "mins", "min", "分"]) {
        if c.get(k + n).is_some_and(|x| x.is_ascii_alphabetic()) {
            return None;
        }
        return Some((v.round() as u32, k + n));
    }
    None
}

/// 一段文字里第一个时长：（分钟，起点，终点）
fn first_duration(c: &[char]) -> Option<(u32, usize, usize)> {
    (0..c.len()).find_map(|i| {
        if i > 0 && is_num(c[i - 1]) {
            return None;
        }
        duration_at(c, i).map(|(m, e)| (m, i, e))
    })
}

/// 一段话里第一个时长（分钟）
pub fn first_minutes(text: &str) -> Option<u32> {
    let c: Vec<char> = text.chars().collect();
    first_duration(&c).map(|x| x.0)
}

/// 从 `i` 往后、不跨过标点，找第一个时长（最多看 `span` 个字）
fn duration_after(c: &[char], i: usize, span: usize) -> Option<u32> {
    let end = (i + span).min(c.len());
    (i..end)
        .take_while(|&k| !SEP.contains(&c[k]))
        .find_map(|k| duration_at(c, k).map(|(m, _)| m))
}

fn find(c: &[char], pat: &str) -> Option<usize> {
    (0..c.len()).find(|&i| starts(c, i, pat))
}

fn find_all(c: &[char], pat: &str) -> Vec<usize> {
    (0..c.len()).filter(|&i| starts(c, i, pat)).collect()
}

fn has(c: &[char], pats: &[&str]) -> bool {
    pats.iter().any(|p| find(c, p).is_some())
}

fn wd(x: char) -> Option<usize> {
    Some(match x {
        '一' => 0,
        '二' => 1,
        '三' => 2,
        '四' => 3,
        '五' => 4,
        '六' => 5,
        '日' | '天' | '七' => 6,
        _ => return None,
    })
}

/// 「周一三五」「星期二、四」「周一到周五」→（星期几（0 = 周一），结束位置）。只说了一天的不算。
fn weekday_set(c: &[char]) -> Option<(Vec<usize>, usize)> {
    for i in 0..c.len() {
        let Some(n) = starts_any(c, i, &["星期", "礼拜", "周"]) else {
            continue;
        };
        let mut j = i + n;
        // 「每周」「一周」「周末」不是在列星期几
        let Some(first) = c.get(j).and_then(|x| wd(*x)) else {
            continue;
        };
        let k = j + 1;
        if let Some(m) = starts_any(c, k, &["到", "至", "-", "~", "—"]) {
            let mut e = k + m;
            if let Some(m2) = starts_any(c, e, &["星期", "礼拜", "周"]) {
                e += m2;
            }
            if let Some(last) = c.get(e).and_then(|x| wd(*x))
                && last >= first
            {
                return Some(((first..=last).collect(), e + 1));
            }
        }
        let mut days = vec![first];
        j += 1;
        while j < c.len() {
            if let Some(d) = wd(c[j]) {
                days.push(d);
                j += 1;
            } else if matches!(c[j], '、' | '和' | '/' | ',' | '，')
                && c.get(j + 1).and_then(|x| wd(*x)).is_some()
            {
                j += 1;
            } else if let Some(m) = starts_any(c, j, &["、周", "和周", "、星期", "周"]) {
                if c.get(j + m).and_then(|x| wd(*x)).is_some() {
                    j += m;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        days.sort_unstable();
        days.dedup();
        if days.len() >= 2 {
            return Some((days, j));
        }
    }
    None
}

fn keyword_duration(c: &[char], keys: &[&str], span: usize) -> Option<u32> {
    keys.iter()
        .flat_map(|k| {
            find_all(c, k)
                .into_iter()
                .map(move |i| i + k.chars().count())
        })
        .find_map(|i| duration_after(c, i, span))
}

/// 每周每天的分钟
fn week_minutes(c: &[char]) -> Option<[u32; 7]> {
    let daily = keyword_duration(c, &["每天", "每日", "一天", "每晚", "天天", "每个晚上"], 14);
    let each = keyword_duration(c, &["每次", "一次"], 10);
    let weekend = find_all(c, "周末")
        .into_iter()
        .chain(find_all(c, "双休"))
        .find_map(|i| {
            if has(
                &c[i..(i + 6).min(c.len())],
                &["不学", "不做", "休息", "不排", "放假", "除外", "不用"],
            ) {
                Some(0)
            } else {
                duration_after(c, i + 2, 12)
            }
        });
    // 「周末可以多一些」：没说多久，按平时的一倍半
    let weekend_more = weekend.is_none()
        && find_all(c, "周末").into_iter().any(|i| {
            has(
                &c[(i + 2).min(c.len())..(i + 8).min(c.len())],
                &["多", "长"],
            )
        });
    let weekday_mins = keyword_duration(c, &["工作日"], 12);
    let weekly_total = keyword_duration(c, &["每周", "一周", "每星期", "一星期"], 12);
    let times = ["每周", "一周", "每星期", "一星期"].iter().find_map(|k| {
        let i = find(c, k)? + k.chars().count();
        let (v, j) = num_at(c, i)?;
        starts_any(c, j, &["次", "天"]).map(|_| v as usize)
    });
    let set = weekday_set(c);
    let set_mins = set.as_ref().and_then(|(_, e)| duration_after(c, *e, 14));
    let only_weekdays = has(c, &["工作日", "周一到周五", "周一至周五", "上班的日子"])
        && weekend.is_none()
        && (daily.is_none() || daily == weekday_mins || weekday_mins.is_none());
    if daily.is_none()
        && each.is_none()
        && weekend.is_none()
        && weekday_mins.is_none()
        && weekly_total.is_none()
        && times.is_none()
        && set.is_none()
        && !only_weekdays
    {
        return None;
    }
    // 哪几天
    let mut on = [true; 7];
    if let Some((s, _)) = &set {
        on = [false; 7];
        for d in s {
            on[*d] = true;
        }
    } else if let Some(n) = times.filter(|n| (1..7).contains(n)) {
        // 每周 N 次：尽量隔开
        on = [false; 7];
        let picks: &[usize] = match n {
            1 => &[5],
            2 => &[1, 4],
            3 => &[0, 2, 4],
            4 => &[0, 1, 3, 4],
            5 => &[0, 1, 2, 3, 4],
            _ => &[0, 1, 2, 3, 4, 5],
        };
        for d in picks {
            on[*d] = true;
        }
    } else if only_weekdays || weekend == Some(0) {
        on[5] = false;
        on[6] = false;
    }
    let days_on = on.iter().filter(|x| **x).count().max(1) as u32;
    let base = daily
        .or(each)
        .or(set_mins)
        .or(weekday_mins)
        .or(weekly_total
            .filter(|_| times.is_none())
            .map(|t| (t / days_on).max(10)))
        .unwrap_or(60);
    let weekend = if weekend_more {
        Some((base * 3 / 2).div_ceil(5) * 5)
    } else {
        weekend
    };
    let mut w = [0u32; 7];
    for (d, m) in w.iter_mut().enumerate() {
        if !on[d] {
            continue;
        }
        *m = if d >= 5 {
            weekend.filter(|x| *x > 0).unwrap_or(base)
        } else {
            weekday_mins.unwrap_or(base)
        };
    }
    if w.iter().all(|m| *m == 0) {
        return None;
    }
    Some(w.map(|m| (m.min(720) / 5) * 5))
}

fn last_of_month(y: i32, m: u32) -> Option<NaiveDate> {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).map(|d| d - Duration::days(1))
}

/// 截止日期
fn end_day(c: &[char], today: NaiveDate) -> Option<NaiveDate> {
    let future = |m: u32, d: u32| -> Option<NaiveDate> {
        let x = NaiveDate::from_ymd_opt(today.year(), m, d)?;
        if x >= today {
            Some(x)
        } else {
            NaiveDate::from_ymd_opt(today.year() + 1, m, d)
        }
    };
    for i in 0..c.len() {
        if i > 0 && is_num(c[i - 1]) {
            continue;
        }
        // 「12月20日」「12月20号」「12月底」「12月末」
        if let Some((m, j)) = num_at(c, i).filter(|(_, j)| c.get(*j) == Some(&'月')) {
            let m = m as u32;
            if !(1..=12).contains(&m) {
                continue;
            }
            if starts_any(c, j + 1, &["底", "末", "份结束", "结束"]).is_some() {
                let y = if m >= today.month() {
                    today.year()
                } else {
                    today.year() + 1
                };
                return last_of_month(y, m);
            }
            if let Some((d, k)) = num_at(c, j + 1)
                && matches!(c.get(k), Some('日' | '号'))
                && let Some(x) = future(m, d as u32)
            {
                return Some(x);
            }
        }
        // 「12/20」
        if c[i].is_ascii_digit()
            && let Some((m, j)) = num_at(c, i)
            && c.get(j) == Some(&'/')
            && let Some((d, _)) = num_at(c, j + 1)
            && let Some(x) = future(m as u32, d as u32)
        {
            return Some(x);
        }
    }
    if has(c, &["年底", "年末", "今年内", "今年之内"]) {
        return NaiveDate::from_ymd_opt(today.year(), 12, 31);
    }
    if has(c, &["下个月底", "下月底"]) {
        let (y, m) = if today.month() == 12 {
            (today.year() + 1, 1)
        } else {
            (today.year(), today.month() + 1)
        };
        return last_of_month(y, m);
    }
    if has(c, &["月底", "月末", "这个月内", "本月内"]) {
        return last_of_month(today.year(), today.month());
    }
    // 「两周内」「一个月学完」「30天」「三个星期」
    for i in 0..c.len() {
        if i > 0 && (is_num(c[i - 1]) || c[i - 1] == '第') {
            continue;
        }
        let Some((v, j)) = num_at(c, i) else { continue };
        let (days, n) = if let Some(n) = starts_any(c, j, &["个星期", "星期", "礼拜", "周"])
        {
            (v * 7.0, n)
        } else if let Some(n) = starts_any(c, j, &["个月", "月"]) {
            (v * 30.0, n)
        } else if let Some(n) = starts_any(c, j, &["天"]) {
            (v, n)
        } else {
            continue;
        };
        let after = j + n;
        if has(
            &c[after..(after + 4).min(c.len())],
            &[
                "内", "之内", "以内", "学完", "做完", "看完", "读完", "完成", "搞定", "时间",
                "后考",
            ],
        ) && (1.0..=730.0).contains(&days)
        {
            return Some(today + Duration::days(days as i64 - 1));
        }
    }
    None
}

/// 在 `i` 处的一个完整日期：「2026年11月9日」「2027年6月」「2026-11-09」「2026/11/9」「11月9日」。返回（日期，结束位置）。
/// 只写到月份的：`month_end` 取月底，否则取月初；没写年份的：取 `base` 前一个月之后最近的那个。
fn date_at(c: &[char], i: usize, base: NaiveDate, month_end: bool) -> Option<(NaiveDate, usize)> {
    if !c.get(i)?.is_ascii_digit() {
        return None;
    }
    let (a, j) = num_at(c, i)?;
    if (2000.0..2100.0).contains(&a) {
        let y = a as i32;
        if c.get(j) == Some(&'年') {
            let (m, k) = num_at(c, j + 1)?;
            if c.get(k) != Some(&'月') || !(1.0..=12.0).contains(&m) {
                return None;
            }
            let m = m as u32;
            if let Some((d, e)) = num_at(c, k + 1)
                && matches!(c.get(e), Some('日' | '号'))
            {
                return NaiveDate::from_ymd_opt(y, m, d as u32).map(|x| (x, e + 1));
            }
            let end = month_end || starts_any(c, k + 1, &["底", "末"]).is_some();
            let x = if end {
                last_of_month(y, m)?
            } else {
                NaiveDate::from_ymd_opt(y, m, 1)?
            };
            return Some((x, k + 1));
        }
        if matches!(c.get(j), Some('-' | '/' | '.')) {
            let (m, k) = num_at(c, j + 1)?;
            if !matches!(c.get(k), Some('-' | '/' | '.')) {
                return None;
            }
            let (d, e) = num_at(c, k + 1)?;
            return NaiveDate::from_ymd_opt(y, m as u32, d as u32).map(|x| (x, e));
        }
        return None;
    }
    if c.get(j) == Some(&'月')
        && let Some((d, e)) = num_at(c, j + 1)
        && matches!(c.get(e), Some('日' | '号'))
    {
        let x = NaiveDate::from_ymd_opt(base.year(), a as u32, d as u32)?;
        let x = if x < base - Duration::days(31) {
            NaiveDate::from_ymd_opt(base.year() + 1, a as u32, d as u32)?
        } else {
            x
        };
        return Some((x, e + 1));
    }
    None
}

/// 写明了的时间范围：「2026年11月9日〜2027年6月30日」「11月9日到明年6月30日」「2026-11-09 ~ 2027-06-30」
pub fn date_range(c: &[char], today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    for i in 0..c.len() {
        if i > 0 && c[i - 1].is_ascii_digit() {
            continue;
        }
        let Some((a, j)) = date_at(c, i, today, false) else {
            continue;
        };
        let mut k = j;
        while c.get(k) == Some(&' ') {
            k += 1;
        }
        let Some(n) = starts_any(c, k, RANGE_SEP) else {
            continue;
        };
        k += n;
        while matches!(c.get(k), Some(' ' | '—' | '-')) {
            k += 1;
        }
        if let Some(n) = starts_any(c, k, &["明年", "次年"]) {
            k += n;
        }
        if let Some((b, _)) = date_at(c, k, a, true)
            && b > a
        {
            return Some((a, b));
        }
    }
    None
}

/// 分节写的计划（「【我的背景】」「## 准备内容」）：（节的标题，正文）。没分节返回空。
pub fn sections(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut lead = String::new();
    for line in text.lines() {
        let t = line.trim();
        let head = if let Some(r) = t.strip_prefix('【')
            && let Some(e) = r.find('】')
        {
            Some((
                r[..e].trim().to_string(),
                r[e + '】'.len_utf8()..].trim().to_string(),
            ))
        } else if t.starts_with('#') {
            Some((t.trim_start_matches('#').trim().to_string(), String::new()))
        } else {
            None
        };
        match head {
            Some((h, rest)) => out.push((h, rest)),
            None => match out.last_mut() {
                Some((_, body)) => {
                    body.push('\n');
                    body.push_str(line);
                }
                None => {
                    lead.push_str(line);
                    lead.push('\n');
                }
            },
        }
    }
    if out.len() < 2 {
        return Vec::new();
    }
    if !lead.trim().is_empty() {
        out.insert(0, (String::new(), lead));
    }
    out
}

/// 不是「要做的内容」的节：背景、目标、约束、输出要求……
const SKIP_HEADS: &[&str] = &[
    "背景", "目标", "约束", "要求", "输出", "说明", "限制", "条件", "情况", "现状", "备注", "关于",
];
/// 「要做的内容」的节
const WORK_HEADS: &[&str] = &[
    "内容", "准备", "任务", "要做", "学习", "科目", "方向", "安排", "事项", "待办", "清单",
];

pub fn is_work_head(h: &str) -> bool {
    !SKIP_HEADS.iter().any(|w| h.contains(w)) && WORK_HEADS.iter().any(|w| h.contains(w))
}

pub fn is_skip_head(h: &str) -> bool {
    SKIP_HEADS.iter().any(|w| h.contains(w))
}

/// 分了节的计划只看「要做的内容」那几节（背景里列的经历不是要做的事）；认不出哪节是内容就去掉背景、要求这些节
pub fn work_text(text: &str) -> String {
    let s = sections(text);
    if s.is_empty() {
        return text.to_string();
    }
    let pick = |f: &dyn Fn(&str) -> bool| -> String {
        s.iter()
            .filter(|(h, _)| f(h))
            .map(|(_, b)| b.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let work = pick(&|h| is_work_head(h));
    if !work.trim().is_empty() {
        return work;
    }
    let rest = pick(&|h| !h.is_empty() && !is_skip_head(h));
    if rest.trim().is_empty() {
        text.to_string()
    } else {
        rest
    }
}

const UNITS: &[&str] = &[
    "单元", "部分", "章", "节", "讲", "课", "集", "篇", "关", "期",
];
const EN_UNITS: &[&str] = &["chapter", "unit", "lesson", "part", "lecture", "module"];

fn capitalize(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f
            .to_uppercase()
            .chain(ch.flat_map(|c| c.to_lowercase()))
            .collect(),
        None => String::new(),
    }
}

/// 「第1-8章」「1~12讲」「第一章到第八章」「Unit 1-10」「共12讲」→ 条目
fn range_items(c: &[char]) -> Vec<(String, u32)> {
    for i in 0..c.len() {
        // 英文：Unit 1-10
        if let Some(n) = starts_any(c, i, EN_UNITS) {
            let word: String = c[i..i + n].iter().collect();
            let mut j = i + n;
            while c.get(j) == Some(&' ') {
                j += 1;
            }
            if let Some((a, k)) = num_at(c, j)
                && let Some(m) = starts_any(c, k, &["-", "~", "到", "至", "—"])
                && let Some((b, _)) = num_at(c, k + m)
                && a >= 1.0
                && b > a
                && b - a < 300.0
            {
                let w = capitalize(&word);
                return (a as u32..=b as u32)
                    .map(|x| (format!("{w} {x}"), 0))
                    .collect();
            }
        }
        // 中文：第1-8章 / 1到8章 / 第一章到第八章
        if c[i] != '第' && i > 0 && (is_num(c[i - 1]) || c[i - 1] == '第') {
            continue;
        }
        let start = if c[i] == '第' { i + 1 } else { i };
        let Some((a, k)) = num_at(c, start) else {
            continue;
        };
        let (k, unit_mid) = match UNITS.iter().find(|u| starts(c, k, u)) {
            Some(u) => (k + u.chars().count(), Some(*u)),
            None => (k, None),
        };
        let Some(m) = starts_any(
            c,
            k,
            &[
                "到第", "至第", "-第", "~第", "—第", "到", "至", "-", "~", "—", "－",
            ],
        ) else {
            continue;
        };
        let Some((b, e)) = num_at(c, k + m) else {
            continue;
        };
        let Some(unit) = UNITS.iter().find(|u| starts(c, e, u)).copied().or(unit_mid) else {
            continue;
        };
        if a >= 1.0 && b > a && b - a < 300.0 {
            return (a as u32..=b as u32)
                .map(|x| (format!("第{x}{unit}"), 0))
                .collect();
        }
    }
    // 「共12讲」「一共8章」「总共20集」
    for k in ["一共", "总共", "共", "全书"] {
        for i in find_all(c, k) {
            let j = i + k.chars().count();
            if let Some((n, e)) = num_at(c, j)
                && let Some(u) = UNITS.iter().find(|u| starts(c, e, u))
                && (1.0..=300.0).contains(&n)
            {
                return (1..=n as u32).map(|x| (format!("第{x}{u}"), 0)).collect();
            }
        }
    }
    Vec::new()
}

/// 「每天30页」「每天100个」里的数
fn per_day(c: &[char], unit: Option<char>) -> Option<u32> {
    ["每天", "一天", "每次"].iter().find_map(|k| {
        let s = find(c, k)? + k.chars().count();
        let j = (s..(s + 6).min(c.len())).find(|&j| c[j].is_ascii_digit() || is_cn_num(c[j]))?;
        let (v, e) = num_at(c, j)?;
        match unit {
            Some(u) => (c.get(e) == Some(&u)).then_some(v as u32),
            None => Some(v as u32),
        }
    })
}

/// 「共300页」「这本书 420 页」「第20-180页」+「每天20页」→ 按页分段
fn page_items(c: &[char]) -> Vec<(String, u32)> {
    let mut range: Option<(u32, u32)> = None;
    for i in find_all(c, "页") {
        let mut s = i;
        while s > 0 && is_num(c[s - 1]) {
            s -= 1;
        }
        if s == i {
            continue;
        }
        let Some((n, _)) = num_at(c, s) else { continue };
        let before: String = c[s.saturating_sub(3)..s].iter().collect();
        if before.contains("每天") || before.contains("一天") || before.contains("每次") {
            continue;
        }
        // 「20-180页」
        if s >= 2 && matches!(c[s - 1], '-' | '~' | '到' | '至' | '—') {
            let mut a = s - 1;
            while a > 0 && is_num(c[a - 1]) {
                a -= 1;
            }
            if let Some((x, _)) = num_at(c, a)
                && x < n
                && x >= 1.0
            {
                range = Some((x as u32, n as u32));
                break;
            }
        }
        if n >= 10.0 {
            range = Some((1, n as u32));
            break;
        }
    }
    let Some((a, b)) = range else {
        return Vec::new();
    };
    let per = per_day(c, Some('页')).filter(|p| *p >= 1).unwrap_or(20);
    let mut out = Vec::new();
    let mut p = a;
    while p <= b && out.len() < 300 {
        let q = (p + per - 1).min(b);
        out.push((format!("第{p}–{q}页"), 0));
        p = q + 1;
    }
    out
}

/// 「3000个单词，每天50个」→ 按组分段
fn count_items(c: &[char]) -> Vec<(String, u32)> {
    const NOUNS: &[&str] = &["单词", "生词", "词汇", "道题", "题", "句子", "视频"];
    for i in 0..c.len() {
        if i > 0 && is_num(c[i - 1]) {
            continue;
        }
        let Some((n, j)) = num_at(c, i) else { continue };
        let Some(noun) = NOUNS.iter().find(|w| starts(c, j, w)) else {
            continue;
        };
        let before: String = c[i.saturating_sub(2)..i].iter().collect();
        if before.contains('每') || before.contains("一天") || n < 20.0 {
            continue;
        }
        let total = n as u32;
        let per = per_day(c, None)
            .filter(|p| *p >= 1 && *p < total)
            .unwrap_or_else(|| (total / 30).max(1));
        let noun = noun.trim_start_matches('道');
        let mut out = Vec::new();
        let mut p = 1;
        while p <= total && out.len() < 300 {
            let q = (p + per - 1).min(total);
            out.push((format!("{noun} {p}–{q}"), 0));
            p = q + 1;
        }
        return out;
    }
    Vec::new()
}

/// 一行是不是列表项（「1.」「1、」「(1)」「-」「•」「第3章」「Unit 2」开头）：返回标题从哪里开始
fn list_marker(line: &[char]) -> Option<usize> {
    let mut i = 0;
    while line.get(i).is_some_and(|c| c.is_whitespace()) {
        i += 1;
    }
    let c0 = *line.get(i)?;
    if matches!(
        c0,
        '-' | '*' | '•' | '·' | '・' | '○' | '●' | '□' | '☐' | '✓' | '▪'
    ) {
        return Some(i + 1);
    }
    if matches!(c0, '(' | '（')
        && let Some((_, j)) = num_at(line, i + 1)
        && matches!(line.get(j), Some(')' | '）'))
    {
        return Some(j + 1);
    }
    if is_num(c0)
        && let Some((_, j)) = num_at(line, i)
        && matches!(line.get(j), Some('.' | '、' | ')' | '）' | ':' | '：'))
    {
        return Some(j + 1);
    }
    // 「第3章 xxx」「Unit 2 xxx」：标记本身留在标题里
    if c0 == '第'
        && let Some((_, j)) = num_at(line, i + 1)
        && UNITS.iter().any(|u| starts(line, j, u))
    {
        return Some(i);
    }
    if starts_any(line, i, EN_UNITS).is_some() {
        return Some(i);
    }
    None
}

fn clean(s: &str) -> String {
    s.trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                '：' | ':' | '-' | '—' | '，' | ',' | '。' | '.' | '；' | ';' | ' ' | '约'
            )
        })
        .chars()
        .take(100)
        .collect()
}

/// 一行里去掉时长（「(2小时)」「- 90min」「约1.5h」）后的标题和分钟
fn split_duration(line: &[char]) -> (String, u32) {
    let Some((m, s, e)) = first_duration(line) else {
        return (clean(&line.iter().collect::<String>()), 0);
    };
    let mut a = s;
    while a > 0
        && matches!(
            line[a - 1],
            '约' | '(' | '（' | '[' | '【' | ' ' | '-' | '—' | '：' | ':' | '，' | ','
        )
    {
        a -= 1;
    }
    if a >= 2 && starts(line, a - 2, "大约") || a >= 2 && starts(line, a - 2, "大概") {
        a -= 2;
    }
    let mut b = e;
    loop {
        if b < line.len() && matches!(line[b], ')' | '）' | ']' | '】' | ' ') {
            b += 1;
        } else if starts(line, b, "左右") {
            b += 2;
        } else {
            break;
        }
    }
    let mut t: String = line[..a].iter().collect();
    t.push(' ');
    t.push_str(&line[b..].iter().collect::<String>());
    (clean(&t), m)
}

const CONSTRAINT_WORDS: &[&str] = &[
    "每天",
    "每日",
    "周末",
    "工作日",
    "截止",
    "之前",
    "以前",
    "前完成",
    "每周",
    "一周",
    "提醒",
    "点开始",
    "考试",
];

/// 列表形式写的内容（每行一条）
pub fn list_items(text: &str) -> Vec<(String, u32)> {
    let lines: Vec<Vec<char>> = text
        .split(['\n', '；', ';'])
        .map(|l| l.trim().chars().collect::<Vec<char>>())
        .filter(|l| !l.is_empty())
        .collect();
    let marked: Vec<(String, u32)> = lines
        .iter()
        .filter_map(|l| list_marker(l).map(|i| split_duration(&l[i..])))
        .filter(|(t, _)| !t.is_empty())
        .collect();
    if marked.len() >= 2 {
        return marked.into_iter().take(300).collect();
    }
    // 没有编号、但第一行之后还有好几行短句：多半是一行一条
    if lines.len() >= 3 {
        let rest: Vec<(String, u32)> = lines[1..]
            .iter()
            .filter(|l| l.len() <= 40)
            .filter(|l| {
                let s: String = l.iter().collect();
                !CONSTRAINT_WORDS.iter().any(|w| s.contains(w))
            })
            .map(|l| split_duration(l))
            .filter(|(t, _)| !t.is_empty())
            .collect();
        if rest.len() >= 2 {
            return rest.into_iter().take(300).collect();
        }
    }
    Vec::new()
}

/// 从第一句话猜个标题
fn guess_title(text: &str) -> String {
    let first: String = text
        .trim()
        .chars()
        .take_while(|c| !SEP.contains(c) && !matches!(c, '（' | '('))
        .collect();
    let mut t = first.trim().to_string();
    for _ in 0..3 {
        for p in [
            "帮我做一个",
            "帮我做个",
            "帮我安排",
            "帮我",
            "我要",
            "我想要",
            "我想",
            "我打算",
            "我准备",
            "我计划",
            "打算",
            "准备",
            "想要",
        ] {
            if let Some(r) = t.strip_prefix(p) {
                t = r.trim().to_string();
            }
        }
        // 「在12月底前」「用两周时间」
        for (a, b) in [("在", "之前"), ("在", "前"), ("用", "时间"), ("花", "时间")] {
            if t.starts_with(a)
                && let Some(i) = t.find(b)
                && i < 40
            {
                t = t[i + b.len()..].trim().to_string();
            }
        }
    }
    for s in ["的计划", "计划"] {
        if let Some(r) = t.strip_suffix(s) {
            t = r.to_string();
        }
    }
    let t: String = t.trim().chars().take(24).collect();
    if t.is_empty() {
        "我的计划".into()
    } else {
        t
    }
}

/// 按关键词挑个图标
pub fn guess_emoji(text: &str) -> String {
    const MAP: &[(&[&str], &str)] = &[
        (
            &[
                "单词", "英语", "日语", "雅思", "托福", "四级", "六级", "口语", "听力",
            ],
            "🔤",
        ),
        (&["数学", "代数", "微积分", "高数", "概率", "几何"], "📐"),
        (
            &[
                "编程", "代码", "python", "java", "rust", "算法", "前端", "leetcode", "刷题",
            ],
            "💻",
        ),
        (
            &["健身", "跑步", "运动", "锻炼", "瑜伽", "减肥", "俯卧撑"],
            "🏃",
        ),
        (&["琴", "吉他", "唱歌", "音乐", "乐理"], "🎵"),
        (&["画", "素描", "插画", "绘"], "🎨"),
        (&["论文", "作文", "写作", "小说", "日记"], "✍️"),
        (&["读书", "看书", "阅读", "这本书", "读完"], "📖"),
        (&["项目", "工作", "上线", "汇报"], "💼"),
        (&["考试", "复习", "考研", "期末", "备考"], "📝"),
        (&["学", "课", "章", "讲"], "📚"),
    ];
    let low = text.to_lowercase();
    MAP.iter()
        .find(|(ks, _)| ks.iter().any(|k| low.contains(k)))
        .map(|(_, e)| (*e).to_string())
        .unwrap_or_else(|| "📌".into())
}

/// 总时长（「总共大概40小时」「一共要30个小时」）
fn total_minutes(c: &[char]) -> Option<u32> {
    keyword_duration(c, &["总共", "一共", "总计", "加起来", "全部"], 10)
}

const HABIT_WORDS: &[&str] = &[
    "打卡",
    "坚持",
    "养成",
    "习惯",
    "健身",
    "跑步",
    "锻炼",
    "练琴",
    "练字",
    "冥想",
    "早起",
    "运动",
    "每天背",
    "每天练",
    "每天读",
    "每天写",
    "每天做",
    "每天学",
];
const CONTENT_WORDS: &[&str] = &[
    "学完",
    "看完",
    "读完",
    "做完",
    "刷完",
    "复习完",
    "写完",
    "一共",
    "总共",
    "全书",
    "整本",
];

/// 理解用户写的计划
pub fn understand(text: &str, today: NaiveDate) -> Hints {
    let c: Vec<char> = text
        .chars()
        .map(|x| match x {
            '０'..='９' => char::from_u32(x as u32 - '０' as u32 + '0' as u32).unwrap_or(x),
            '：' => ':',
            _ => x,
        })
        .collect();
    let norm: String = c.iter().collect();
    // 分了节的只从「要做的内容」那几节里找条目（背景里列的经历不是要做的事）
    let mut items = list_items(&work_text(&norm));
    if items.is_empty() {
        items = range_items(&c);
    }
    if items.is_empty() {
        items = page_items(&c);
    }
    if items.is_empty() {
        items = count_items(&c);
    }
    let mode = if !items.is_empty() {
        Some(Mode::Content)
    } else if has(&c, HABIT_WORDS) && !has(&c, CONTENT_WORDS) {
        Some(Mode::Habit)
    } else {
        None
    };
    let remind_at = crate::reminders::parse::clock_in(&norm).map(|(h, m)| format!("{h:02}:{m:02}"));
    // 写明了时间范围的，截止就是范围的结束（不是正文里先碰到的「2月底前」）
    let range = date_range(&c, today);
    Hints {
        title: guess_title(&norm),
        emoji: guess_emoji(&norm),
        mode,
        week_minutes: week_minutes(&c),
        end_day: range.map(|r| r.1).or_else(|| end_day(&c, today)),
        start_day: range.map(|r| r.0),
        remind_at,
        items,
        total_minutes: total_minutes(&c),
        per_item: keyword_duration(
            &c,
            &[
                "每一章",
                "每章",
                "每节",
                "每讲",
                "每课",
                "每个单元",
                "每单元",
                "每集",
                "每篇",
                "每部分",
                "每一部分",
            ],
            8,
        ),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    fn titles(h: &Hints) -> Vec<&str> {
        h.items.iter().map(|(t, _)| t.as_str()).collect()
    }

    #[test]
    fn rough_one_liner() {
        let h = understand(
            "我要在12月底前学完线性代数第1-8章，每天1小时，周末可以学2个小时，晚上8点开始",
            today(),
        );
        assert_eq!(h.title, "学完线性代数第1-8章");
        assert_eq!(h.emoji, "📐");
        assert_eq!(h.mode, Some(Mode::Content));
        assert_eq!(h.items.len(), 8);
        assert_eq!(h.items[0].0, "第1章");
        assert_eq!(h.week_minutes, Some([60, 60, 60, 60, 60, 120, 120]));
        assert_eq!(h.end_day, NaiveDate::from_ymd_opt(2026, 12, 31));
        assert_eq!(h.remind_at.as_deref(), Some("20:00"));
    }

    #[test]
    fn detailed_list_with_durations() {
        let h = understand(
            "期末复习计划\n1. 第一章 极限（3小时）\n2. 第二章 导数 2h\n3、第三章 积分 约90分钟\n- 真题两套\n12月20日考试",
            today(),
        );
        assert_eq!(
            h.items,
            [
                ("第一章 极限".to_string(), 180),
                ("第二章 导数".to_string(), 120),
                ("第三章 积分".to_string(), 90),
                ("真题两套".to_string(), 0),
            ]
        );
        assert_eq!(h.end_day, NaiveDate::from_ymd_opt(2026, 12, 20));
        assert_eq!(h.title, "期末复习");
        assert_eq!(h.emoji, "📝");
    }

    #[test]
    fn plain_lines_without_numbers() {
        let h = understand("这周要做的\n整理房间\n写周报\n给妈妈打电话", today());
        assert_eq!(titles(&h), ["整理房间", "写周报", "给妈妈打电话"]);
    }

    #[test]
    fn ranges_pages_and_counts() {
        assert_eq!(understand("看完第一章到第五章", today()).items.len(), 5);
        assert_eq!(
            titles(&understand("Unit 3-5", today())),
            ["Unit 3", "Unit 4", "Unit 5"]
        );
        assert_eq!(understand("这门网课一共12讲", today()).items.len(), 12);
        let p = understand("这本书一共300页，每天30页", today());
        assert_eq!(p.items.len(), 10);
        assert_eq!(p.items[1].0, "第31–60页");
        let w = understand("背3000个单词，每天100个", today());
        assert_eq!(w.items.len(), 30);
        assert_eq!(w.items[0].0, "单词 1–100");
    }

    #[test]
    fn habits_and_week_patterns() {
        let h = understand("每天跑步30分钟，周末休息", today());
        assert_eq!(h.mode, Some(Mode::Habit));
        assert_eq!(h.week_minutes, Some([30, 30, 30, 30, 30, 0, 0]));
        assert!(h.items.is_empty());
        let h = understand("周一三五练琴一个半小时", today());
        assert_eq!(h.week_minutes, Some([90, 0, 90, 0, 90, 0, 0]));
        let h = understand("健身，每周3次，每次1小时", today());
        assert_eq!(h.week_minutes, Some([60, 0, 60, 0, 60, 0, 0]));
        let h = understand("工作日每天半小时", today());
        assert_eq!(h.week_minutes, Some([30, 30, 30, 30, 30, 0, 0]));
        let h = understand("每天1小时，工作日2小时", today());
        assert_eq!(h.week_minutes, Some([120, 120, 120, 120, 120, 60, 60]));
        assert_eq!(understand("随便写点什么", today()).week_minutes, None);
        assert_eq!(
            understand("第1-5章，每章2小时", today()).per_item,
            Some(120)
        );
    }

    #[test]
    fn relative_deadlines() {
        let t = today();
        assert_eq!(
            understand("两周内看完", t).end_day,
            Some(t + Duration::days(13))
        );
        assert_eq!(
            understand("一个月学完Python", t).end_day,
            Some(t + Duration::days(29))
        );
        assert_eq!(
            understand("月底前交", t).end_day,
            NaiveDate::from_ymd_opt(2026, 11, 30)
        );
        assert_eq!(
            understand("3月1号开学前", t).end_day,
            NaiveDate::from_ymd_opt(2027, 3, 1)
        );
        assert_eq!(understand("第3章", t).end_day, None);
    }

    /// 和本机样例同结构的虚构例子：分节、背景里有列表、约束里先出现「2月底前」、时间写成「1〜2小时」
    pub(crate) const LONG: &str = "请帮我做一份考证准备计划。\n【我的背景】\n- 某某大学 机械专业，明年7月毕业\n- 英语四级，做过两个课程设计\n【目标】\n- 毕业前拿到证书，顺便找到实习\n【约束】\n- 每天只能投入1〜2小时，周末可多一些\n- 实习作为备选：每周最多1小时用于联系公司，2027年2月底前没有回应就放弃\n【准备内容（按优先级）】\n1. 吃透专业课，能讲清楚三个核心概念（最高优先级）\n2. 刷历年真题：选择、计算、案例\n3. 做一个小作品集\n【输出要求】\n- 时间范围：2026年11月9日〜2027年6月30日\n- 按阶段划分，最后附一个每周日自查清单\n- 如有需要补充的信息，请先向我提问";

    #[test]
    fn long_sectioned_plan() {
        let h = understand(LONG, today());
        assert_eq!(
            h.end_day,
            NaiveDate::from_ymd_opt(2027, 6, 30),
            "时间范围优先"
        );
        assert_eq!(h.start_day, NaiveDate::from_ymd_opt(2026, 11, 9));
        assert_eq!(
            titles(&h),
            [
                "吃透专业课，能讲清楚三个核心概念（最高优先级）",
                "刷历年真题:选择、计算、案例",
                "做一个小作品集"
            ],
            "背景里列的不是要做的事"
        );
        assert_eq!(
            h.week_minutes,
            Some([90, 90, 90, 90, 90, 135, 135]),
            "1〜2小时取中间，周末多一些"
        );
        let s = sections(LONG);
        assert_eq!(s.len(), 6);
        assert!(is_work_head("准备内容（按优先级）") && !is_work_head("输出要求"));
        let c: Vec<char> = "11月9日到明年6月30日".chars().collect();
        assert_eq!(
            date_range(&c, today()),
            Some((
                NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
                NaiveDate::from_ymd_opt(2027, 6, 30).unwrap()
            ))
        );
        let c: Vec<char> = "30-45分钟".chars().collect();
        assert_eq!(duration_at(&c, 0).map(|x| x.0), Some(38));
    }

    #[test]
    fn numbers() {
        let p = |s: &str| parse_cn(&s.chars().collect::<Vec<_>>());
        assert_eq!(p("一千二百"), Some(1200));
        assert_eq!(p("三百五"), Some(350));
        assert_eq!(p("一百零五"), Some(105));
        assert_eq!(p("十五"), Some(15));
        let c: Vec<char> = "两个半小时".chars().collect();
        assert_eq!(duration_at(&c, 0), Some((150, 5)));
        let c: Vec<char> = "1.5h".chars().collect();
        assert_eq!(duration_at(&c, 0), Some((90, 4)));
        let c: Vec<char> = "3 hello".chars().collect();
        assert_eq!(duration_at(&c, 0), None);
    }
}
