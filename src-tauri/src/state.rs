//! 全局应用状态（通过 `app.state::<AppState>()` 访问）

use crate::character::{self, Character};
use crate::chat::ChatState;
use crate::desktop::DesktopState;
use crate::lines::{self, LineCtx, Lines};
use crate::paths::DataPaths;
use crate::secrets::SecretStore;
use crate::settings::{self, Settings};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

pub struct AppState {
    pub paths: DataPaths,
    db: Mutex<Connection>,
    settings: RwLock<Settings>,
    characters: RwLock<Vec<Character>>,
    /// 通用台词（角色没写的场景用它）
    common_lines: RwLock<Arc<Lines>>,
    /// 每个场景最近说过的台词（尽量不重复）
    pub recent_lines: crate::lines::Recent,
    pub builtin_characters_dir: Option<PathBuf>,
    pub chat: ChatState,
    pub desktop: DesktopState,
    /// 共用的 HTTP 客户端
    pub http: reqwest::Client,
    /// API Key 存储（系统凭据管理器）
    pub secrets: Box<dyn SecretStore>,
    /// 下次打开聊天窗口时要告诉用户的一句话（比如数据库从备份恢复了）
    notice: Mutex<Option<String>>,
    /// 正在执行 / 等待批准的任务
    pub tasks: crate::task_cmds::TaskState,
    /// 日常体验：互动时间、全屏、睡眠、快捷键
    pub daily: crate::daily::DailyState,
    /// 主动行为调度器的队列和节奏
    pub proactive: crate::proactive::ProactiveState,
    /// 提醒定时器
    pub reminders: crate::reminders::ReminderState,
    /// 每日播报：天气缓存、上次播报时间
    pub briefing: crate::briefing::BriefingState,
    /// 活动感知：现在在做什么、会话、最近用过的程序
    pub activity: crate::activity::ActivityState,
    /// 日常关怀的计时
    pub care: crate::care::CareRuntime,
}

impl AppState {
    pub fn new(
        paths: DataPaths,
        conn: Connection,
        builtin_characters_dir: Option<PathBuf>,
        secrets: Box<dyn SecretStore>,
    ) -> Self {
        let settings = settings::load(&conn);
        let characters =
            character::load_all(builtin_characters_dir.as_deref(), &paths.characters());
        let common_lines = character::load_common_lines(builtin_characters_dir.as_deref());
        Self {
            paths,
            db: Mutex::new(conn),
            settings: RwLock::new(settings),
            characters: RwLock::new(characters),
            common_lines: RwLock::new(common_lines),
            recent_lines: Default::default(),
            builtin_characters_dir,
            chat: ChatState::default(),
            desktop: DesktopState::default(),
            http: crate::backends::http::client(),
            secrets,
            notice: Mutex::new(None),
            tasks: Default::default(),
            daily: Default::default(),
            proactive: Default::default(),
            reminders: Default::default(),
            briefing: Default::default(),
            activity: Default::default(),
            care: Default::default(),
        }
    }

    /// 数据库连接。锁住的时间要尽量短，不要跨 await 持有。
    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 修改设置并立即保存
    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Settings {
        let mut guard = self.settings.write().unwrap_or_else(|e| e.into_inner());
        let mut s = guard.clone();
        f(&mut s);
        let s = s.sanitize();
        if let Err(e) = settings::save(&self.db(), &s) {
            log::error!("保存设置失败：{e}");
        }
        *guard = s.clone();
        s
    }

    pub fn characters(&self) -> Vec<Character> {
        self.characters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_notice(&self, text: String) {
        *self.notice.lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
    }

    pub fn take_notice(&self) -> Option<String> {
        self.notice.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    pub fn reload_characters(&self) {
        let list = character::load_all(
            self.builtin_characters_dir.as_deref(),
            &self.paths.characters(),
        );
        *self.characters.write().unwrap_or_else(|e| e.into_inner()) = list;
        *self.common_lines.write().unwrap_or_else(|e| e.into_inner()) =
            character::load_common_lines(self.builtin_characters_dir.as_deref());
    }

    pub fn common_lines(&self) -> Arc<Lines> {
        self.common_lines
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 从当前角色的台词池（没有就用通用台词）里挑一句，填好占位符。挑不到返回 None。
    pub fn line(&self, scene: &str, vars: &[(&str, String)]) -> Option<String> {
        use chrono::Timelike;
        let own = self
            .current_character()
            .map(|c| c.pool.clone())
            .unwrap_or_default();
        let ctx = LineCtx {
            affinity: 0,
            mood: 0,
            hour: chrono::Local::now().hour(),
            vars,
        };
        lines::pick(
            &own,
            &self.common_lines(),
            scene,
            &ctx,
            &self.recent_lines,
            lines::seed(),
        )
    }

    /// 按 id 找角色
    pub fn character(&self, id: &str) -> Option<Character> {
        self.characters
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|c| c.manifest.id == id)
            .cloned()
    }

    /// 当前角色；设置里的 id 不存在时用第一个可用角色
    pub fn current_character(&self) -> Option<Character> {
        let id = self.settings().character_id;
        let list = self.characters.read().unwrap_or_else(|e| e.into_inner());
        list.iter()
            .find(|c| c.manifest.id == id)
            .or_else(|| list.first())
            .cloned()
    }
}

/// 测试用：临时目录里的完整状态（内置角色包、mock 后端、内存里的凭据）
#[cfg(test)]
pub fn test_state(dir: &std::path::Path) -> AppState {
    let paths = DataPaths::from_root(dir);
    paths.ensure().unwrap();
    let conn = crate::db::open(&paths.db(), &paths.backups()).unwrap();
    let builtin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../characters");
    AppState::new(
        paths,
        conn,
        Some(builtin),
        Box::new(crate::secrets::MemoryStore::default()),
    )
}
