//! 日常体验（M6）：开机自启、全局快捷键、全屏时自动隐藏、长时间无操作睡觉、主动搭话。
//!
//! 后台只有一个低频循环（每 3 秒查一次全屏状态，每 30 秒查一次空闲时间和主动搭话条件），
//! 每次只是一两个系统调用，空闲 CPU 不受影响。

use crate::chat::{self, PetState};
use crate::desktop;
use crate::memory;
use crate::state::AppState;
use crate::usage;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_HOTKEY: &str = "Alt+Shift+D";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DailySettings {
    /// 开机自启（默认关闭）
    pub autostart: bool,
    pub hotkey_enabled: bool,
    pub hotkey: String,
    /// 有全屏程序（游戏、视频、演示）时自动隐藏
    pub hide_in_fullscreen: bool,
    /// 多少分钟没有任何操作后角色睡觉（0 = 不睡）
    pub sleep_minutes: u32,
    /// 主动搭话（默认关闭）
    pub proactive: bool,
    /// 多久没聊天才主动搭话（分钟，最少 30）
    pub proactive_minutes: u32,
    /// 每天最多主动搭话几次
    pub proactive_daily_max: u32,
}

impl Default for DailySettings {
    fn default() -> Self {
        Self {
            autostart: false,
            hotkey_enabled: true,
            hotkey: DEFAULT_HOTKEY.into(),
            hide_in_fullscreen: true,
            sleep_minutes: 10,
            proactive: false,
            proactive_minutes: 90,
            proactive_daily_max: 3,
        }
    }
}

impl DailySettings {
    pub fn sanitize(mut self) -> Self {
        self.sleep_minutes = self.sleep_minutes.min(240);
        self.proactive_minutes = self.proactive_minutes.clamp(30, 24 * 60);
        self.proactive_daily_max = self.proactive_daily_max.clamp(1, 10);
        if parse_hotkey(&self.hotkey).is_err() {
            self.hotkey = DEFAULT_HOTKEY.into();
        }
        self
    }
}

/// 解析快捷键；必须带 Ctrl / Alt / Shift / Win 之一，避免占用单个按键
pub fn parse_hotkey(s: &str) -> Result<Shortcut, String> {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    let has_mod = [
        "ctrl", "control", "alt", "shift", "super", "win", "cmd", "command",
    ]
    .iter()
    .any(|m| lower.split('+').any(|p| p.trim() == *m));
    if !has_mod {
        return Err("快捷键需要包含 Ctrl、Alt、Shift 或 Win 中的至少一个".into());
    }
    s.parse::<Shortcut>()
        .map_err(|e| format!("快捷键格式不对：{e}"))
}

/// 运行时状态
#[derive(Default)]
pub struct DailyState {
    /// 上次和角色互动（聊天、点击）的时间
    pub last_interaction: AtomicI64,
    /// 因为全屏而被自动隐藏
    hidden_for_fullscreen: AtomicBool,
    fullscreen: AtomicBool,
    sleeping: AtomicBool,
    /// 当前注册的快捷键
    hotkey: Mutex<Option<Shortcut>>,
    /// 「看一眼」的快捷键（第三层打开时才注册）
    vision_hotkey: Mutex<Option<Shortcut>>,
    /// 今天主动搭话的次数（日期，次数）
    proactive_count: Mutex<(String, u32)>,
}

impl DailyState {
    pub fn touch(&self) {
        self.last_interaction
            .store(memory::now_secs(), Ordering::Relaxed);
    }
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen.load(Ordering::Relaxed)
    }
    pub fn is_sleeping(&self) -> bool {
        self.sleeping.load(Ordering::Relaxed)
    }
}

/// 用户和角色互动（点击、发消息、按快捷键）：记下时间；睡着的话马上醒来
pub fn interact(app: &AppHandle) {
    let st = app.state::<AppState>();
    st.daily.touch();
    st.proactive.engaged();
    // 每天第一次来：亲密度 +3（每天只算一次）
    crate::companion::interact(app, crate::companion::Interaction::FirstToday);
    if st.daily.sleeping.swap(false, Ordering::Relaxed) {
        chat::emit_pet_state(app, PetState::Idle);
    }
}

