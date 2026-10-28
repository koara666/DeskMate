//! 设置窗口用的命令。Key 只进不出：前端只能写入或清除，读到的永远只是末 4 位。

use crate::backends::{self, BackendConfig, BackendError, ChatMessage, ChatRequest, SystemBlock};
use crate::chat;
use crate::desktop;
use crate::redact::{mask_key, redact_secrets};
use crate::secrets::normalize_key;
use crate::state::AppState;
use crate::usage::{self, Budget};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use tokio_util::sync::CancellationToken;

type CmdResult<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    pub always_on_top: bool,
    pub pet_scale: f64,
    pub idle_animation: bool,
    pub dnd: bool,
    /// 显卡加速（重启后生效）
    pub gpu_acceleration: bool,
}

#[derive(Serialize)]
pub struct BackendView {
    pub config: BackendConfig,
    pub kind_name: String,
    pub needs_key: bool,
    pub key_url: String,
    pub note: String,
    /// 已保存 Key 的末 4 位（没有保存时为 None）
    pub key_mask: Option<String>,
    /// 内置的默认配置不能删除
    pub removable: bool,
}

#[derive(Serialize)]
pub struct KindView {
    pub kind: String,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct SettingsView {
    pub general: General,
    pub backends: Vec<BackendView>,
    pub kinds: Vec<KindView>,
    pub chat_backend: String,
    pub background_backend: String,
    pub budget: Budget,
    pub data_dir: String,
    pub version: String,
    /// 打开设置窗口时要直接显示的页面（只生效一次）
    pub initial_tab: Option<String>,
    pub daily: crate::daily::DailySettings,
    pub companion: crate::proactive::CompanionSettings,
    pub advanced: crate::abilities::AdvancedSettings,
    pub briefing: crate::briefing::BriefingSettings,
    pub activity: crate::activity::ActivitySettings,
    pub care: crate::care::CareSettings,
    pub vision: crate::vision::VisionSettings,
    pub update: crate::update::UpdateSettings,
}

fn is_builtin_id(id: &str) -> bool {
    backends::kinds().iter().any(|k| k.kind == id)
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> SettingsView {
    let s = state.settings();
    let backends = s
        .backends
        .iter()
        .map(|c| {
            let k = backends::find_kind(&c.kind);
            let key_mask = if k.is_some_and(|k| k.needs_key) {
                state
                    .secrets
                    .get(&c.id)
                    .ok()
                    .flatten()
                    .map(|key| mask_key(&key))
            } else {
                None
            };
            BackendView {
                config: c.clone(),
                kind_name: k.map(|k| k.display_name.to_string()).unwrap_or_default(),
                needs_key: k.is_some_and(|k| k.needs_key),
                key_url: k.map(|k| k.key_url.to_string()).unwrap_or_default(),
                note: k.map(|k| k.note.to_string()).unwrap_or_default(),
                key_mask,
                removable: !is_builtin_id(&c.id),
            }
        })
        .collect();
    SettingsView {
        general: General {
            always_on_top: s.always_on_top,
            pet_scale: s.pet_scale,
            idle_animation: s.idle_animation,
            dnd: s.dnd,
            gpu_acceleration: s.gpu_acceleration,
        },
        backends,
        kinds: backends::kinds()
            .iter()
            .filter(|k| k.kind != "mock")
            .map(|k| KindView {
                kind: k.kind.to_string(),
                display_name: k.display_name.to_string(),
            })
            .collect(),
        chat_backend: s.chat_backend,
        background_backend: s.background_backend,
        budget: s.budget,
        data_dir: state.paths.root().display().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        initial_tab: state.desktop.take_settings_tab(),
        daily: s.daily.clone(),
        companion: s.companion.clone(),
        advanced: s.advanced.clone(),
        briefing: s.briefing.clone(),
        activity: s.activity.clone(),
        care: s.care.clone(),
        vision: s.vision.clone(),
        update: s.update.clone(),
    }
}

#[tauri::command]
pub fn settings_save_general(
    app: AppHandle,
    state: State<'_, AppState>,
    general: General,
) -> CmdResult<()> {
    let before = state.settings();
    let after = state.update_settings(|s| {
        s.pet_scale = general.pet_scale;
        s.idle_animation = general.idle_animation;
        s.dnd = general.dnd;
        s.gpu_acceleration = general.gpu_acceleration;
    });
    if before.always_on_top != general.always_on_top {
        desktop::set_always_on_top(&app, general.always_on_top).map_err(|e| e.to_string())?;
    }
    if (before.pet_scale - after.pet_scale).abs() > f64::EPSILON {
        desktop::apply_pet_scale(&app).map_err(|e| e.to_string())?;
    }
    desktop::broadcast_settings_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn backend_save(
    app: AppHandle,
    state: State<'_, AppState>,
    config: BackendConfig,
) -> CmdResult<()> {
    config.validate()?;
    let mut found = false;
    state.update_settings(|s| {
        if let Some(b) = s.backends.iter_mut().find(|b| b.id == config.id) {
            // 类型不允许改（Key 的含义会变）
            let kind = b.kind.clone();
            *b = config.clone();
            b.kind = kind;
            found = true;
        }
    });
    if !found {
        return Err("找不到这个模型配置".into());
    }
    desktop::broadcast_settings_changed(&app);
    Ok(())
}

/// 新增一个配置（比如第二个 OpenAI 兼容服务），返回新 id
#[tauri::command]
pub fn backend_add(app: AppHandle, state: State<'_, AppState>, kind: String) -> CmdResult<String> {
    let k = backends::find_kind(&kind).ok_or("未知的后端类型")?;
    let existing: Vec<String> = state
        .settings()
        .backends
        .iter()
        .map(|b| b.id.clone())
        .collect();
    if existing.len() >= 30 {
        return Err("模型配置太多了".into());
    }
    let id = (2..100)
        .map(|n| format!("{kind}-{n}"))
        .find(|id| !existing.contains(id))
        .ok_or("无法生成新的配置 id")?;
    let mut cfg = BackendConfig::from_kind(k);
    cfg.id = id.clone();
    cfg.name = format!("{} {}", k.display_name, id.rsplit('-').next().unwrap_or(""));
    state.update_settings(|s| s.backends.push(cfg));
    desktop::broadcast_settings_changed(&app);
    Ok(id)
}

#[tauri::command]
pub fn backend_delete(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if is_builtin_id(&id) {
        return Err("内置的模型配置不能删除（可以清除 Key）".into());
    }
    let _ = state.secrets.delete(&id);
    state.update_settings(|s| s.backends.retain(|b| b.id != id));
    desktop::broadcast_settings_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn backend_set_key(state: State<'_, AppState>, id: String, key: String) -> CmdResult<String> {
    if state.settings().backend(&id).is_none() {
        return Err("找不到这个模型配置".into());
    }
    let key = normalize_key(&key)?;
    state.secrets.set(&id, &key)?;
    log::info!("已保存模型配置 {id} 的 Key");
    Ok(mask_key(&key))
}

#[tauri::command]
pub fn backend_clear_key(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.secrets.delete(&id)?;
    log::info!("已删除模型配置 {id} 的 Key");
    Ok(())
}

#[tauri::command]
pub fn backend_select(
    app: AppHandle,
    state: State<'_, AppState>,
    chat_backend: String,
    background_backend: String,
) -> CmdResult<()> {
    let s = state.settings();
    if s.backend(&chat_backend).is_none() {
        return Err("找不到聊天用的模型配置".into());
    }
    if !background_backend.is_empty() && s.backend(&background_backend).is_none() {
        return Err("找不到后台整理用的模型配置".into());
    }
    state.update_settings(|s| {
        s.chat_backend = chat_backend;
        s.background_backend = background_backend;
    });
    desktop::broadcast_settings_changed(&app);
    Ok(())
}

#[derive(Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub reply: String,
    pub error_kind: String,
    pub error_detail: String,
    pub hint: String,
}

/// 测试连接：发一条极短的请求（会产生极少量真实费用，只在用户点按钮时调用）
#[tauri::command]
pub async fn backend_test(app: AppHandle, id: String) -> TestResult {
    let state = app.state::<AppState>();
    let req = ChatRequest {
        system: vec![SystemBlock {
            text: "这是一次连接测试，请简短回复。".into(),
            cacheable: false,
        }],
        messages: vec![ChatMessage::user("请只回复两个字：你好")],
        context: None,
        max_tokens: 64,
        temperature: None,
    };
    let start = Instant::now();
    let r = tokio::time::timeout(
        Duration::from_secs(40),
        chat::complete(
            &state,
            &id,
            req,
            usage::purpose::TEST,
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap_or(Err(BackendError::Timeout));
    let latency_ms = start.elapsed().as_millis() as u64;
    match r {
        Ok(out) => TestResult {
            ok: true,
            latency_ms,
            reply: out.text.chars().take(60).collect(),
            error_kind: String::new(),
            error_detail: String::new(),
            hint: String::new(),
        },
        Err(e) => TestResult {
            ok: false,
            latency_ms,
            reply: String::new(),
            error_kind: e.kind().into(),
            error_detail: redact_secrets(&e.to_string()),
            hint: e.hint().into(),
        },
    }
}

#[tauri::command]
pub fn budget_save(state: State<'_, AppState>, budget: Budget) -> CmdResult<()> {
    let b = budget.sanitize();
    state.update_settings(|s| s.budget = b);
    Ok(())
}

#[derive(Serialize)]
pub struct UsageView {
    pub today: String,
    pub month: String,
    pub spent_today: f64,
    pub spent_month: f64,
    pub budget: Budget,
    pub rows_today: Vec<usage::Row>,
    pub rows_month: Vec<usage::Row>,
    pub rows_all: Vec<usage::Row>,
}

#[tauri::command]
pub fn usage_get(state: State<'_, AppState>) -> UsageView {
    let (day, month) = usage::today();
    let budget = state.settings().budget;
    let db = state.db();
    UsageView {
        spent_today: usage::spent_day(&db, &day),
        spent_month: usage::spent_month(&db, &month),
        rows_today: usage::summary(&db, "day", Some(&day)),
        rows_month: usage::summary(&db, "month", Some(&month)),
        rows_all: usage::summary(&db, "day", None),
        budget,
        today: day,
        month,
    }
}

/// 在资源管理器里打开数据目录（放角色图片用）
#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>, sub: Option<String>) -> CmdResult<()> {
    let mut dir = state.paths.root().to_path_buf();
    if let Some(sub) = sub {
        // 只允许打开数据目录下的固定子目录
        match sub.as_str() {
            "characters" | "logs" | "backups" => dir = dir.join(sub),
            _ => return Err("不支持的目录".into()),
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 保存日常体验设置：开机自启、快捷键（检测冲突）、全屏、睡眠、主动搭话
#[tauri::command]
pub fn companion_save(
    app: AppHandle,
    state: State<'_, AppState>,
    companion: crate::proactive::CompanionSettings,
) -> CmdResult<()> {
    state.update_settings(|s| s.companion = companion);
    desktop::broadcast_settings_changed(&app);
    Ok(())
}

/// 高级设置：委托外部 agent 开关、允许读取的文件夹（逐个检查）、常用程序
#[tauri::command]
pub fn advanced_save(
    app: AppHandle,
    state: State<'_, AppState>,
    advanced: crate::abilities::AdvancedSettings,
) -> CmdResult<crate::abilities::AdvancedSettings> {
    let mut a = advanced.sanitize();
    let mut dirs = Vec::new();
    for d in &a.read_dirs {
        dirs.push(crate::abilities::validate_read_dir(d, state.paths.root())?);
    }
    a.read_dirs = dirs;
    for p in &a.programs {
        let path = std::path::Path::new(&p.path);
        if !path.is_absolute() || !path.is_file() {
            return Err(format!("找不到程序「{}」：{}", p.name, p.path));
        }
    }
    let saved = state.update_settings(|s| s.advanced = a).advanced;
    desktop::broadcast_settings_changed(&app);
    Ok(saved)
}

/// 每日播报设置
#[tauri::command]
pub fn briefing_save(
    app: AppHandle,
    state: State<'_, AppState>,
    briefing: crate::briefing::BriefingSettings,
) -> CmdResult<crate::briefing::BriefingSettings> {
    for f in &briefing.feeds {
        crate::briefing::news::check_feed_url(&f.url)?;
    }
    let saved = state.update_settings(|s| s.briefing = briefing).briefing;
    desktop::broadcast_settings_changed(&app);
    Ok(saved)
}

/// 找城市：先查内置表（中文名），没有再用 Open-Meteo 地理编码查（用户点「查找」时才联网）
#[tauri::command]
pub async fn city_search(
    app: AppHandle,
    query: String,
) -> CmdResult<Vec<crate::briefing::city::City>> {
    use crate::briefing::city;
    let q: String = query.trim().chars().take(60).collect();
    let local = city::search_local(&q);
    if !local.is_empty() {
        return Ok(local);
    }
    let http = app.state::<AppState>().http.clone();
    let found = city::search_online(&http, &q).await?;
    if found.is_empty() && !q.is_ascii() {
        return Err(
            "没找到。常用城市可以直接输中文；别的城市请输英文或拼音，比如 Lijiang、Sapporo。"
                .into(),
        );
    }
    Ok(found)
}

/// 「现在播报一次」（设置里试听）
#[tauri::command]
pub async fn briefing_now(app: AppHandle) -> CmdResult<()> {
    let hour = chrono::Timelike::hour(&crate::reminders::now_local());
    crate::briefing::run(&app, crate::briefing::Occasion::at(hour), "manual", true).await;
    Ok(())
}

/// 活动感知页：最近用过的程序（只在内存里）、现在在做什么、最近几天的活动汇总
#[derive(Serialize)]
pub struct ActivityOverview {
    pub current: Option<crate::activity::Current>,
    pub recent: Vec<(String, crate::activity::classify::Category)>,
    pub days: Vec<(String, Vec<crate::activity::log::Total>)>,
}

#[tauri::command]
pub fn activity_overview(state: State<'_, AppState>) -> ActivityOverview {
    use crate::activity::log;
    let db = state.db();
    let days = log::recent_days(&db, 7)
        .into_iter()
        .map(|d| {
            let t = log::day_totals(&db, &d);
            (d, t)
        })
        .collect();
    ActivityOverview {
        current: state.activity.current().filter(|c| !c.private),
        recent: state.activity.recent(),
        days,
    }
}

#[tauri::command]
pub fn activity_save(
    app: AppHandle,
    state: State<'_, AppState>,
    activity: crate::activity::ActivitySettings,
) -> CmdResult<crate::activity::ActivitySettings> {
    let saved = state.update_settings(|s| s.activity = activity).activity;
    desktop::broadcast_settings_changed(&app);
    // 马上按新规则看一眼（比如刚把某个程序设成回避）
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { crate::activity::sample(&app2) });
    Ok(saved)
}

/// 删除活动日志：某一天，或者 day = "all" 全部
#[tauri::command]
pub fn activity_delete(state: State<'_, AppState>, day: String) -> CmdResult<usize> {
    use crate::activity::log;
    let db = state.db();
    if day == "all" {
        log::delete_all(&db).map_err(|e| e.to_string())
    } else if chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").is_ok() {
        log::delete_day(&db, &day).map_err(|e| e.to_string())
    } else {
        Err("日期不对".into())
    }
}

#[tauri::command]
pub fn care_save(
    app: AppHandle,
    state: State<'_, AppState>,
    care: crate::care::CareSettings,
) -> CmdResult<crate::care::CareSettings> {
    let saved = state.update_settings(|s| s.care = care).care;
    desktop::broadcast_settings_changed(&app);
    Ok(saved)
}

/// 当前角色的日记（最近 60 篇）
#[tauri::command]
pub fn diary_list(state: State<'_, AppState>) -> Vec<crate::diary::Diary> {
    let id = state
        .current_character()
        .map(|c| c.manifest.id)
        .unwrap_or_default();
    crate::diary::list(&state.db(), &id, 60)
}

#[tauri::command]
pub fn diary_delete(state: State<'_, AppState>, id: i64) -> CmdResult<bool> {
    crate::diary::delete(&state.db(), id).map_err(|e| e.to_string())
}

/// 和当前角色的关系：亲密度等级、积分、离下一级还差多少、心情
#[derive(Serialize)]
pub struct CompanionStatus {
    pub character: String,
    pub level: u8,
    pub points: i64,
    pub next: Option<i64>,
    pub title: String,
    pub mood: i32,
    pub mood_word: String,
    pub nickname: String,
    pub call: String,
}

#[tauri::command]
pub fn companion_status(state: State<'_, AppState>) -> CompanionStatus {
    use crate::companion;
    let c = state.current_character();
    let id = c
        .as_ref()
        .map(|c| c.manifest.id.clone())
        .unwrap_or_default();
    let b = companion::load(&state.db(), &id);
    let now = crate::memory::now_secs();
    let level = b.level();
    let title = c
        .as_ref()
        .and_then(|c| c.manifest.affinity_titles.get(level as usize).cloned())
        .unwrap_or_else(|| companion::DEFAULT_TITLES[level as usize].to_string());
    CompanionStatus {
        character: c.map(|c| c.manifest.name).unwrap_or_default(),
        level,
        points: b.points,
        next: companion::LEVELS.get(level as usize + 1).copied(),
        title,
        mood: b.mood_now(now),
        mood_word: companion::mood_word(b.mood_now(now)).into(),
        nickname: state.settings().nickname,
        call: companion::call_name(&state),
    }
}

#[tauri::command]
pub fn nickname_save(
    app: AppHandle,
    state: State<'_, AppState>,
    nickname: String,
) -> CmdResult<String> {
    let saved = state.update_settings(|s| s.nickname = nickname).nickname;
    desktop::broadcast_settings_changed(&app);
    Ok(saved)
}

/// 「看一眼」设置：检查模型能不能看图、注册快捷键
#[tauri::command]
pub fn vision_save(
    app: AppHandle,
    state: State<'_, AppState>,
    vision: crate::vision::VisionSettings,
) -> CmdResult<crate::vision::VisionSettings> {
    let v = vision.sanitize();
    if v.enabled {
        let s = state.settings();
        let cfg = s.backend(&v.backend).ok_or("先选一个看图用的模型")?;
        if !crate::vision::kind_supports_images(&cfg.kind) {
            return Err(
                "这个模型不能看图，换成 Claude、Gemini 或支持图片的 OpenAI 兼容模型".into(),
            );
        }
    }
    crate::daily::apply_vision_hotkey(&app, v.enabled, &v.hotkey)?;
    let saved = state.update_settings(|s| s.vision = v).vision;
    desktop::broadcast_settings_changed(&app);
    Ok(saved)
}

/// 「看一眼」累计的（次数，花费美元）
#[tauri::command]
pub fn vision_cost(state: State<'_, AppState>) -> (i64, f64) {
    crate::usage::purpose_total(&state.db(), crate::usage::purpose::VISION)
}

/// 「预览她会看到什么」：和按快捷键时一样截图、做隐私处理，结果只给设置窗口看（不发送、不保存）
#[tauri::command]
pub async fn vision_preview(state: State<'_, AppState>) -> CmdResult<crate::vision::PreviewView> {
    Ok(crate::vision::preview(&state).await)
}

/// 本次运行里看过的记录（不存图、不存标题）
#[tauri::command]
pub fn vision_records(state: State<'_, AppState>) -> Vec<crate::vision::LookRecord> {
    state.vision.records()
}

/// 「检查更新」按钮：返回 Some = 有新版本
#[tauri::command]
pub async fn update_check(
    state: State<'_, AppState>,
) -> CmdResult<Option<crate::update::UpdateView>> {
    crate::update::check_now(&state).await
}

/// 启动时自动检查更新的开关
#[tauri::command]
pub fn update_save(
    app: AppHandle,
    state: State<'_, AppState>,
    auto_check: bool,
) -> CmdResult<crate::update::UpdateSettings> {
    let saved = state
        .update_settings(|s| s.update.auto_check = auto_check)
        .update;
    desktop::broadcast_settings_changed(&app);
    Ok(saved)
}

#[tauri::command]
pub fn reminder_list(state: State<'_, AppState>) -> CmdResult<Vec<crate::reminders::ReminderView>> {
    let now = crate::reminders::now_local();
    Ok(crate::reminders::list(&state.db())
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| r.view(now))
        .collect())
}

#[tauri::command]
pub fn reminder_delete(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    crate::reminders::delete(&state.db(), id).map_err(|e| e.to_string())?;
    crate::reminders::notify_changed(&app);
    Ok(())
}

/// 暂停 / 恢复。恢复时如果定时提醒的时间已经过了：重复的算到下一次，不重复的提示改时间
#[tauri::command]
pub fn reminder_set_status(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    status: String,
) -> CmdResult<()> {
    use crate::reminders;
    if !matches!(status.as_str(), "active" | "paused") {
        return Err("状态不对".into());
    }
    let r = reminders::get(&state.db(), id)
        .map_err(|e| e.to_string())?
        .ok_or("找不到这个提醒")?;
    let r = reminders::Reminder { status, ..r }.sanitize(crate::memory::now_secs())?;
    reminders::save(&state.db(), &r, 0).map_err(|e| e.to_string())?;
    reminders::notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn daily_save(
    app: AppHandle,
    state: State<'_, AppState>,
    daily: crate::daily::DailySettings,
) -> CmdResult<()> {
    if daily.hotkey_enabled {
        crate::daily::parse_hotkey(&daily.hotkey)?;
    }
    let d = daily.sanitize();
    crate::daily::apply_autostart(&app, d.autostart)?;
    let hotkey_result = crate::daily::apply_hotkey(&app, d.hotkey_enabled, &d.hotkey);
    state.update_settings(|s| s.daily = d);
    desktop::broadcast_settings_changed(&app);
    hotkey_result
}
