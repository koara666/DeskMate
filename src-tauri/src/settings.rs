//! 设置：整个结构以 JSON 存在 `settings` 表的 `app` 键下（不含任何密钥）。
//! 新增字段一律带默认值（`#[serde(default)]`），老数据读出来自动补齐。

use crate::backends::{self, BackendConfig};
use crate::usage::Budget;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

/// 上次桌宠位置（物理像素，左上角）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 当前角色 id
    pub character_id: String,
    /// 总在最前
    pub always_on_top: bool,
    /// 桌宠缩放（0.6 ~ 2.0）
    pub pet_scale: f64,
    /// 待机小动画（呼吸、眨眼）
    pub idle_animation: bool,
    /// 勿扰模式
    pub dnd: bool,
    /// 上次位置；None 表示使用默认角落
    pub pet_position: Option<SavedPosition>,
    /// 默认停靠角落
    pub corner: Corner,
    /// 模型配置列表（不含 Key）
    pub backends: Vec<BackendConfig>,
    /// 日常聊天用哪个配置（id）
    pub chat_backend: String,
    /// 后台记忆整理用哪个配置（空 = 和聊天相同）。建议选最便宜的。
    pub background_backend: String,
    /// 花费上限
    pub budget: Budget,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            character_id: "deepseek-whale".into(),
            always_on_top: true,
            pet_scale: 1.0,
            idle_animation: true,
            dnd: false,
            pet_position: None,
            corner: Corner::BottomRight,
            backends: backends::default_configs(),
            chat_backend: "mock".into(),
            background_backend: String::new(),
            budget: Budget::default(),
        }
    }
}

impl Settings {
    /// 把用户可能填错的值收敛到合理范围
    pub fn sanitize(mut self) -> Self {
        if !self.pet_scale.is_finite() {
            self.pet_scale = 1.0;
        }
        self.pet_scale = self.pet_scale.clamp(0.6, 2.0);
        self.budget = self.budget.sanitize();
        // 丢掉无效配置；新版本新增的后端类型自动补上默认配置
        let mut seen = std::collections::HashSet::new();
        self.backends
            .retain(|b| b.validate().is_ok() && seen.insert(b.id.clone()));
        for d in backends::default_configs() {
            if !self.backends.iter().any(|b| b.id == d.id) {
                self.backends.push(d);
            }
        }
        if !self.backends.iter().any(|b| b.id == self.chat_backend) {
            self.chat_backend = "mock".into();
        }
        if !self.background_backend.is_empty()
            && !self
                .backends
                .iter()
                .any(|b| b.id == self.background_backend)
        {
            self.background_backend = String::new();
        }
        self
    }

    pub fn backend(&self, id: &str) -> Option<&BackendConfig> {
        self.backends.iter().find(|b| b.id == id)
    }
}

const KEY: &str = "app";

pub fn load(conn: &Connection) -> Settings {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .unwrap_or_else(|e| {
            log::warn!("读取设置失败，使用默认设置：{e}");
            None
        });
    match raw {
        Some(s) => serde_json::from_str::<Settings>(&s)
            .unwrap_or_else(|e| {
                log::warn!("设置格式错误，使用默认设置：{e}");
                Settings::default()
            })
            .sanitize(),
        None => Settings::default(),
    }
}

pub fn save(conn: &Connection, s: &Settings) -> rusqlite::Result<()> {
    let json = serde_json::to_string(s).expect("Settings 一定能序列化");
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [KEY, json.as_str()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn default_when_empty() {
        let conn = db::open_in_memory().unwrap();
        assert_eq!(load(&conn), Settings::default());
    }

    #[test]
    fn roundtrip() {
        let conn = db::open_in_memory().unwrap();
        let s = Settings {
            dnd: true,
            pet_position: Some(SavedPosition { x: -100, y: 20 }),
            ..Default::default()
        };
        save(&conn, &s).unwrap();
        assert_eq!(load(&conn), s);
    }

    #[test]
    fn missing_fields_get_defaults_and_values_are_clamped() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('app', '{\"pet_scale\": 9.5}')",
            [],
        )
        .unwrap();
        let s = load(&conn);
        assert_eq!(s.pet_scale, 2.0);
        assert_eq!(s.character_id, "deepseek-whale");
    }

    #[test]
    fn invalid_backends_are_dropped_and_defaults_restored() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('app', '{\"backends\": [{\"id\": \"deepseek\", \"kind\": \"deepseek\", \"name\": \"DS\", \"base_url\": \"http://evil.com\", \"model\": \"x\"}], \"chat_backend\": \"gone\"}')",
            [],
        )
        .unwrap();
        let s = load(&conn);
        let ds = s.backend("deepseek").unwrap();
        assert_eq!(
            ds.base_url, "https://api.deepseek.com",
            "http 地址无效，恢复默认"
        );
        assert_eq!(s.chat_backend, "mock");
        assert!(s.backend("gemini").is_some());
    }

    #[test]
    fn corrupted_json_falls_back_to_default() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('app', '{oops')",
            [],
        )
        .unwrap();
        assert_eq!(load(&conn), Settings::default());
    }
}