// ---------------- Windows 系统查询 ----------------

/// 有全屏程序在前台（游戏、全屏视频、演示模式）
#[cfg(windows)]
pub fn fullscreen_active() -> bool {
    use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
    let mut state = 0;
    // QUNS_BUSY = 2（全屏程序），QUNS_RUNNING_D3D_FULL_SCREEN = 3，QUNS_PRESENTATION_MODE = 4
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    hr >= 0 && matches!(state, 2..=4)
}

#[cfg(not(windows))]
pub fn fullscreen_active() -> bool {
    false
}

/// 系统多久没有键盘鼠标输入了（秒）
#[cfg(windows)]
pub fn system_idle_secs() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return 0;
    }
    let now = unsafe { GetTickCount() };
    (now.wrapping_sub(info.dwTime) / 1000) as u64
}

#[cfg(not(windows))]
pub fn system_idle_secs() -> u64 {
    0
}

/// 显示窗口但不抢走键盘焦点（主动搭话、任务完成提示时用）
pub fn show_no_activate(w: &tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};
        if let Ok(hwnd) = w.hwnd() {
            unsafe {
                ShowWindow(hwnd.0 as _, SW_SHOWNOACTIVATE);
            }
            return;
        }
    }
    let _ = w.show();
}

/// 不抢焦点地弹出聊天气泡
pub fn pop_chat(app: &AppHandle) {
    if let Some(p) = desktop::pet(app)
        && !p.is_visible().unwrap_or(true)
    {
        show_no_activate(&p);
    }
    if let Ok(c) = desktop::ensure_chat(app) {
        if !c.is_visible().unwrap_or(false) {
            show_no_activate(&c);
        }
        let _ = desktop::position_chat(app);
    }
}

/// 第一次使用时气泡里的引导（只出现一次）
pub fn onboarding_text(chat_backend: &str) -> String {
    let mut t = String::from(
        "第一次见面！点角色就能聊天，右键角色可以换角色、换位置、打开设置；快捷键 Alt+Shift+D 随时叫出聊天框。",
    );
    if chat_backend == "mock" {
        t.push_str("现在是测试模式，到「设置 → 模型」填好 Key 才能真正聊天。");
    }
    t
}

// ---------------- 开机自启与快捷键 ----------------

pub fn apply_autostart(app: &AppHandle, on: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    if crate::paths::test_mode() {
        // 测试实例不能改注册表里的开机启动项（会覆盖正式安装的设置）
        log::info!("测试模式：跳过开机自启设置（{on}）");
        return Ok(());
    }
    let al = app.autolaunch();
    let now = al.is_enabled().unwrap_or(false);
    if on && !now {
        al.enable().map_err(|e| format!("开启开机自启失败：{e}"))?;
    } else if !on && now {
        al.disable().map_err(|e| format!("关闭开机自启失败：{e}"))?;
    }
    Ok(())
}

/// 注册（或取消）全局快捷键。被其他程序占用时返回错误（冲突检测）。
pub fn apply_hotkey(app: &AppHandle, enabled: bool, hotkey: &str) -> Result<(), String> {
    let st = app.state::<AppState>();
    let gs = app.global_shortcut();
    let mut cur = st.daily.hotkey.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(old) = cur.take() {
        let _ = gs.unregister(old);
    }
    if !enabled {
        return Ok(());
    }
    let sc = parse_hotkey(hotkey)?;
    gs.register(sc).map_err(|e| {
        format!("快捷键 {hotkey} 注册失败，可能已经被其他程序占用了，换一个试试（{e}）")
    })?;
    *cur = Some(sc);
    Ok(())
}

/// 注册（或取消）「看一眼」的快捷键
pub fn apply_vision_hotkey(app: &AppHandle, enabled: bool, hotkey: &str) -> Result<(), String> {
    let st = app.state::<AppState>();
    let gs = app.global_shortcut();
    let mut cur = st
        .daily
        .vision_hotkey
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(old) = cur.take() {
        let _ = gs.unregister(old);
    }
    if !enabled {
        return Ok(());
    }
    let sc = parse_hotkey(hotkey)?;
    if st
        .daily
        .hotkey
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        == Some(&sc)
    {
        return Err("「看一眼」的快捷键不能和唤出聊天的快捷键一样".into());
    }
    gs.register(sc).map_err(|e| {
        format!("快捷键 {hotkey} 注册失败，可能已经被其他程序占用了，换一个试试（{e}）")
    })?;
    *cur = Some(sc);
    Ok(())
}

