//! 主动行为调度器（DECISIONS #35）：提醒、关怀、播报、活动评论、随机小事件、补提醒都只能通过这里说话。
//!
//! - `decide` 是纯函数：给定种类、设置、当前状况和节奏，决定「现在说 / 发系统通知 / 等一会 / 不说了」。
//!   时间一律由调用方传入，测试用假时钟。
//! - 运行时：各功能 `submit` 一条待说的话（带过期时间），日常循环每 3 秒 `tick` 一次，
//!   按优先级取出能说的那条，弹出聊天气泡（不抢焦点）。队列空时 `tick` 只是加一次锁。
//! - 用户连续忽略主动发言（前端气泡无人理会自动收起后上报）→ 冷却时间翻倍，降低频率；
//!   用户和角色互动后恢复。

use crate::chat::{self, PetState};
use crate::memory::{self, conversation};
use crate::state::AppState;
use crate::usage::{self, Budget};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// 主动说话的种类，按优先级从高到低排列
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// 用户设定的提醒（含补提醒）
    Reminder,
    /// 日常关怀
    Care,
    /// 每日播报
    Briefing,
    /// 活动评论
    Activity,
    /// 随机小事件、很久没聊天时打招呼
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Rare,
    #[default]
    Sometimes,
    Often,
}

/// 「陪伴」设置：所有主动行为的总控
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanionSettings {
    /// 主动说话总开关（关掉后只剩用户设定的提醒）
    pub enabled: bool,
    /// 两次主动发言之间至少隔多久（分钟）
    pub cooldown_minutes: u32,
    /// 活动评论、随机小事件的频率
    pub frequency: Frequency,
    /// 安静时段（这段时间里除了提醒都不说话）
    pub quiet_enabled: bool,
    /// "HH:MM"
    pub quiet_start: String,
    pub quiet_end: String,
    /// 主动行为每天最多调用几次模型（用完就只用预写台词）
    pub daily_calls: u32,
    /// 主动行为每天最多花多少钱（美元）
    pub daily_usd: f64,
    /// 随机小事件（分享、提问、给选项）
    pub events: bool,
    /// 全屏时到了的提醒、重要提醒：同时发系统通知
    pub system_notify: bool,
}

impl Default for CompanionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            cooldown_minutes: 20,
            frequency: Frequency::Sometimes,
            quiet_enabled: false,
            quiet_start: "23:30".into(),
            quiet_end: "08:00".into(),
            daily_calls: 10,
            daily_usd: 0.05,
            events: true,
            system_notify: true,
        }
    }
}

/// "HH:MM" → 一天中的第几分钟
pub fn parse_hm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

impl CompanionSettings {
    pub fn sanitize(mut self) -> Self {
        self.cooldown_minutes = self.cooldown_minutes.clamp(5, 240);
        self.daily_calls = self.daily_calls.min(100);
        if !self.daily_usd.is_finite() || self.daily_usd < 0.0 {
            self.daily_usd = 0.0;
        }
        self.daily_usd = self.daily_usd.min(10.0);
        if parse_hm(&self.quiet_start).is_none() {
            self.quiet_start = "23:30".into();
        }
        if parse_hm(&self.quiet_end).is_none() {
            self.quiet_end = "08:00".into();
        }
        self
    }

    /// 现在（一天中的第几分钟）是否在安静时段里（支持跨午夜）
    pub fn in_quiet(&self, minute_of_day: u32) -> bool {
        if !self.quiet_enabled {
            return false;
        }
        let (Some(a), Some(b)) = (parse_hm(&self.quiet_start), parse_hm(&self.quiet_end)) else {
            return false;
        };
        if a == b {
            return false;
        }
        if a < b {
            (a..b).contains(&minute_of_day)
        } else {
            minute_of_day >= a || minute_of_day < b
        }
    }

    /// 活动评论 / 随机小事件每天各最多几次
    pub fn daily_cap(&self, kind: Kind) -> u32 {
        match kind {
            Kind::Activity | Kind::Random => match self.frequency {
                Frequency::Rare => 2,
                Frequency::Sometimes => 5,
                Frequency::Often => 10,
            },
            _ => u32::MAX,
        }
    }

