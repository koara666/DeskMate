//! 角色感（v2 P4，DECISIONS #41）：心情、亲密度、触摸反应、随机小事件、聊天里记下生日纪念日。
//!
//! - 心情：-100 ~ 100，受互动、游戏结果、天气影响，按半衰期 3 小时自然回到 0（读的时候算，不用定时器）。
//!   体现在待机图片（有对应图片时）、台词池的 `[心情好]` / `[心情差]` 条件和提示词里的一句话。
//! - 亲密度：积分只增不减（**用户离开多久都不会掉**，没有惩罚），每天最多 +20；6 级，解锁称呼、台词和话题。
//! - 每个角色各自一份，存在 settings 表的 kv（`bond:<角色 id>`）。

use crate::proactive::{self, Item, Kind};
use crate::state::AppState;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// 各级需要的积分
pub const LEVELS: [i64; 6] = [0, 30, 100, 250, 500, 1000];
const DAILY_CAP: i64 = 20;
const MOOD_HALF_LIFE_SECS: f64 = 3.0 * 3600.0;

/// 没写 `affinity_titles` 的角色用的关系描述
pub const DEFAULT_TITLES: [&str; 6] = [
    "刚认识的人",
    "渐渐熟悉起来的人",
    "朋友",
    "好朋友",
    "很亲近的人",
    "最重要的人",
];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bond {
    pub mood: f64,
    pub mood_at: i64,
    pub points: i64,
    /// 今天（按早上 5 点分天）已经加了多少分
    pub day: String,
    pub day_points: i64,
    /// 今天各种互动加分的次数（防止刷分）
    pub day_counts: HashMap<String, u32>,
}

pub fn level_of(points: i64) -> u8 {
    LEVELS.iter().rposition(|&t| points >= t).unwrap_or(0) as u8
}

impl Bond {
    /// 现在的心情（按半衰期回落到 0）
    pub fn mood_now(&self, now: i64) -> i32 {
        let dt = (now - self.mood_at).max(0) as f64;
        (self.mood * 0.5f64.powf(dt / MOOD_HALF_LIFE_SECS)).round() as i32
    }

    pub fn add_mood(&mut self, delta: f64, now: i64) {
        self.mood = (self.mood_now(now) as f64 + delta).clamp(-100.0, 100.0);
        self.mood_at = now;
    }

    pub fn level(&self) -> u8 {
        level_of(self.points)
    }

    /// 加亲密度（每天上限；同一种互动每天也有次数上限）。升级了返回新等级。
    pub fn add_points(&mut self, kind: &str, pts: i64, max_times: u32, day: &str) -> Option<u8> {
        if self.day != day {
            self.day = day.to_string();
            self.day_points = 0;
            self.day_counts.clear();
        }
        let n = self.day_counts.entry(kind.to_string()).or_default();
        if *n >= max_times || self.day_points >= DAILY_CAP {
            return None;
        }
        *n += 1;
        let before = self.level();
        let add = pts.min(DAILY_CAP - self.day_points);
        self.points += add;
        self.day_points += add;
        let after = self.level();
        (after > before).then_some(after)
    }
}

fn key(id: &str) -> String {
    format!("bond:{id}")
}

pub fn load(db: &Connection, id: &str) -> Bond {
    crate::settings::kv_get(db, &key(id))
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(db: &Connection, id: &str, b: &Bond) {
    if let Ok(s) = serde_json::to_string(b) {
        let _ = crate::settings::kv_set(db, &key(id), &s);
    }
}

/// 心情的一个词（提示词、界面用）
pub fn mood_word(m: i32) -> &'static str {
    match m {
        i32::MIN..=-50 => "有点低落",
        -49..=-20 => "稍微有点闷",
        -19..=19 => "平常",
        20..=59 => "不错",
        _ => "很好",
    }
}

/// 互动
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    Chat,
    PatHead,
    Poke,
    Drag,
    /// 每天第一次来
    FirstToday,
    /// 理会了主动说的话（点了、回答了小事件）
    Engaged,
    GameWin,
    GameLose,
    GameDraw,
    Rainy,
    Sunny,
}