/// 快捷键按下：「看一眼」或者显示/隐藏聊天输入框
pub fn on_hotkey(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    if state != ShortcutState::Pressed {
        return;
    }
    let is_vision = app
        .state::<AppState>()
        .daily
        .vision_hotkey
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        == Some(shortcut);
    if is_vision {
        interact(app);
        crate::vision::on_hotkey(app);
        return;
    }
    if let Some(p) = desktop::pet(app)
        && !p.is_visible().unwrap_or(true)
    {
        let _ = desktop::show_window(&p);
    }
    interact(app);
    // 快捷键回调在主线程：放到后台执行，避免创建聊天窗口时死锁
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = desktop::toggle_chat(&app);
    });
}

// ---------------- 后台循环 ----------------

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick: u64 = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            tick += 1;
            check_fullscreen(&app);
            // 主动行为：队列里有待说的话才做事（队列空时只是加一次锁）
            crate::proactive::tick(&app);
            // 睡着时每 3 秒看一次用户回来没有（只是一次系统调用），醒得及时；平时 30 秒查一次
            if tick % 10 == 0 || app.state::<AppState>().daily.is_sleeping() {
                check_sleep(&app);
            }
            if tick % 10 == 0 {
                check_proactive(&app).await;
                // 每日播报：每天第一次、用户设定的时间点
                crate::briefing::check(&app);
                // 日常关怀：久坐、喝水、深夜、吃饭
                crate::care::check(&app);
                // 随机小事件
                crate::companion::check_event(&app);
            }
        }
    });
}

fn check_fullscreen(app: &AppHandle) {
    let st = app.state::<AppState>();
    let s = st.settings().daily;
    let fs = fullscreen_active();
    let was = st.daily.fullscreen.swap(fs, Ordering::Relaxed);
    if fs == was {
        return;
    }
    if fs && s.hide_in_fullscreen {
        if desktop::pet(app)
            .and_then(|p| p.is_visible().ok())
            .unwrap_or(false)
        {
            st.daily
                .hidden_for_fullscreen
                .store(true, Ordering::Relaxed);
            let _ = desktop::hide_chat(app);
            if let Some(p) = desktop::pet(app) {
                let _ = desktop::hide_window(&p);
            }
            log::info!("检测到全屏程序，已自动隐藏");
        }
    } else if !fs
        && st
            .daily
            .hidden_for_fullscreen
            .swap(false, Ordering::Relaxed)
    {
        if let Some(p) = desktop::pet(app) {
            show_no_activate(&p);
        }
        log::info!("全屏程序已退出，恢复显示");
    }
}

/// 睡觉 / 醒来的判断（纯函数，便于测试）：Some(true) = 该睡了，Some(false) = 该醒了，None = 不变
pub fn sleep_change(
    sleep_minutes: u32,
    sleeping: bool,
    idle_secs: u64,
    generating: bool,
) -> Option<bool> {
    if sleep_minutes > 0 && !sleeping && idle_secs >= sleep_minutes as u64 * 60 && !generating {
        Some(true)
    } else if sleeping && (idle_secs < 10 || sleep_minutes == 0) {
        // 10 秒内动过键盘鼠标 = 用户回来了
        Some(false)
    } else {
        None
    }
}

fn check_sleep(app: &AppHandle) {
    let st = app.state::<AppState>();
    let change = sleep_change(
        st.settings().daily.sleep_minutes,
        st.daily.is_sleeping(),
        system_idle_secs(),
        st.chat.is_generating(),
    );
    if let Some(sleep) = change {
        st.daily.sleeping.store(sleep, Ordering::Relaxed);
        chat::emit_pet_state(
            app,
            if sleep {
                PetState::Sleep
            } else {
                PetState::Idle
            },
        );
    }
}