    /// 实际冷却（秒）：频率和「连续被忽略」都会拉长它
    pub fn cooldown_secs(&self, kind: Kind, ignored_streak: u32) -> i64 {
        let base = self.cooldown_minutes as i64 * 60;
        let freq = match (kind, self.frequency) {
            (Kind::Activity | Kind::Random, Frequency::Rare) => 2.0,
            (Kind::Activity | Kind::Random, Frequency::Often) => 0.5,
            _ => 1.0,
        };
        // 连续忽略 2 次起：×2、×4、×8 封顶
        let backoff = if ignored_streak >= 2 {
            1i64 << (ignored_streak - 1).min(3)
        } else {
            1
        };
        (base as f64 * freq) as i64 * backoff
    }
}

/// 此刻的状况（调用方从系统和设置里收集）
#[derive(Debug, Clone, Copy, Default)]
pub struct Gate {
    pub dnd: bool,
    pub fullscreen: bool,
    /// 前台是隐私名单里的程序（密码管理器、网银、隐身窗口……）
    pub private_foreground: bool,
    /// 正在生成回复
    pub generating: bool,
    /// 用户离开电脑了（几分钟没有键盘鼠标操作）
    pub user_away: bool,
    /// 在玩游戏 / 看视频 / 开会：关怀和播报等一等
    pub deep_focus: bool,
    /// 本地时间：一天中的第几分钟
    pub minute_of_day: u32,
}

/// 节奏：上次主动说话的时间、连续被忽略的次数、今天这种已经说了几次
#[derive(Debug, Clone, Copy, Default)]
pub struct Pace {
    pub last_spoke: i64,
    pub ignored_streak: u32,
    pub said_today: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// 现在说
    Speak,
    /// 改发系统通知（全屏时到了的提醒）
    Notify,
    /// 等一会再看（过期就丢掉）
    Wait,
    /// 不说了
    Drop,
}

/// 现在能不能说（纯函数）
pub fn decide(kind: Kind, s: &CompanionSettings, g: &Gate, p: &Pace, now: i64) -> Decision {
    if kind == Kind::Reminder {
        // 用户自己设的提醒：勿扰、安静时段、隐私名单都挡不住；全屏时改发系统通知或等退出全屏
        if g.fullscreen {
            return if s.system_notify {
                Decision::Notify
            } else {
                Decision::Wait
            };
        }
        if g.generating {
            return Decision::Wait;
        }
        return Decision::Speak;
    }
    if !s.enabled || g.dnd || g.private_foreground || s.in_quiet(g.minute_of_day) {
        return Decision::Drop;
    }
    if p.said_today >= s.daily_cap(kind) {
        return Decision::Drop;
    }
    // 关怀和播报可以等（等用户退出全屏、回到电脑前）；活动评论和小事件过了那个时机就没意义了
    let can_wait = matches!(kind, Kind::Care | Kind::Briefing);
    if g.fullscreen || g.user_away || (g.deep_focus && can_wait) {
        return if can_wait {
            Decision::Wait
        } else {
            Decision::Drop
        };
    }
    if g.generating {
        return Decision::Wait;
    }
    if p.last_spoke > 0 && now - p.last_spoke < s.cooldown_secs(kind, p.ignored_streak) {
        return if can_wait {
            Decision::Wait
        } else {
            Decision::Drop
        };
    }
    Decision::Speak
}

/// 主动行为能不能调用模型（每日次数、花费上限；全局预算用到八成就不再给主动行为用，留给聊天）
pub fn model_allowed(
    db: &Connection,
    s: &CompanionSettings,
    global: &Budget,
    day: &str,
    month: &str,
) -> bool {
    let (calls, cost) = usage::purpose_day(db, day, usage::purpose::PROACTIVE);
    if calls >= s.daily_calls as i64 || cost >= s.daily_usd {
        return false;
    }
    if global.daily_usd > 0.0 && usage::spent_day(db, day) >= global.daily_usd * 0.8 {
        return false;
    }
    if global.monthly_usd > 0.0 && usage::spent_month(db, month) >= global.monthly_usd * 0.8 {
        return false;
    }
    true
}

