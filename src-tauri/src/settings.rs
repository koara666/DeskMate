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

/// 某个角色单独的用户偏好（叠加在全局偏好之上）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterPrefs {
    pub banned_words: Vec<String>,
    pub custom_rules: Vec<String>,
}

const MAX_BANNED: usize = 200;
const MAX_BANNED_LEN: usize = 50;
const MAX_RULES: usize = 30;
const MAX_RULE_LEN: usize = 300;

/// 清理用户输入的列表：去首尾空白、去空行、去重、限制条数和长度
pub fn clean_list(items: &[String], max_items: usize, max_len: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for it in items {
        let t: String = it.trim().chars().take(max_len).collect();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
        if out.len() >= max_items {
            break;
        }
    }
    out
}

impl CharacterPrefs {
    pub fn sanitize(self) -> Self {
        Self {
            banned_words: clean_list(&self.banned_words, MAX_BANNED, MAX_BANNED_LEN),
            custom_rules: clean_list(&self.custom_rules, MAX_RULES, MAX_RULE_LEN),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.banned_words.is_empty() && self.custom_rules.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 全局禁词和自定义要求（对所有角色生效）
    pub global_prefs: CharacterPrefs,
    /// 每个角色单独的禁词和自定义要求
    pub character_prefs: std::collections::BTreeMap<String, CharacterPrefs>,
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
    /// 暂停记忆：不记录新的长期记忆（已有记忆仍可使用）
    pub memory_paused: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            global_prefs: CharacterPrefs::default(),
            character_prefs: Default::default(),
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
            memory_paused: false,
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
        self.global_prefs = std::mem::take(&mut self.global_prefs).sanitize();
        self.character_prefs = std::mem::take(&mut self.character_prefs)
            .into_iter()
            .filter(|(id, _)| crate::character::valid_id(id))
            .map(|(id, p)| (id, p.sanitize()))
            .filter(|(_, p)| !p.is_empty())
            .collect();
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

    /// 某个角色实际生效的禁词和自定义要求 = 全局 + 角色专属
    pub fn effective_prefs(&self, character_id: &str) -> CharacterPrefs {
        let mut p = self.global_prefs.clone();
        if let Some(c) = self.character_prefs.get(character_id) {
            p.banned_words.extend(c.banned_words.iter().cloned());
            p.custom_rules.extend(c.custom_rules.iter().cloned());
        }
        CharacterPrefs {
            banned_words: clean_list(&p.banned_words, MAX_BANNED * 2, MAX_BANNED_LEN),
            custom_rules: clean_list(&p.custom_rules, MAX_RULES * 2, MAX_RULE_LEN),
        }
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
    fn prefs_are_cleaned_and_merged() {
        let mut s = Settings::default();
        s.global_prefs.banned_words =
            vec![" 笨蛋 ".into(), "".into(), "笨蛋".into(), "x".repeat(80)];
        s.global_prefs.custom_rules = vec!["回复不超过三句话".into()];
        s.character_prefs.insert(
            "nailong".into(),
            CharacterPrefs {
                banned_words: vec!["笨蛋".into(), "傻".into()],
                custom_rules: vec!["叫我小林".into()],
            },
        );
        s.character_prefs
            .insert("../bad".into(), CharacterPrefs::default());
        s.character_prefs
            .insert("empty".into(), CharacterPrefs::default());
        let s = s.sanitize();
        assert_eq!(s.global_prefs.banned_words.len(), 2);
        assert_eq!(s.global_prefs.banned_words[0], "笨蛋");
        assert_eq!(s.global_prefs.banned_words[1].chars().count(), 50);
        assert_eq!(s.character_prefs.len(), 1, "无效 id 和空偏好被丢掉");
        let p = s.effective_prefs("nailong");
        assert_eq!(p.banned_words.iter().filter(|w| *w == "笨蛋").count(), 1);
        assert!(p.banned_words.contains(&"傻".to_string()));
        assert_eq!(p.custom_rules, vec!["回复不超过三句话", "叫我小林"]);
        assert_eq!(
            s.effective_prefs("other").custom_rules,
            vec!["回复不超过三句话"]
        );
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