/// 主动搭话的条件是否满足（纯函数，便于测试）
#[allow(clippy::too_many_arguments)]
pub fn should_greet(
    s: &DailySettings,
    dnd: bool,
    fullscreen: bool,
    chat_visible: bool,
    generating: bool,
    now: i64,
    last_interaction: i64,
    system_idle_secs: u64,
    count_today: u32,
) -> bool {
    s.proactive
        && !dnd
        && !fullscreen
        && !chat_visible
        && !generating
        // 用户正在电脑前（最近 2 分钟有操作），但很久没和角色说话
        && system_idle_secs < 120
        && now - last_interaction >= s.proactive_minutes as i64 * 60
        && count_today < s.proactive_daily_max
}

async fn check_proactive(app: &AppHandle) {
    let st = app.state::<AppState>();
    let settings = st.settings();
    let (day, _) = usage::today();
    let count = {
        let mut c = st
            .daily
            .proactive_count
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if c.0 != day {
            *c = (day.clone(), 0);
        }
        c.1
    };
    let chat_visible = desktop::chat(app)
        .and_then(|c| c.is_visible().ok())
        .unwrap_or(false);
    let now = memory::now_secs();
    let mut last = st.daily.last_interaction.load(Ordering::Relaxed);
    if last == 0 {
        // 刚启动：从现在开始计时，不会一启动就搭话
        st.daily.touch();
        last = now;
    }
    if !should_greet(
        &settings.daily,
        settings.dnd,
        st.daily.is_fullscreen(),
        chat_visible,
        st.chat.is_generating(),
        now,
        last,
        system_idle_secs(),
        count,
    ) {
        return;
    }
    st.daily.touch();
    st.daily
        .proactive_count
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .1 += 1;
    let text = greeting_text(&st).await;
    if text.trim().is_empty() {
        return;
    }
    log::info!("很久没聊天，排队打个招呼（今天第 {} 次）", count + 1);
    // 交给调度器：冷却、安静时段、被忽略降频都在那里统一处理
    let mut item = crate::proactive::Item::new(crate::proactive::Kind::Random, text, now + 120);
    item.key = "greeting".into();
    crate::proactive::submit(app, item);
}