// ---------------- 运行时 ----------------

/// 一条待说的话
#[derive(Debug, Clone)]
pub struct Item {
    pub kind: Kind,
    pub text: String,
    /// 过了这个时间还没说出去就丢掉（Unix 秒）
    pub expires_at: i64,
    /// 记进聊天记录（之后聊天能接上）；触摸反应这类小事不记
    pub store: bool,
    /// 说的时候的表情
    pub mood: PetState,
    /// 同一来源的去重键（比如提醒 id）：队列里已有同键的就不再加
    pub key: String,
    /// 小事件：给用户的选项和对应的回应
    pub choices: Option<crate::companion::Choice>,
    /// 只是一句小反应（摸头等），不记进聊天记录
    pub ephemeral: bool,
    /// 气泡下面的一行小字（比如「看了「Hades」· 遮住 2 处」），不进聊天记录
    pub note: String,
    /// 有新版本：气泡里显示更新卡片
    pub update: Option<crate::update::UpdateView>,
}

impl Item {
    pub fn new(kind: Kind, text: impl Into<String>, expires_at: i64) -> Self {
        Self {
            kind,
            text: text.into(),
            expires_at,
            store: true,
            mood: PetState::Happy,
            key: String::new(),
            choices: None,
            ephemeral: false,
            note: String::new(),
            update: None,
        }
    }
}

/// 发给聊天窗口的主动消息
#[derive(Debug, Clone, Serialize)]
pub struct ProactiveMessage {
    pub id: u64,
    pub kind: Kind,
    pub text: String,
    /// 弹出了气泡（false = 全屏时改发了系统通知，只是记进聊天窗口）
    pub popped: bool,
    /// 小事件的选项（点了调用 `event_choose`）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<crate::update::UpdateView>,
}

#[derive(Default)]
struct Inner {
    queue: Vec<Item>,
    last_spoke: i64,
    ignored_streak: u32,
    /// （日期，每种已说次数）
    said: (String, HashMap<Kind, u32>),
    next_id: u64,
}

#[derive(Default)]
pub struct ProactiveState {
    inner: Mutex<Inner>,
}

impl ProactiveState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 排队一条主动发言
    pub fn submit(&self, item: Item) {
        let mut g = self.lock();
        if !item.key.is_empty() && g.queue.iter().any(|x| x.key == item.key) {
            return;
        }
        if g.queue.len() >= 20 {
            // 异常情况：只留优先级高的
            g.queue.sort_by_key(|x| x.kind);
            g.queue.truncate(10);
        }
        g.queue.push(item);
    }

    pub fn has_pending(&self) -> bool {
        !self.lock().queue.is_empty()
    }

    /// 用户和角色互动了：之前的「被忽略」清零
    pub fn engaged(&self) {
        self.lock().ignored_streak = 0;
    }

    /// 主动发言被无视（气泡无人理会自动收起）
    pub fn ignored(&self) -> u32 {
        let mut g = self.lock();
        g.ignored_streak = g.ignored_streak.saturating_add(1);
        g.ignored_streak
    }

    pub fn ignored_streak(&self) -> u32 {
        self.lock().ignored_streak
    }

    /// 取出现在该说的那条（最多一条）。`gate` 是此刻的状况，`day` 是今天的日期。
    pub fn take_due(
        &self,
        s: &CompanionSettings,
        gate: &Gate,
        now: i64,
        day: &str,
    ) -> Option<(Item, Decision, u64)> {
        let mut g = self.lock();
        if g.said.0 != day {
            g.said = (day.to_string(), HashMap::new());
        }
        g.queue.retain(|x| x.expires_at > now);
        g.queue.sort_by_key(|x| x.kind);
        let mut i = 0;
        while i < g.queue.len() {
            let kind = g.queue[i].kind;
            let pace = Pace {
                last_spoke: g.last_spoke,
                ignored_streak: g.ignored_streak,
                said_today: g.said.1.get(&kind).copied().unwrap_or(0),
            };
            match decide(kind, s, gate, &pace, now) {
                Decision::Wait => i += 1,
                Decision::Drop => {
                    let it = g.queue.remove(i);
                    log::info!("主动发言（{:?}）没有说出去：当前不适合打扰", it.kind);
                }
                d @ (Decision::Speak | Decision::Notify) => {
                    let it = g.queue.remove(i);
                    if d == Decision::Speak {
                        g.last_spoke = now;
                    }
                    *g.said.1.entry(kind).or_default() += 1;
                    g.next_id += 1;
                    let id = g.next_id;
                    return Some((it, d, id));
                }
            }
        }
        None
    }
}

