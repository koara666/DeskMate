//! 日常关怀（v2 P3，DECISIONS #40）：久坐、喝水、深夜还没睡、到点该吃饭。
//! 每项单独开关和间隔，默认温和、低频；玩游戏、看视频、开会、全屏时由调度器推迟（种类 Care 可以等）。
//! 全部用台词池，不调用模型。

use crate::proactive::{self, Item, Kind};
use crate::state::AppState;
use chrono::{NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CareSettings {
    /// 坐太久了提醒站起来走走、看看远处
    pub sit: bool,
    pub sit_minutes: u32,
    pub water: bool,
    pub water_minutes: u32,
    /// 深夜还在电脑前
    pub late: bool,
    /// 过了这个时间算深夜（"HH:MM"，凌晨 5 点前有效）
    pub late_time: String,
    /// 到饭点了
    pub meal: bool,
}

impl Default for CareSettings {
    fn default() -> Self {
        Self {
            sit: true,
            sit_minutes: 60,
            water: true,
            water_minutes: 150,
            late: true,
            late_time: "00:30".into(),
            meal: true,
        }
    }
}

impl CareSettings {
    pub fn sanitize(mut self) -> Self {
        self.sit_minutes = self.sit_minutes.clamp(30, 240);
        self.water_minutes = self.water_minutes.clamp(60, 360);
        match proactive::parse_hm(&self.late_time) {
            // 只认 21:00 以后到凌晨 4:59
            Some(m) if !(5 * 60..21 * 60).contains(&m) => {}
            _ => self.late_time = "00:30".into(),
        }
        self
    }
}

/// 运行时状态（纯数据，`due` 是纯函数）
#[derive(Debug, Default, Clone)]
pub struct CareState {
    /// 这一段连续在电脑前从什么时候开始（0 = 不在）
    pub active_since: i64,
    pub last_sit: i64,
    pub last_water: i64,
    /// 已经说过深夜的那一天（按早上 5 点分天）
    pub late_day: Option<NaiveDate>,
    /// 今天已经提醒过的饭点（午饭 = 1，晚饭 = 2）
    pub meals: (Option<NaiveDate>, u8),
}

/// 该说哪一句关怀（每次最多一句）：（场景，占位符）
pub fn due(
    s: &CareSettings,
    st: &mut CareState,
    now: i64,
    local: NaiveDateTime,
    idle_secs: u64,
) -> Option<(&'static str, Vec<(&'static str, String)>)> {
    // 离开电脑 5 分钟以上 = 休息过了
    if idle_secs >= 300 {
        st.active_since = 0;
        st.last_sit = now;
        return None;
    }
    if st.active_since == 0 {
        st.active_since = now;
        if st.last_water == 0 {
            st.last_water = now;
        }
        if st.last_sit == 0 {
            st.last_sit = now;
        }
    }
    let day = crate::briefing::briefing_day(local);
    let minute = local.hour() * 60 + local.minute();
    if s.late
        && st.late_day != Some(day)
        && let Some(t) = proactive::parse_hm(&s.late_time)
    {
        let late = if t >= 12 * 60 {
            minute >= t || minute < 5 * 60
        } else {
            (t..5 * 60).contains(&minute)
        };
        // 刚坐下就说「这么晚了」不太好：至少在电脑前 10 分钟
        if late && now - st.active_since >= 600 {
            st.late_day = Some(day);
            return Some(("care.late_night", vec![]));
        }
    }
    if s.meal {
        if st.meals.0 != Some(local.date()) {
            st.meals = (Some(local.date()), 0);
        }
        let slot = match minute {
            m if (11 * 60 + 50..13 * 60).contains(&m) => Some((1u8, "午饭")),
            m if (18 * 60..19 * 60 + 30).contains(&m) => Some((2u8, "晚饭")),
            _ => None,
        };
        if let Some((bit, name)) = slot
            && st.meals.1 & bit == 0
        {
            st.meals.1 |= bit;
            return Some(("care.meal", vec![("饭", name.to_string())]));
        }
    }
    if s.sit
        && now - st.active_since >= s.sit_minutes as i64 * 60
        && now - st.last_sit >= s.sit_minutes as i64 * 60
    {
        st.last_sit = now;
        return Some(("care.stand_up", vec![]));
    }
    if s.water && now - st.last_water >= s.water_minutes as i64 * 60 {
        st.last_water = now;
        return Some(("care.water", vec![]));
    }
    None
}