/// 各种互动对心情和亲密度的影响：（心情变化，加分，每天最多几次）
fn effect(i: Interaction, competitive: bool) -> (f64, i64, u32, &'static str) {
    match i {
        Interaction::Chat => (2.0, 1, 10, "chat"),
        Interaction::PatHead => (6.0, 1, 5, "pat"),
        Interaction::Poke => (1.0, 0, 0, "poke"),
        Interaction::Drag => (0.0, 0, 0, "drag"),
        Interaction::FirstToday => (0.0, 3, 1, "first"),
        Interaction::Engaged => (3.0, 1, 5, "engaged"),
        // 好胜的角色输了会更不服气（心情掉得多一点），但游戏总能加亲密度
        Interaction::GameWin => (if competitive { 12.0 } else { 6.0 }, 2, 5, "game"),
        Interaction::GameLose => (if competitive { -8.0 } else { 2.0 }, 2, 5, "game"),
        Interaction::GameDraw => (1.0, 2, 5, "game"),
        Interaction::Rainy => (-5.0, 0, 0, "weather"),
        Interaction::Sunny => (3.0, 0, 0, "weather"),
    }
}

/// 记一次互动：更新心情和亲密度；升级时让角色说一句（台词池 `affinity.level_up`）
pub fn interact(app: &AppHandle, i: Interaction) {
    let st = app.state::<AppState>();
    let Some(c) = st.current_character() else {
        return;
    };
    let now = crate::memory::now_secs();
    let day = crate::briefing::briefing_day(crate::reminders::now_local()).to_string();
    let competitive = c.manifest.game_style.competitive;
    let (mood, pts, times, kind) = effect(i, competitive);
    let (up, mood_before, mood_after) = {
        let db = st.db();
        let mut b = load(&db, &c.manifest.id);
        let before = b.mood_now(now);
        b.add_mood(mood, now);
        let up = if pts > 0 {
            b.add_points(kind, pts, times, &day)
        } else {
            None
        };
        save(&db, &c.manifest.id, &b);
        (up, before, b.mood_now(now))
    };
    // 心情跨过「好 / 差」的线时，换待机图片
    if (mood_before >= 30) != (mood_after >= 30) || (mood_before <= -30) != (mood_after <= -30) {
        let _ = app.emit_to("pet", "pet-variant", ());
    }
    if let Some(lv) = up {
        log::info!("亲密度升到 {lv} 级");
        if let Some(text) = st.line("affinity.level_up", &[]) {
            let mut item = Item::new(Kind::Briefing, text, now + 6 * 3600);
            item.key = format!("level_up:{lv}");
            proactive::submit(app, item);
        }
    }
}

/// 当前角色的（亲密度等级，心情）
pub fn current(st: &AppState) -> (u8, i32) {
    let Some(c) = st.current_character() else {
        return (0, 0);
    };
    let b = load(&st.db(), &c.manifest.id);
    (b.level(), b.mood_now(crate::memory::now_secs()))
}

/// 对用户的称呼：亲密度够了、用户也设了昵称就用昵称，否则「你」
pub fn call_name(st: &AppState) -> String {
    let nick = st.settings().nickname;
    let from = st
        .current_character()
        .map(|c| c.manifest.call_from_level)
        .unwrap_or(2);
    if !nick.is_empty() && current(st).0 >= from {
        nick
    } else {
        "你".into()
    }
}

/// 提示词里关于关系的一句（几乎不变，放可缓存的部分）
pub fn relation_line(st: &AppState) -> String {
    let Some(c) = st.current_character() else {
        return String::new();
    };
    let (lv, _) = current(st);
    let title = c
        .manifest
        .affinity_titles
        .get(lv as usize)
        .map(String::as_str)
        .unwrap_or(DEFAULT_TITLES[lv as usize]);
    let call = call_name(st);
    let mut s = format!(
        "你们现在的关系：对你来说，用户是「{title}」（亲密度 {lv}/5，慢慢积累，不会减少）。称呼用户：{call}。"
    );
    if lv <= 1 {
        s.push_str("还不太熟，说话自然、有分寸一点。");
    }
    s
}