/// 收集此刻的状况
pub fn current_gate(app: &AppHandle) -> Gate {
    use chrono::Timelike;
    let st = app.state::<AppState>();
    let now = chrono::Local::now();
    let (private_foreground, deep_focus) = crate::activity::gate_flags(&st);
    Gate {
        dnd: st.settings().dnd,
        fullscreen: st.daily.is_fullscreen(),
        private_foreground,
        generating: st.chat.is_generating(),
        user_away: crate::daily::system_idle_secs() >= 300,
        deep_focus,
        minute_of_day: now.hour() * 60 + now.minute(),
    }
}

/// 排队一条主动发言（各功能调用）
pub fn submit(app: &AppHandle, item: Item) {
    app.state::<AppState>().proactive.submit(item);
}

/// 日常循环每 3 秒调用一次：有能说的就说出来
pub fn tick(app: &AppHandle) {
    let st = app.state::<AppState>();
    if !st.proactive.has_pending() {
        return;
    }
    let s = st.settings().companion;
    let gate = current_gate(app);
    let (day, _) = usage::today();
    if let Some((item, decision, id)) = st.proactive.take_due(&s, &gate, memory::now_secs(), &day) {
        deliver(app, item, decision, id);
    }
}

/// 让模型用角色口吻说一句（活动评论、小事件等偶尔用）：只在主动行为预算够时调用，用途记为 proactive。
/// `instruction` 里只放分类、时长这类概括信息，不放窗口标题等隐私。失败或不可用返回 None（调用方用台词池）。
pub async fn model_say(st: &AppState, instruction: &str, max_tokens: u32) -> Option<String> {
    let s = st.settings();
    if s.chat_backend == "mock" {
        return None;
    }
    {
        let (day, month) = usage::today();
        if !model_allowed(&st.db(), &s.companion, &s.budget, &day, &month) {
            return None;
        }
    }
    let c = st.current_character()?;
    let prefs = s.effective_prefs(&c.manifest.id);
    let note = format!(
        "【主动说一句】{instruction} 只说一两句，用你的口吻，不要提「程序」「提示」之类的词。"
    );
    let now_text = crate::chat::now_text();
    let parts = crate::prompt::PromptParts {
        character_name: &c.manifest.name,
        persona: &c.persona,
        user_rules: &prefs.custom_rules,
        banned_words: &prefs.banned_words,
        quotes_fixed: &c.quotes.fixed,
        notes: std::slice::from_ref(&note),
        user_text: "（……）",
        now: &now_text,
        ..Default::default()
    };
    let req = crate::prompt::build(
        &parts,
        crate::prompt::PromptBudget {
            max_output_tokens: max_tokens,
            ..Default::default()
        },
    );
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        crate::chat::complete(
            st,
            &s.chat_backend,
            req,
            usage::purpose::PROACTIVE,
            tokio_util::sync::CancellationToken::new(),
        ),
    )
    .await
    .ok()?
    .ok()?;
    let banned = crate::filter::BannedWords::new(&prefs.banned_words);
    let (text, _) = crate::markers::strip(&banned.apply(&out.text));
    (!text.trim().is_empty()).then_some(text)
}

/// 用户自己点的（比如「现在播报一次」）：不排队、不看冷却，马上说
pub fn say_now(app: &AppHandle, item: Item) {
    let id = {
        let st = app.state::<AppState>();
        let mut g = st.proactive.lock();
        g.last_spoke = memory::now_secs();
        g.next_id += 1;
        g.next_id
    };
    deliver(app, item, Decision::Speak, id);
}

