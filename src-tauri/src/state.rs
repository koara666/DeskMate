//! 全局应用状态（通过 `app.state::<AppState>()` 访问）

use crate::character::{self, Character};
use crate::chat::ChatState;
use crate::desktop::DesktopState;
use crate::paths::DataPaths;
use crate::secrets::SecretStore;
use crate::settings::{self, Settings};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, RwLock};

pub struct AppState {
    pub paths: DataPaths,
    db: Mutex<Connection>,
    settings: RwLock<Settings>,
    characters: RwLock<Vec<Character>>,
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
        Self {
            paths,
            db: Mutex::new(conn),
            settings: RwLock::new(settings),
            characters: RwLock::new(characters),
            builtin_characters_dir,
            chat: ChatState::default(),
            desktop: DesktopState::default(),
            http: crate::backends::http::client(),
            secrets,
            notice: Mutex::new(None),
            tasks: Default::default(),
            daily: Default::default(),
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