/// 提示词里关于心情的一句（每轮可能变，放在背景资料里）
pub fn mood_line(st: &AppState) -> Option<String> {
    let (_, m) = current(st);
    (m.abs() >= 20).then(|| format!("你现在的心情：{}（自然流露就好，不用说出来）", mood_word(m)))
}

// ---------------- 待机图片变体 ----------------

/// 待机时想用的图片变体，按优先级（心情 > 天气 > 夜里 > 季节）；桌宠用第一个有图的，没有就用 idle
pub fn idle_variants(
    mood: i32,
    rainy: bool,
    snowy: bool,
    hour: u32,
    month: u32,
) -> Vec<&'static str> {
    let mut v = Vec::new();
    if mood >= 30 {
        v.push("idle_good");
    } else if mood <= -30 {
        v.push("idle_low");
    }
    if snowy {
        v.push("idle_snow");
    } else if rainy {
        v.push("idle_rain");
    }
    if !(5..22).contains(&hour) {
        v.push("idle_night");
    }
    match month {
        6..=8 => v.push("idle_summer"),
        12 | 1 | 2 => v.push("idle_winter"),
        _ => {}
    }
    v
}

/// 现在的变体偏好（天气用播报缓存里用户所在城市的，没查过就不管天气）
pub fn current_variants(st: &AppState) -> Vec<&'static str> {
    use chrono::{Datelike, Timelike};
    let (_, mood) = current(st);
    let now = crate::reminders::now_local();
    let (mut rainy, mut snowy) = (false, false);
    if let Some(c) = st.settings().briefing.city
        && let Some(w) = st.briefing.weather.get(
            &crate::briefing::weather::cache_key(&c),
            crate::memory::now_secs(),
        )
        && let Some(d) = w.days.first()
    {
        rainy = crate::briefing::weather::is_rain(d.code);
        snowy = crate::briefing::weather::is_snow(d.code);
    }
    let mut v = idle_variants(mood, rainy, snowy, now.hour(), now.month());
    // 勿扰时戴上兜帽（有这张图的角色）
    if st.settings().dnd {
        v.insert(0, "hood");
    }
    v
}

// ---------------- 触摸 ----------------

#[derive(Default)]
pub struct TouchState {
    pats: Mutex<Vec<i64>>,
    last_reaction: Mutex<HashMap<&'static str, i64>>,
    /// 等用户选择的小事件：消息 id → （问题，选项，回应）
    pub choices: Mutex<HashMap<u64, Choice>>,
    /// 正在拖她：「惊讶」图要撑到松手（松手时 `end_drag_pose` 清掉）
    dragging: AtomicBool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub question: String,
    pub options: Vec<String>,
    pub replies: Vec<String>,
}

/// 摸头的台词场景（纯函数）：20 秒内第 3 次换一组，30 秒内 8 次以上「头发要乱了」
pub fn pat_scene(recent: &[i64], now: i64, rub: bool) -> &'static str {
    let in20 = recent.iter().filter(|t| now - **t < 20).count();
    let in30 = recent.iter().filter(|t| now - **t < 30).count();
    if in30 >= 8 {
        "touch.head_lots"
    } else if in20 >= 3 {
        "touch.head_many"
    } else if rub {
        "touch.rub"
    } else {
        "touch.head"
    }
}

