//! 活动感知（v2 P3，DECISIONS #40）。
//!
//! - 第一层（默认开，零成本）：前台窗口切换的系统事件 + 每 30 秒看一眼 → 本地规则分类（`classify`）。
//! - 第二层（默认开）：状态变化时决定要不要说一句（玩了很久、深夜还在玩、刚从写作业切到视频……），
//!   经过主动行为调度器；台词池为主，偶尔（有预算时）让模型说，只告诉模型「分类 + 时长 + 清洗过的程序名」。
//! - 用户规则：按分类或程序设「可以评论 / 保持安静 / 回避（角色暂时藏起来）」；隐私名单默认回避，前台时完全不记录、不评论。
//! - 活动日志只存汇总（`log`）。

pub mod classify;
pub mod log;
pub mod tracker;
pub mod win;

use crate::memory;
use crate::proactive::{self, Item, Kind};
use crate::state::AppState;
use chrono::Timelike;
use classify::{Category, Sample};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracker::{Event, Tracker};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 可以评论
    Comment,
    /// 保持安静（照样记录）
    Quiet,
    /// 回避：角色暂时藏起来，不评论
    Avoid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// 分类 id（game、coding……）或者进程名（xxx.exe）
    pub target: String,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivitySettings {
    /// 第一层：看前台是什么程序（只在本地分类）
    pub enabled: bool,
    /// 第二层：状态变化时偶尔说一句
    pub comments: bool,
    /// 记录活动日志（只存汇总）
    pub record: bool,
    pub rules: Vec<Rule>,
    /// 额外的隐私名单（进程名，小写）
    pub private_apps: Vec<String>,
}

impl Default for ActivitySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            comments: true,
            record: true,
            rules: Vec::new(),
            private_apps: Vec::new(),
        }
    }
}

/// 没设规则时各分类的默认做法
pub fn default_action(c: Category) -> Action {
    match c {
        Category::Game | Category::Coding | Category::Writing | Category::Video => Action::Comment,
        _ => Action::Quiet,
    }
}

fn valid_target(t: &str) -> bool {
    Category::from_id(t).is_some()
        || (t.ends_with(".exe")
            && t.len() <= 80
            && !t.contains(['\\', '/', ':'])
            && t.chars().all(|c| !c.is_control()))
}

impl ActivitySettings {
    pub fn sanitize(mut self) -> Self {
        for r in &mut self.rules {
            r.target = r.target.trim().to_lowercase();
        }
        let mut seen = std::collections::HashSet::new();
        self.rules
            .retain(|r| valid_target(&r.target) && seen.insert(r.target.clone()));
        self.rules.truncate(100);
        self.private_apps = self
            .private_apps
            .iter()
            .map(|p| p.trim().to_lowercase())
            .filter(|p| p.ends_with(".exe") && valid_target(p))
            .collect();
        self.private_apps.dedup();
        self.private_apps.truncate(50);
        self
    }

    /// 某个采样该怎么对待：隐私 → 回避；程序规则 > 分类规则 > 默认
    pub fn action_for(&self, s: &Sample) -> Action {
        if s.private {
            return Action::Avoid;
        }
        if let Some(r) = self.rules.iter().find(|r| r.target == s.exe) {
            return r.action;
        }
        if let Some(r) = self.rules.iter().find(|r| r.target == s.category.id()) {
            return r.action;
        }
        default_action(s.category)
    }
}

/// 现在在做什么（给调度器、聊天提示词、设置页用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Current {
    pub category: Category,
    pub label: Option<String>,
    pub since: i64,
    #[serde(skip)]
    pub private: bool,
}

#[derive(Default)]
pub struct ActivityState {
    tracker: Mutex<Tracker>,
    current: Mutex<Option<Current>>,
    /// 因为「回避」而藏起来了
    avoiding: AtomicBool,
    /// 最近用过的程序（进程名、分类），设置页加规则用；只在内存里
    recent: Mutex<VecDeque<(String, Category)>>,
}