#[derive(Default)]
pub struct CareRuntime {
    state: Mutex<CareState>,
}

/// 日常循环每 30 秒调用一次
pub fn check(app: &AppHandle) {
    if crate::paths::test_quiet() {
        return;
    }
    let st = app.state::<AppState>();
    let s = st.settings().care;
    let now = crate::memory::now_secs();
    let local = crate::reminders::now_local();
    let idle = crate::daily::system_idle_secs();
    let hit = {
        let mut g = st.care.state.lock().unwrap_or_else(|e| e.into_inner());
        due(&s, &mut g, now, local, idle)
    };
    let Some((scene, vars)) = hit else { return };
    let Some(text) = st.line(scene, &vars) else {
        return;
    };
    let mut item = Item::new(Kind::Care, text, now + 30 * 60);
    item.key = format!("care:{scene}");
    proactive::submit(app, item);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 28)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    fn quiet() -> CareSettings {
        CareSettings {
            sit: false,
            water: false,
            late: false,
            meal: false,
            ..Default::default()
        }
    }

    #[test]
    fn sit_resets_after_break() {
        let s = CareSettings {
            sit: true,
            ..quiet()
        };
        let mut st = CareState::default();
        let t0 = 1_000_000;
        assert!(due(&s, &mut st, t0, at(15, 0), 0).is_none());
        assert!(due(&s, &mut st, t0 + 59 * 60, at(15, 59), 0).is_none());
        assert_eq!(
            due(&s, &mut st, t0 + 60 * 60, at(16, 0), 0).unwrap().0,
            "care.stand_up"
        );
        assert!(
            due(&s, &mut st, t0 + 61 * 60, at(16, 1), 0).is_none(),
            "刚说过"
        );
        // 离开 5 分钟算休息过了，重新计时
        assert!(due(&s, &mut st, t0 + 70 * 60, at(16, 10), 400).is_none());
        assert!(due(&s, &mut st, t0 + 71 * 60, at(16, 11), 0).is_none());
        assert!(due(&s, &mut st, t0 + 125 * 60, at(17, 5), 0).is_none());
        assert_eq!(
            due(&s, &mut st, t0 + 131 * 60, at(17, 11), 0).unwrap().0,
            "care.stand_up"
        );
    }

    #[test]
    fn meals_once_each_and_late_night_once() {
        let s = CareSettings {
            meal: true,
            late: true,
            ..quiet()
        };
        let mut st = CareState::default();
        let t = 2_000_000;
        assert!(due(&s, &mut st, t, at(11, 0), 0).is_none());
        let (scene, vars) = due(&s, &mut st, t + 60, at(12, 0), 0).unwrap();
        assert_eq!((scene, vars[0].1.as_str()), ("care.meal", "午饭"));
        assert!(
            due(&s, &mut st, t + 120, at(12, 30), 0).is_none(),
            "午饭只说一次"
        );
        assert_eq!(
            due(&s, &mut st, t + 180, at(18, 30), 0).unwrap().1[0].1,
            "晚饭"
        );
        // 深夜：在电脑前 10 分钟以上才说，一晚只说一次
        let mut st = CareState::default();
        assert!(due(&s, &mut st, t, at(0, 40), 0).is_none());
        assert_eq!(
            due(&s, &mut st, t + 700, at(0, 52), 0).unwrap().0,
            "care.late_night"
        );
        assert!(due(&s, &mut st, t + 3000, at(1, 30), 0).is_none());
    }

    #[test]
    fn sanitize() {
        let s = CareSettings {
            sit_minutes: 5,
            late_time: "15:00".into(),
            ..Default::default()
        }
        .sanitize();
        assert_eq!(s.sit_minutes, 30);
        assert_eq!(s.late_time, "00:30");
        assert_eq!(
            CareSettings {
                late_time: "23:30".into(),
                ..Default::default()
            }
            .sanitize()
            .late_time,
            "23:30"
        );
    }
}