fn deliver(app: &AppHandle, item: Item, decision: Decision, id: u64) {
    let st = app.state::<AppState>();
    if item.store
        && !item.ephemeral
        && let Some(c) = st.current_character()
    {
        let now = memory::now_secs();
        let paused = st.settings().memory_paused;
        let db = st.db();
        if let Ok(conv) = conversation::current_or_new(&db, &c.manifest.id, now, paused) {
            let _ =
                conversation::append(&db, conv, crate::backends::Role::Assistant, &item.text, now);
        }
    }
    log::info!("主动发言：{:?}（{:?}）", item.kind, decision);
    let msg = ProactiveMessage {
        id,
        kind: item.kind,
        text: item.text.clone(),
        popped: decision == Decision::Speak,
        choices: item
            .choices
            .as_ref()
            .map(|c| c.options.clone())
            .unwrap_or_default(),
        note: item.note.clone(),
        update: item.update.clone(),
    };
    if let Some(c) = &item.choices {
        let mut g = st
            .companion
            .choices
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        g.retain(|k, _| *k + 20 > id);
        g.insert(id, c.clone());
    }
    let _ = app.emit_to("chat", "chat-proactive", msg);
    if decision == Decision::Notify {
        // 全屏时：系统通知（P1 接入）；聊天记录里已经有了，退出全屏后打开聊天能看到
        crate::notify::send(app, &item.text);
        return;
    }
    chat::emit_pet_state(app, item.mood);
    crate::daily::pop_chat(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> CompanionSettings {
        CompanionSettings::default()
    }

    fn free() -> Gate {
        Gate {
            minute_of_day: 15 * 60,
            ..Default::default()
        }
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn quiet_hours_cross_midnight() {
        let mut c = s();
        assert!(!c.in_quiet(0), "默认不开安静时段");
        c.quiet_enabled = true;
        c.quiet_start = "23:30".into();
        c.quiet_end = "08:00".into();
        assert!(c.in_quiet(23 * 60 + 45));
        assert!(c.in_quiet(3 * 60));
        assert!(!c.in_quiet(8 * 60));
        assert!(!c.in_quiet(12 * 60));
        c.quiet_start = "13:00".into();
        c.quiet_end = "14:00".into();
        assert!(c.in_quiet(13 * 60 + 30));
        assert!(!c.in_quiet(14 * 60));
    }

    #[test]
    fn reminders_pass_dnd_but_not_fullscreen() {
        let g = Gate {
            dnd: true,
            private_foreground: true,
            ..free()
        };
        assert_eq!(
            decide(Kind::Reminder, &s(), &g, &Pace::default(), NOW),
            Decision::Speak
        );
        let fs = Gate {
            fullscreen: true,
            ..free()
        };
        assert_eq!(
            decide(Kind::Reminder, &s(), &fs, &Pace::default(), NOW),
            Decision::Notify
        );
        let no_notify = CompanionSettings {
            system_notify: false,
            ..s()
        };
        assert_eq!(
            decide(Kind::Reminder, &no_notify, &fs, &Pace::default(), NOW),
            Decision::Wait
        );
        // 总开关关了也照样提醒
        let off = CompanionSettings {
            enabled: false,
            ..s()
        };
        assert_eq!(
            decide(Kind::Reminder, &off, &free(), &Pace::default(), NOW),
            Decision::Speak
        );
    }

    #[test]
    fn others_respect_dnd_quiet_private_and_switch() {
        for g in [
            Gate {
                dnd: true,
                ..free()
            },
            Gate {
                private_foreground: true,
                ..free()
            },
        ] {
            for k in [Kind::Care, Kind::Briefing, Kind::Activity, Kind::Random] {
                assert_eq!(decide(k, &s(), &g, &Pace::default(), NOW), Decision::Drop);
            }
        }
        let quiet = CompanionSettings {
            quiet_enabled: true,
            quiet_start: "14:00".into(),
            quiet_end: "16:00".into(),
            ..s()
        };
        assert_eq!(
            decide(Kind::Care, &quiet, &free(), &Pace::default(), NOW),
            Decision::Drop
        );
        let off = CompanionSettings {
            enabled: false,
            ..s()
        };
        assert_eq!(
            decide(Kind::Briefing, &off, &free(), &Pace::default(), NOW),
            Decision::Drop
        );
        assert_eq!(
            decide(Kind::Activity, &s(), &free(), &Pace::default(), NOW),
            Decision::Speak
        );
    }

    #[test]
    fn fullscreen_and_away_wait_or_drop_by_kind() {
        for g in [
            Gate {
                fullscreen: true,
                ..free()
            },
            Gate {
                user_away: true,
                ..free()
            },
        ] {
            assert_eq!(
                decide(Kind::Care, &s(), &g, &Pace::default(), NOW),
                Decision::Wait
            );
            assert_eq!(
                decide(Kind::Briefing, &s(), &g, &Pace::default(), NOW),
                Decision::Wait
            );
            assert_eq!(
                decide(Kind::Activity, &s(), &g, &Pace::default(), NOW),
                Decision::Drop
            );
            assert_eq!(
                decide(Kind::Random, &s(), &g, &Pace::default(), NOW),
                Decision::Drop
            );
        }
    }

    #[test]
    fn deep_focus_postpones_care_only() {
        let g = Gate {
            deep_focus: true,
            ..free()
        };
        assert_eq!(
            decide(Kind::Care, &s(), &g, &Pace::default(), NOW),
            Decision::Wait
        );
        assert_eq!(
            decide(Kind::Briefing, &s(), &g, &Pace::default(), NOW),
            Decision::Wait
        );
        assert_eq!(
            decide(Kind::Activity, &s(), &g, &Pace::default(), NOW),
            Decision::Speak
        );
        assert_eq!(
            decide(Kind::Reminder, &s(), &g, &Pace::default(), NOW),
            Decision::Speak
        );
    }

    #[test]
    fn cooldown_and_backoff() {
        let c = s(); // 20 分钟
        let recent = Pace {
            last_spoke: NOW - 10 * 60,
            ..Default::default()
        };
        assert_eq!(
            decide(Kind::Care, &c, &free(), &recent, NOW),
            Decision::Wait
        );
        assert_eq!(
            decide(Kind::Activity, &c, &free(), &recent, NOW),
            Decision::Drop
        );
        let older = Pace {
            last_spoke: NOW - 25 * 60,
            ..Default::default()
        };
        assert_eq!(
            decide(Kind::Activity, &c, &free(), &older, NOW),
            Decision::Speak
        );
        // 连续被忽略 3 次：冷却 ×4 = 80 分钟
        let ignored = Pace {
            ignored_streak: 3,
            ..older
        };
        assert_eq!(c.cooldown_secs(Kind::Care, 3), 80 * 60);
        assert_eq!(
            decide(Kind::Activity, &c, &free(), &ignored, NOW),
            Decision::Drop
        );
        assert_eq!(c.cooldown_secs(Kind::Care, 1), 20 * 60, "忽略一次不惩罚");
        assert_eq!(c.cooldown_secs(Kind::Care, 99), 160 * 60, "封顶 ×8");
        // 频率：很少 = 冷却翻倍，只影响活动评论和小事件
        let rare = CompanionSettings {
            frequency: Frequency::Rare,
            ..s()
        };
        assert_eq!(rare.cooldown_secs(Kind::Random, 0), 40 * 60);
        assert_eq!(rare.cooldown_secs(Kind::Care, 0), 20 * 60);
    }

    #[test]
    fn daily_cap_by_frequency() {
        let p = Pace {
            said_today: 5,
            ..Default::default()
        };
        assert_eq!(decide(Kind::Random, &s(), &free(), &p, NOW), Decision::Drop);
        assert_eq!(decide(Kind::Care, &s(), &free(), &p, NOW), Decision::Speak);
        let often = CompanionSettings {
            frequency: Frequency::Often,
            ..s()
        };
        assert_eq!(
            decide(Kind::Random, &often, &free(), &p, NOW),
            Decision::Speak
        );
    }

    #[test]
    fn queue_takes_by_priority_and_expires() {
        let st = ProactiveState::default();
        st.submit(Item::new(Kind::Random, "小事件", NOW + 60));
        st.submit(Item::new(Kind::Reminder, "收衣服", NOW + 3600));
        st.submit(Item::new(Kind::Activity, "过期的", NOW - 1));
        let (it, d, id1) = st.take_due(&s(), &free(), NOW, "d1").unwrap();
        assert_eq!((it.text.as_str(), d), ("收衣服", Decision::Speak));
        // 提醒不占冷却之外的名额，但会刷新「上次说话」：小事件要等冷却
        assert!(st.take_due(&s(), &free(), NOW + 30, "d1").is_none());
        assert!(!st.has_pending(), "小事件不能等，被丢掉；过期的也被清掉");
        st.submit(Item::new(Kind::Care, "喝水", NOW + 3600));
        let (it, _, id2) = st.take_due(&s(), &free(), NOW + 21 * 60, "d1").unwrap();
        assert_eq!(it.text, "喝水");
        assert!(id2 > id1);
    }

    #[test]
    fn queue_dedupes_by_key_and_waits() {
        let st = ProactiveState::default();
        let mut a = Item::new(Kind::Briefing, "播报", NOW + 3600);
        a.key = "briefing:d1".into();
        st.submit(a.clone());
        st.submit(a);
        let away = Gate {
            user_away: true,
            ..free()
        };
        assert!(st.take_due(&s(), &away, NOW, "d1").is_none());
        assert!(st.has_pending(), "播报等用户回来");
        assert!(st.take_due(&s(), &free(), NOW + 60, "d1").is_some());
        assert!(!st.has_pending(), "同一个 key 只排一次");
    }

    #[test]
    fn ignore_streak_resets_on_engagement() {
        let st = ProactiveState::default();
        assert_eq!(st.ignored(), 1);
        assert_eq!(st.ignored(), 2);
        st.engaged();
        assert_eq!(st.ignored_streak(), 0);
    }

    #[test]
    fn model_budget_for_proactive() {
        let db = crate::db::open_in_memory().unwrap();
        let c = s();
        let g = Budget {
            daily_usd: 1.0,
            monthly_usd: 10.0,
        };
        assert!(model_allowed(&db, &c, &g, "2026-10-28", "2026-10"));
        let rec = |purpose: &'static str, cost: f64| usage::Record {
            backend_id: "deepseek",
            model: "m",
            purpose,
            usage: Default::default(),
            cost_usd: cost,
        };
        // 主动行为自己的花费上限
        usage::record_at(
            &db,
            &rec(usage::purpose::PROACTIVE, 0.05),
            "2026-10-28",
            "2026-10",
        )
        .unwrap();
        assert!(!model_allowed(&db, &c, &g, "2026-10-28", "2026-10"));
        assert!(model_allowed(&db, &c, &g, "2026-10-29", "2026-10"));
        // 聊天把全局预算用到八成：主动行为先停
        usage::record_at(
            &db,
            &rec(usage::purpose::CHAT, 0.8),
            "2026-10-29",
            "2026-10",
        )
        .unwrap();
        assert!(!model_allowed(&db, &c, &g, "2026-10-29", "2026-10"));
        // 次数上限
        let zero = CompanionSettings {
            daily_calls: 0,
            ..s()
        };
        assert!(!model_allowed(&db, &zero, &g, "2026-10-30", "2026-10"));
    }

    #[test]
    fn sanitize_settings() {
        let c = CompanionSettings {
            cooldown_minutes: 0,
            daily_usd: f64::NAN,
            quiet_start: "25:00".into(),
            ..s()
        }
        .sanitize();
        assert_eq!(c.cooldown_minutes, 5);
        assert_eq!(c.daily_usd, 0.0);
        assert_eq!(c.quiet_start, "23:30");
        assert_eq!(parse_hm("7:05"), Some(425));
        assert_eq!(parse_hm("x"), None);
    }
}