impl ActivityState {
    pub fn current(&self) -> Option<Current> {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn recent(&self) -> Vec<(String, Category)> {
        self.recent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

/// 聊天时给模型的一句话：「用户正在写代码（VS Code），大约 40 分钟了」
pub fn context_line(st: &AppState) -> Option<String> {
    let c = st.activity.current()?;
    if c.private || matches!(c.category, Category::Other | Category::Idle) {
        return None;
    }
    let mins = (memory::now_secs() - c.since) / 60;
    let what = match &c.label {
        Some(l) => format!("{}（{l}）", c.category.name()),
        None => c.category.name().to_string(),
    };
    Some(if mins >= 5 {
        format!("用户正在{what}，大约 {} 了", log::duration_text(mins * 60))
    } else {
        format!("用户刚开始{what}")
    })
}

/// 调度器要用的：前台是隐私窗口？在玩游戏 / 看视频 / 开会（关怀推迟）？
pub fn gate_flags(st: &AppState) -> (bool, bool) {
    match st.activity.current() {
        Some(c) => (
            c.private,
            matches!(
                c.category,
                Category::Game | Category::Video | Category::Meeting
            ),
        ),
        None => (false, false),
    }
}

// ---------------- 运行时 ----------------

pub fn spawn(app: AppHandle) {
    let mut rx = win::install(&app);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            tokio::select! {
                r = rx.recv() => {
                    if r.is_none() {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                    // 切窗口常常一下子来好几个事件：稍等一下，只处理一次
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    while rx.try_recv().is_ok() {}
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
            sample(&app);
        }
    });
}

fn set_avoid(app: &AppHandle, avoid: bool) {
    let st = app.state::<AppState>();
    if st.activity.avoiding.swap(avoid, Ordering::Relaxed) == avoid {
        return;
    }
    if avoid {
        let _ = crate::desktop::hide_chat(app);
        if let Some(p) = crate::desktop::pet(app) {
            let _ = crate::desktop::hide_window(&p);
        }
        ::log::info!("前台是需要回避的程序，桌宠暂时藏起来");
    } else if let Some(p) = crate::desktop::pet(app) {
        crate::daily::show_no_activate(&p);
    }
}

/// 看一眼现在前台是什么，更新状态，处理事件
pub fn sample(app: &AppHandle) {
    let st = app.state::<AppState>();
    let s = st.settings().activity;
    if !s.enabled || crate::paths::test_quiet() {
        *st.activity
            .current
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        set_avoid(app, false);
        return;
    }
    let now = memory::now_secs();
    let sample = if crate::daily::system_idle_secs() >= 300 {
        Sample {
            category: Category::Idle,
            label: None,
            exe: String::new(),
            private: false,
        }
    } else {
        let Some(fg) = win::foreground() else { return };
        // 自己的窗口（用户在和角色聊天）：保持原来的判断
        if fg.pid == std::process::id() {
            return;
        }
        classify::classify(&fg.exe, &fg.path, &fg.title, &s.private_apps)
    };
    let action = s.action_for(&sample);
    set_avoid(app, action == Action::Avoid);
    if !sample.private && !sample.exe.is_empty() {
        let mut r = st.activity.recent.lock().unwrap_or_else(|e| e.into_inner());
        r.retain(|(e, _)| *e != sample.exe);
        r.push_front((sample.exe.clone(), sample.category));
        r.truncate(20);
    }
    let local = crate::reminders::now_local();
    let minute = local.hour() * 60 + local.minute();
    let (events, cur) = {
        let mut t = st
            .activity
            .tracker
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let ev = t.observe(&sample, now, minute);
        // 记活动日志：一段做满 2 分钟后开始记，之后每次采样更新时长
        if s.record
            && let Some(c) = t.cur.as_mut()
            && c.category != Category::Idle
            && now - c.start >= 120
        {
            let day =
                crate::briefing::briefing_day(crate::reminders::to_local(c.start)).to_string();
            match log::upsert(
                &st.db(),
                c.row,
                &day,
                c.category.id(),
                c.label.as_deref().unwrap_or(""),
                c.start,
                now - c.start,
            ) {
                Ok(id) => c.row = Some(id),
                Err(e) => ::log::warn!("记录活动失败：{e}"),
            }
        }
        (ev, t.current().cloned())
    };
    *st.activity
        .current
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = match (&cur, sample.private) {
        (_, true) => Some(Current {
            category: Category::Other,
            label: None,
            since: now,
            private: true,
        }),
        (Some(c), false) => Some(Current {
            category: c.category,
            label: c.label.clone(),
            since: c.start,
            private: false,
        }),
        (None, false) => None,
    };
    for e in events {
        handle_event(app, &s, &e, action, now);
    }
}

fn comment_allowed(s: &ActivitySettings, category: Category) -> bool {
    s.comments
        && s.rules
            .iter()
            .find(|r| r.target == category.id())
            .map(|r| r.action)
            .unwrap_or_else(|| default_action(category))
            == Action::Comment
}

fn handle_event(app: &AppHandle, s: &ActivitySettings, e: &Event, now_action: Action, now: i64) {
    match e {
        Event::Start { category, .. } => {
            // 「下次开始玩游戏时提醒我……」：不管评论规则，用户自己设的提醒照样触发
            if *category != Category::Idle {
                crate::reminders::on_activity(app, category.id());
            }
        }
        Event::End {
            category: Category::Game,
            label,
            secs,
        } if *secs >= 3600 && comment_allowed(s, Category::Game) => {
            let mut vars = vec![("时长", log::duration_text(*secs))];
            if let Some(l) = label {
                vars.push(("程序", l.clone()));
            }
            say(
                app,
                "activity.game_end",
                &vars,
                now,
                Some(format!(
                    "用户刚玩完一段游戏{}，玩了 {}。用一句话自然地说点什么（可以夸、可以关心他休息一下），不要说教。",
                    label
                        .as_deref()
                        .map(|l| format!("（{l}）"))
                        .unwrap_or_default(),
                    log::duration_text(*secs)
                )),
            );
        }
        Event::Long {
            category,
            label,
            secs,
        } if now_action == Action::Comment && comment_allowed(s, *category) => {
            let scene = match category {
                Category::Game => "activity.long_game",
                Category::Coding => "activity.long_coding",
                Category::Video => "activity.long_video",
                Category::Writing => "activity.long_writing",
                _ => return,
            };
            let mut vars = vec![("时长", log::duration_text(*secs))];
            if let Some(l) = label {
                vars.push(("程序", l.clone()));
            }
            say(
                app,
                scene,
                &vars,
                now,
                Some(format!(
                    "用户已经连续{}{}了。用一句话自然地说点什么（可以关心一下、提议休息），不要说教、不要命令。",
                    category.name(),
                    log::duration_text(*secs)
                )),
            );
        }
        Event::LateNight { category } if comment_allowed(s, *category) => {
            say(app, "activity.late_night", &[], now, None);
        }
        Event::Switch {
            from: Category::Writing,
            from_secs,
            to: Category::Video,
        } if *from_secs >= 20 * 60 && comment_allowed(s, Category::Video) => {
            say(app, "activity.writing_to_video", &[], now, None);
        }
        _ => {}
    }
}

/// 说一句活动评论：通常用台词池；有预算时四分之一的机会让模型说（只给分类、时长、程序名）
fn say(
    app: &AppHandle,
    scene: &str,
    vars: &[(&str, String)],
    now: i64,
    model_hint: Option<String>,
) {
    let st = app.state::<AppState>();
    let pool = st.line(scene, vars);
    let app2 = app.clone();
    let scene = scene.to_string();
    tauri::async_runtime::spawn(async move {
        let st = app2.state::<AppState>();
        let mut text = pool;
        if let Some(hint) = model_hint
            && crate::lines::seed() % 4 == 0
            && let Some(t) = proactive::model_say(&st, &hint, 100).await
        {
            text = Some(t);
        }
        let Some(text) = text else { return };
        let mut item = Item::new(Kind::Activity, text, now + 120);
        item.key = scene;
        proactive::submit(&app2, item);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(c: Category, exe: &str) -> Sample {
        Sample {
            category: c,
            label: None,
            exe: exe.into(),
            private: false,
        }
    }

    #[test]
    fn rules_resolve_in_order() {
        let mut s = ActivitySettings::default();
        assert_eq!(
            s.action_for(&sample(Category::Game, "x.exe")),
            Action::Comment
        );
        assert_eq!(
            s.action_for(&sample(Category::Chat, "wechat.exe")),
            Action::Quiet
        );
        s.rules = vec![
            Rule {
                target: "game".into(),
                action: Action::Quiet,
            },
            Rule {
                target: "hades.exe".into(),
                action: Action::Avoid,
            },
        ];
        assert_eq!(
            s.action_for(&sample(Category::Game, "x.exe")),
            Action::Quiet
        );
        assert_eq!(
            s.action_for(&sample(Category::Game, "hades.exe")),
            Action::Avoid
        );
        let mut p = sample(Category::Coding, "code.exe");
        p.private = true;
        assert_eq!(s.action_for(&p), Action::Avoid, "隐私名单永远回避");
    }

    #[test]
    fn sanitize_rules() {
        let s = ActivitySettings {
            rules: vec![
                Rule {
                    target: " GAME ".into(),
                    action: Action::Quiet,
                },
                Rule {
                    target: "game".into(),
                    action: Action::Avoid,
                },
                Rule {
                    target: "C:\\evil.exe".into(),
                    action: Action::Avoid,
                },
                Rule {
                    target: "Hades.exe".into(),
                    action: Action::Avoid,
                },
                Rule {
                    target: "nonsense".into(),
                    action: Action::Avoid,
                },
            ],
            private_apps: vec!["My Bank.EXE".into(), "notes.txt".into()],
            ..Default::default()
        }
        .sanitize();
        assert_eq!(s.rules.len(), 2);
        assert_eq!(s.rules[0].action, Action::Quiet, "重复的只留第一个");
        assert_eq!(s.rules[1].target, "hades.exe");
        assert_eq!(s.private_apps, vec!["my bank.exe"]);
        let d = ActivitySettings::default();
        assert!(d.enabled && d.comments && d.record);
    }

    #[test]
    fn context_line_never_mentions_private_or_titles() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        assert!(context_line(&st).is_none());
        *st.activity.current.lock().unwrap() = Some(Current {
            category: Category::Coding,
            label: Some("VS Code".into()),
            since: memory::now_secs() - 40 * 60,
            private: false,
        });
        assert_eq!(
            context_line(&st).as_deref(),
            Some("用户正在写代码（VS Code），大约 40 分钟 了")
        );
        assert_eq!(gate_flags(&st), (false, false));
        *st.activity.current.lock().unwrap() = Some(Current {
            category: Category::Other,
            label: None,
            since: 0,
            private: true,
        });
        assert!(context_line(&st).is_none());
        assert_eq!(gate_flags(&st), (true, false));
    }
}