/// 摸头（点头顶 / 在头上来回蹭）、戳身体、拖动、按住不放、连戳：返回（要说的话，台词场景），话不存进聊天记录。
/// 拖动、戳有冷却，不是每次都说。
pub fn touch(app: &AppHandle, part: &str) -> Option<(String, &'static str)> {
    let st = app.state::<AppState>();
    let now = crate::memory::now_secs();
    let (scene, cooldown, interaction): (&'static str, i64, Interaction) = match part {
        "head" | "rub" => {
            let mut p = st.companion.pats.lock().unwrap_or_else(|e| e.into_inner());
            p.retain(|t| now - t < 30);
            p.push(now);
            let scene = pat_scene(&p, now, part == "rub");
            if scene == "touch.head_lots" {
                p.clear();
            }
            (scene, 0, Interaction::PatHead)
        }
        "drag" => ("touch.drag", 180, Interaction::Drag),
        "hold" => ("touch.hold", 30, Interaction::Poke),
        "poke" => ("touch.poke_many", 60, Interaction::Poke),
        _ => ("touch.body", 120, Interaction::Poke),
    };
    interact(app, interaction);
    if part == "drag" {
        st.companion.dragging.store(true, Ordering::SeqCst);
    }
    emit_pose(app, scene);
    if cooldown > 0 {
        let mut last = st
            .companion
            .last_reaction
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if now - last.get(scene).copied().unwrap_or(0) < cooldown {
            return None;
        }
        // 戳身体一半的时候才说话（多数时候只是打开聊天）
        if part == "body" && crate::lines::seed() % 2 == 0 {
            return None;
        }
        last.insert(scene, now);
    }
    st.line(scene, &[]).map(|t| (t, scene))
}

/// 反应时桌宠短暂换上的图（纯函数）：台词场景 → （图名，显示多久毫秒）。没画这张图的角色保持原样。
pub fn pose_for(scene: &str) -> Option<(&'static str, u32)> {
    Some(match scene {
        "touch.head" | "touch.rub" | "touch.head_many" => ("pat", 2200),
        "touch.head_lots" => ("shy", 2600),
        "touch.hold" | "menu.repeat" | "menu.indecisive" | "menu.hold_long" => ("confused", 2400),
        "touch.poke_many" | "menu.dnd_spam" => ("pout", 2600),
        // 拖着不放就一直是这张，松手时 `end_drag_pose` 清掉；30 秒是保险（松手的事件万一丢了也会自己换回来）
        "touch.drag" => ("surprised", 30_000),
        "touch.shake" => ("dizzy", 3200),
        "touch.carry" | "menu.secret" => ("wink", 2400),
        "touch.wake" => ("wake", 2600),
        "intent.hide" | "menu.hide" | "intent.dnd_on" => ("hood", 2400),
        "update.available" | "vision.refuse_password" => ("surprised", 2400),
        // 小游戏的事件是从用户的角度说的：用户赢了 = 她输了
        "game.win" => ("lose", 3500),
        "game.lose" => ("win", 3500),
        _ => return None,
    })
}

/// 说完这句反应的话以后桌宠的心情图：有专属反应图的（惊讶、生气、晕……）图本身已经表达了情绪，
/// 播完回到平常，不要再盖一层笑脸（以前所有主动说的话一律置成 happy，拖她、戳她时她会笑）；
/// 摸头是舒服的，保留笑；没有专属图的（点身体）照旧。
pub fn reaction_mood(scene: &str) -> crate::chat::PetState {
    use crate::chat::PetState;
    match scene {
        "touch.head" | "touch.rub" | "touch.head_many" => PetState::Happy,
        s if pose_for(s).is_some() => PetState::Idle,
        _ => PetState::Happy,
    }
}

/// 拖完松手：把「惊讶」图换回来（`pose` 为空 = 清掉反应图）
pub fn end_drag_pose(app: &AppHandle) {
    let st = app.state::<AppState>();
    if st.companion.dragging.swap(false, Ordering::SeqCst) {
        let _ = app.emit_to("pet", "pet-pose", PoseEvent { pose: "", ms: 0 });
    }
}