/// 主动搭话的内容：有可用模型时让角色说一句（很短、很便宜）；否则用角色的固定开场白
async fn greeting_text(st: &AppState) -> String {
    let c = st.current_character();
    let fallback = c
        .as_ref()
        .map(|c| {
            crate::character::pick_line(
                &c.manifest.lines.greeting,
                "在忙吗？要不要休息一下～",
                memory::now_secs() as u64,
            )
        })
        .unwrap_or_else(|| "在忙吗？要不要休息一下～".into());
    let Some(c) = c else { return fallback };
    let s = st.settings();
    if s.chat_backend == "mock" {
        return fallback;
    }
    // 主动行为有自己的每日预算（次数、花费），用完就只用固定台词
    {
        let (day, month) = usage::today();
        if !crate::proactive::model_allowed(&st.db(), &s.companion, &s.budget, &day, &month) {
            return fallback;
        }
    }
    let prefs = s.effective_prefs(&c.manifest.id);
    let turn = chat::prepare_turn(st, &c.manifest.id, "（很久没聊天了）", memory::now_secs(), vec![
        "现在是你主动和用户打招呼：用户已经很久没和你说话了。请用一两句话自然地打个招呼，可以关心一下用户最近在做的事；不要提「程序」「提示」之类的词。".into(),
    ]);
    let now_text = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts = crate::prompt::PromptParts {
        character_name: &c.manifest.name,
        persona: &c.persona,
        user_rules: &prefs.custom_rules,
        banned_words: &prefs.banned_words,
        profile_facts: &turn.profile,
        canon: &[],
        // 常驻台词和聊天时一样带上：语气一致，系统提示词也和聊天时相同（提示缓存能命中）
        quotes_fixed: &c.quotes.fixed,
        quotes: &[],
        memories: &[],
        notes: &turn.notes,
        history: &[],
        user_text: "（很久没聊天了）",
        now: &now_text,
        delegation: false,
        relation: "",
    };
    let budget = crate::prompt::PromptBudget {
        max_output_tokens: 120,
        ..Default::default()
    };
    let req = crate::prompt::build(&parts, budget);
    match tokio::time::timeout(
        Duration::from_secs(30),
        chat::complete(
            st,
            &s.chat_backend,
            req,
            usage::purpose::PROACTIVE,
            CancellationToken::new(),
        ),
    )
    .await
    {
        Ok(Ok(out)) => {
            let banned = crate::filter::BannedWords::new(&prefs.banned_words);
            let (text, _) = crate::markers::strip(&banned.apply(&out.text));
            if text.trim().is_empty() {
                fallback
            } else {
                text
            }
        }
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboarding_mentions_key_only_in_mock_mode() {
        assert!(onboarding_text("mock").contains("填好 Key"));
        assert!(!onboarding_text("deepseek").contains("Key"));
        assert!(onboarding_text("deepseek").contains("右键"));
    }

    #[test]
    fn sleeps_after_idle_and_wakes_when_user_returns() {
        assert_eq!(sleep_change(10, false, 599, false), None);
        assert_eq!(sleep_change(10, false, 600, false), Some(true));
        assert_eq!(sleep_change(10, false, 600, true), None, "正在回复时不睡");
        assert_eq!(sleep_change(0, false, 99_999, false), None, "设成不睡");
        assert_eq!(sleep_change(10, true, 700, false), None, "还没人动");
        assert_eq!(
            sleep_change(10, true, 2, false),
            Some(false),
            "动了鼠标就醒"
        );
        assert_eq!(
            sleep_change(0, true, 700, false),
            Some(false),
            "关掉睡觉后醒来"
        );
    }

    #[test]
    fn hotkey_needs_modifier() {
        assert!(parse_hotkey("Alt+Shift+D").is_ok());
        assert!(parse_hotkey("ctrl+alt+KeyK").is_ok());
        assert!(parse_hotkey("D").is_err());
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("Alt+Shift+NotAKey").is_err());
    }

    #[test]
    fn settings_sanitize() {
        let s = DailySettings {
            proactive_minutes: 1,
            proactive_daily_max: 99,
            hotkey: "D".into(),
            ..Default::default()
        }
        .sanitize();
        assert_eq!(s.proactive_minutes, 30);
        assert_eq!(s.proactive_daily_max, 10);
        assert_eq!(s.hotkey, DEFAULT_HOTKEY);
        assert!(!DailySettings::default().autostart, "开机自启默认关闭");
        assert!(!DailySettings::default().proactive, "主动搭话默认关闭");
    }

    #[test]
    fn proactive_conditions() {
        let on = DailySettings {
            proactive: true,
            proactive_minutes: 60,
            proactive_daily_max: 2,
            ..Default::default()
        };
        let now = 100_000;
        let long_ago = now - 2 * 3600;
        // 条件都满足
        assert!(should_greet(
            &on, false, false, false, false, now, long_ago, 10, 0
        ));
        // 默认关闭
        assert!(!should_greet(
            &DailySettings::default(),
            false,
            false,
            false,
            false,
            now,
            long_ago,
            10,
            0
        ));
        // 勿扰、全屏、聊天窗口开着、正在生成时都不打扰
        assert!(!should_greet(
            &on, true, false, false, false, now, long_ago, 10, 0
        ));
        assert!(!should_greet(
            &on, false, true, false, false, now, long_ago, 10, 0
        ));
        assert!(!should_greet(
            &on, false, false, true, false, now, long_ago, 10, 0
        ));
        assert!(!should_greet(
            &on, false, false, false, true, now, long_ago, 10, 0
        ));
        // 最近刚聊过
        assert!(!should_greet(
            &on,
            false,
            false,
            false,
            false,
            now,
            now - 600,
            10,
            0
        ));
        // 用户不在电脑前
        assert!(!should_greet(
            &on, false, false, false, false, now, long_ago, 3600, 0
        ));
        // 每天次数上限
        assert!(!should_greet(
            &on, false, false, false, false, now, long_ago, 10, 2
        ));
    }
}