/// 让桌宠按场景短暂换一张反应图
pub fn emit_pose(app: &AppHandle, scene: &str) {
    if let Some((pose, ms)) = pose_for(scene) {
        let _ = app.emit_to("pet", "pet-pose", PoseEvent { pose, ms });
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PoseEvent {
    pub pose: &'static str,
    pub ms: u32,
}

/// 在「她说的话」气泡里说一句小反应（不记进聊天记录）。台词池里没有就不说。
pub fn say_reaction(app: &AppHandle, scene: &str, vars: &[(&str, String)]) {
    emit_pose(app, scene);
    let Some(text) = app.state::<AppState>().line(scene, vars) else {
        return;
    };
    let mut item = Item::new(Kind::Random, text, crate::memory::now_secs() + 60);
    item.ephemeral = true;
    item.mood = reaction_mood(scene);
    proactive::say_now(app, item);
}

/// 拖完松手：甩来甩去会晕，拖了很久会问是不是搬家（有冷却）
pub fn drag_gesture(app: &AppHandle, g: crate::geometry::DragGesture) {
    let st = app.state::<AppState>();
    let now = crate::memory::now_secs();
    let scene = match g {
        crate::geometry::DragGesture::Shake => "touch.shake",
        crate::geometry::DragGesture::Carry => "touch.carry",
    };
    {
        let mut last = st
            .companion
            .last_reaction
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if now - last.get(scene).copied().unwrap_or(0) < 60 {
            return;
        }
        last.insert(scene, now);
    }
    if g == crate::geometry::DragGesture::Shake {
        add_mood_only(app, -2.0);
    }
    say_reaction(app, scene, &[]);
}

fn add_mood_only(app: &AppHandle, delta: f64) {
    let st = app.state::<AppState>();
    let Some(c) = st.current_character() else {
        return;
    };
    let db = st.db();
    let mut b = load(&db, &c.manifest.id);
    b.add_mood(delta, crate::memory::now_secs());
    save(&db, &c.manifest.id, &b);
}

// ---------------- 随机小事件 ----------------

/// 解析「问题 | 选项 => 回应 | 选项 => 回应」
pub fn parse_choice(line: &str) -> Option<Choice> {
    let mut parts = line.split('|').map(str::trim);
    let question = parts.next()?.to_string();
    let mut options = Vec::new();
    let mut replies = Vec::new();
    for p in parts {
        let (o, r) = p.split_once("=>")?;
        let (o, r) = (o.trim(), r.trim());
        if o.is_empty() || r.is_empty() {
            return None;
        }
        options.push(o.chars().take(20).collect());
        replies.push(r.to_string());
    }
    (!question.is_empty() && (2..=4).contains(&options.len())).then_some(Choice {
        question,
        options,
        replies,
    })
}

/// 小事件该不该来（纯函数）：用户在电脑前、没在忙、离上次小事件够久，再掷个骰子
pub fn event_due(
    enabled: bool,
    idle_secs: u64,
    busy: bool,
    since_last: i64,
    min_gap: i64,
    roll: u64,
) -> bool {
    enabled && idle_secs < 60 && !busy && since_last >= min_gap && roll % 20 == 0
}

static LAST_EVENT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// 日常循环每 30 秒调用一次
pub fn check_event(app: &AppHandle) {
    if crate::paths::test_quiet() {
        return;
    }
    let st = app.state::<AppState>();
    let s = st.settings().companion;
    let now = crate::memory::now_secs();
    let last = LAST_EVENT.load(std::sync::atomic::Ordering::Relaxed);
    if last == 0 {
        // 刚启动：至少过一阵子再说
        LAST_EVENT.store(now, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    let min_gap = match s.frequency {
        proactive::Frequency::Rare => 4 * 3600,
        proactive::Frequency::Sometimes => 2 * 3600,
        proactive::Frequency::Often => 3600,
    };
    let (_, deep) = crate::activity::gate_flags(&st);
    let busy = deep || st.daily.is_fullscreen() || st.chat.is_generating();
    if !event_due(
        s.enabled && s.events,
        crate::daily::system_idle_secs(),
        busy,
        now - last,
        min_gap,
        crate::lines::seed(),
    ) {
        return;
    }
    LAST_EVENT.store(now, std::sync::atomic::Ordering::Relaxed);
    let call = call_name(&st);
    let vars = [("称呼", call)];
    let roll = crate::lines::seed() % 10;
    let mut item = if roll < 3 {
        let Some(raw) = st.line("event.choice", &vars) else {
            return;
        };
        let Some(ch) = parse_choice(&raw) else {
            log::warn!("小事件的选项格式不对，跳过");
            return;
        };
        let mut it = Item::new(Kind::Random, ch.question.clone(), now + 300);
        it.choices = Some(ch);
        it
    } else {
        let scene = if roll < 6 {
            "event.question"
        } else {
            "event.share"
        };
        let Some(text) = st.line(scene, &vars) else {
            return;
        };
        Item::new(Kind::Random, text, now + 300)
    };
    item.key = "event".into();
    proactive::submit(app, item);
}

/// 用户在小事件里选了一项：角色的回应（存进聊天记录，之后聊天能接上）
pub fn choose(app: &AppHandle, id: u64, index: usize) -> Result<String, String> {
    let st = app.state::<AppState>();
    let ch = st
        .companion
        .choices
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id)
        .ok_or("这个问题已经过去啦")?;
    let opt = ch.options.get(index).ok_or("没有这个选项")?.clone();
    let reply = ch.replies[index].clone();
    if let Some(c) = st.current_character() {
        let now = crate::memory::now_secs();
        let paused = st.settings().memory_paused;
        let db = st.db();
        if let Ok(conv) =
            crate::memory::conversation::current_or_new(&db, &c.manifest.id, now, paused)
        {
            use crate::backends::Role;
            let _ = crate::memory::conversation::append(&db, conv, Role::User, &opt, now);
            let _ = crate::memory::conversation::append(&db, conv, Role::Assistant, &reply, now);
        }
    }
    interact(app, Interaction::Engaged);
    Ok(reply)
}

// ---------------- 聊天里记下生日、纪念日 ----------------

/// 「我的生日是 3 月 5 号」「11月3日是我们的纪念日」→（MM-DD，名称）
pub fn detect_special_date(text: &str) -> Option<(String, String)> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let label = if t.contains("生日")
        && (t.contains("我的生日")
            || t.contains("我生日")
            || t.contains("是我生日")
            || t.contains("是我的生日"))
    {
        "生日"
    } else if t.contains("纪念日") && (t.contains("我们") || t.contains("我的")) {
        "纪念日"
    } else {
        return None;
    };
    // 问句（「你知道我生日是几号吗」）不算
    if t.contains('吗')
        || t.contains('？')
        || t.contains('?')
        || t.contains("几号")
        || t.contains("哪天")
    {
        return None;
    }
    let chars: Vec<char> = t.chars().collect();
    let num = |s: &[char]| -> Option<u32> {
        let x: String = s.iter().collect();
        if let Ok(n) = x.parse() {
            return Some(n);
        }
        // 中文数字（一到三十一）
        let d = |c: char| {
            "零一二三四五六七八九"
                .chars()
                .position(|x| x == c)
                .map(|p| p as u32)
        };
        match s {
            ['十'] => Some(10),
            [a] => d(*a),
            ['十', b] => Some(10 + d(*b)?),
            [a, '十'] => Some(d(*a)? * 10),
            [a, '十', b] => Some(d(*a)? * 10 + d(*b)?),
            _ => None,
        }
    };
    let is_n = |c: char| c.is_ascii_digit() || "零一二三四五六七八九十".contains(c);
    let mi = chars.iter().position(|&c| c == '月')?;
    let mut ms = mi;
    while ms > 0 && is_n(chars[ms - 1]) {
        ms -= 1;
    }
    let m = num(&chars[ms..mi])?;
    let mut de = mi + 1;
    while de < chars.len() && is_n(chars[de]) {
        de += 1;
    }
    let d = num(&chars[mi + 1..de])?;
    if !chars.get(de).is_some_and(|&c| c == '日' || c == '号') {
        return None;
    }
    let mmdd = format!("{m:02}-{d:02}");
    chrono::NaiveDate::parse_from_str(&format!("2024-{mmdd}"), "%Y-%m-%d").ok()?;
    Some((mmdd, label.into()))
}

/// 聊天时：如果用户说了生日 / 纪念日就记进设置，并告诉模型已经记下了
pub fn remember_special_date(app: &AppHandle, text: &str) -> Option<String> {
    let (mmdd, label) = detect_special_date(text)?;
    let st = app.state::<AppState>();
    let exists = st
        .settings()
        .briefing
        .special_dates
        .iter()
        .any(|d| d.date == mmdd && d.label == label);
    if !exists {
        st.update_settings(|s| {
            s.briefing
                .special_dates
                .retain(|d| d.label != label || label != "生日");
            s.briefing.special_dates.push(crate::briefing::SpecialDate {
                date: mmdd.clone(),
                label: label.clone(),
            });
        });
        log::info!("记下了一个特殊日期");
    }
    let (m, d) = mmdd.split_once('-')?;
    Some(format!(
        "程序已经把用户的{label}（{} 月 {} 日）记下来了，那天会特别问候。可以自然地回应一句「记住了」。",
        m.trim_start_matches('0'),
        d.trim_start_matches('0')
    ))
}

#[cfg(test)]
mod tests {

    #[test]
    fn reaction_poses_exist_in_the_pose_list() {
        for scene in [
            "touch.head",
            "touch.rub",
            "touch.head_lots",
            "touch.hold",
            "touch.poke_many",
            "touch.drag",
            "touch.shake",
            "touch.carry",
            "touch.wake",
            "menu.hide",
            "intent.dnd_on",
            "game.win",
            "game.lose",
        ] {
            let (pose, ms) = pose_for(scene).unwrap();
            assert!(crate::character::POSES.contains(&pose), "{scene} → {pose}");
            assert!(ms >= 1000);
        }
        assert_eq!(pose_for("game.win").unwrap().0, "lose", "用户赢了 = 她输了");
        assert_eq!(pose_for("touch.body"), None, "点身体是开聊天，不换图");
    }

    #[test]
    fn reaction_mood_follows_the_pose() {
        use crate::chat::PetState;
        // 有专属图的反应：播完回平常，不再盖一层笑脸（以前拖她、戳她时她会笑）
        for scene in [
            "touch.drag",
            "touch.hold",
            "touch.poke_many",
            "touch.shake",
            "touch.head_lots",
            "touch.wake",
            "menu.hide",
        ] {
            assert_eq!(reaction_mood(scene), PetState::Idle, "{scene}");
        }
        // 摸头是舒服的，保留笑；没有专属图的（点身体）照旧
        for scene in ["touch.head", "touch.rub", "touch.head_many", "touch.body"] {
            assert_eq!(reaction_mood(scene), PetState::Happy, "{scene}");
        }
        // 拖动的图撑到松手（比前端最长的普通反应图还久）
        assert!(pose_for("touch.drag").unwrap().1 >= 10_000);
    }

    #[test]
    fn pat_scenes() {
        let now = 1000;
        assert_eq!(pat_scene(&[now], now, false), "touch.head");
        assert_eq!(pat_scene(&[now], now, true), "touch.rub");
        assert_eq!(
            pat_scene(&[now - 5, now - 2, now], now, false),
            "touch.head_many"
        );
        let lots: Vec<i64> = (0..8).map(|i| now - i * 3).collect();
        assert_eq!(pat_scene(&lots, now, true), "touch.head_lots");
        // 很久以前摸过的不算
        assert_eq!(
            pat_scene(&[now - 100, now - 90, now], now, false),
            "touch.head"
        );
    }

    use super::*;

    #[test]
    fn mood_decays_toward_zero() {
        let mut b = Bond::default();
        b.add_mood(60.0, 0);
        assert_eq!(b.mood_now(0), 60);
        assert_eq!(b.mood_now(3 * 3600), 30);
        assert_eq!(b.mood_now(6 * 3600), 15);
        b.add_mood(500.0, 0);
        assert_eq!(b.mood_now(0), 100, "有上限");
        assert_eq!(mood_word(-60), "有点低落");
        assert_eq!(mood_word(40), "不错");
    }

    #[test]
    fn affinity_only_grows_with_daily_caps() {
        let mut b = Bond::default();
        assert_eq!(b.level(), 0);
        // 聊天每天最多加 10 次
        for _ in 0..15 {
            b.add_points("chat", 1, 10, "d1");
        }
        assert_eq!(b.points, 10);
        // 每天总共最多 20 分
        for _ in 0..5 {
            b.add_points("pat", 1, 5, "d1");
        }
        b.add_points("first", 3, 1, "d1");
        assert_eq!(b.points, 18);
        b.add_points("game", 5, 5, "d1");
        assert_eq!(b.points, 20, "到上限只加到 20");
        assert_eq!(b.add_points("game", 2, 5, "d1"), None);
        // 第二天继续；跨过 30 分升到 1 级
        let mut up = None;
        for _ in 0..10 {
            up = up.or(b.add_points("chat", 1, 10, "d2"));
        }
        assert_eq!(up, Some(1));
        assert_eq!(level_of(999), 4);
        assert_eq!(level_of(5000), 5);
    }

    #[test]
    fn variant_preferences() {
        assert_eq!(
            idle_variants(50, true, false, 23, 7),
            vec!["idle_good", "idle_rain", "idle_night", "idle_summer"]
        );
        assert_eq!(
            idle_variants(0, false, true, 12, 1),
            vec!["idle_snow", "idle_winter"]
        );
        assert!(idle_variants(0, false, false, 12, 4).is_empty());
    }

    #[test]
    fn choices_parse() {
        let c = parse_choice("今天想玩什么？ | 动作游戏 => 好，手感很重要 | 解谜游戏 => 我也喜欢")
            .unwrap();
        assert_eq!(c.options, vec!["动作游戏", "解谜游戏"]);
        assert_eq!(c.replies[1], "我也喜欢");
        assert!(parse_choice("只有问题").is_none());
        assert!(parse_choice("问题 | 选项没有回应").is_none());
    }

    #[test]
    fn event_timing() {
        assert!(event_due(true, 5, false, 3 * 3600, 7200, 40));
        assert!(!event_due(true, 5, false, 3 * 3600, 7200, 41), "掷骰子");
        assert!(
            !event_due(true, 300, false, 3 * 3600, 7200, 40),
            "不在电脑前"
        );
        assert!(!event_due(true, 5, true, 3 * 3600, 7200, 40), "在忙");
        assert!(!event_due(true, 5, false, 600, 7200, 40), "刚说过");
        assert!(!event_due(false, 5, false, 3 * 3600, 7200, 40));
    }

    #[test]
    fn special_dates_from_chat() {
        assert_eq!(
            detect_special_date("我的生日是3月5号"),
            Some(("03-05".into(), "生日".into()))
        );
        assert_eq!(
            detect_special_date("十一月三日是我生日哦"),
            Some(("11-03".into(), "生日".into()))
        );
        assert_eq!(
            detect_special_date("6月20日是我们的纪念日"),
            Some(("06-20".into(), "纪念日".into()))
        );
        assert_eq!(detect_special_date("你知道我生日是几号吗"), None);
        assert_eq!(
            detect_special_date("我的生日是2月30号"),
            None,
            "不存在的日期"
        );
        assert_eq!(detect_special_date("3月5号去看电影"), None);
    }

    #[test]
    fn bond_persists_per_character() {
        let db = crate::db::open_in_memory().unwrap();
        let b = Bond {
            points: 120,
            ..Default::default()
        };
        save(&db, "nanami-chiaki", &b);
        assert_eq!(load(&db, "nanami-chiaki").level(), 2);
        assert_eq!(load(&db, "nailong").points, 0);
    }
}
