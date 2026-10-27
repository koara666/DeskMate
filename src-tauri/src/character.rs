//! 角色包加载与校验。角色包在运行时**只读**：本模块只有读取函数，没有任何写入接口。
//!
//! 角色包来源（同 id 时后者覆盖前者）：
//! 1. 内置：安装目录 `characters/<id>/`（仓库里的 `characters/`）
//! 2. 用户：`%APPDATA%\DeskMate\characters\<id>\`
//!    - 有 `character.json` + `persona.md`：完整的用户角色包（新增角色或替换内置设定）
//!    - 只有 `assets/`：给内置角色换上用户自己的图片
//!
//! 安全（防恶意角色包，见 SECURITY.md T6）：id 和图片名只允许 `[a-z0-9_-]`；
//! 只读取固定文件名；文件大小有上限；图片校验文件头；用户目录里的 SVG 不读取。

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 桌宠的表情状态
pub const STATES: [&str; 6] = ["idle", "talking", "thinking", "happy", "sad", "sleep"];

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PERSONA_BYTES: u64 = 64 * 1024;
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_SVG_BYTES: u64 = 256 * 1024;
const IMAGE_EXTS: [&str; 5] = ["png", "webp", "gif", "jpg", "jpeg"];

/// 没有任何图片时使用的通用占位形象（原创几何图形）
const FALLBACK_SVG: &str = include_str!("../../characters/_fallback.svg");

/// 角色口吻的固定台词（出错、断网等场景，不需要调用模型）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lines {
    pub greeting: Vec<String>,
    pub error: Vec<String>,
    pub offline: Vec<String>,
    pub budget: Vec<String>,
    pub not_configured: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub description: String,
    /// 原作 / 原型出处
    #[serde(default)]
    pub source: String,
    /// 主题色（#rrggbb），用于气泡边框等
    #[serde(default)]
    pub color: String,
    /// 表情映射：状态 → 图片文件名（不含扩展名）。没写的状态用同名图片。
    #[serde(default)]
    pub expressions: BTreeMap<String, String>,
    #[serde(default)]
    pub lines: Lines,
}

fn default_language() -> String {
    "zh-CN".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Builtin,
    User,
}

#[derive(Debug, Clone)]
pub struct Character {
    pub manifest: Manifest,
    pub persona: String,
    pub origin: Origin,
    /// 内置角色包目录（读取占位 SVG）
    pub builtin_dir: Option<PathBuf>,
    /// 用户目录（读取用户图片），不一定存在
    pub user_dir: PathBuf,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CharacterError {
    #[error("角色 id「{0}」不合法（只能用小写字母、数字、- 和 _，最长 40 个字符）")]
    BadId(String),
    #[error("{0} 太大了（上限 {1} 字节）")]
    TooLarge(String, u64),
    #[error("读取 {0} 失败：{1}")]
    Io(String, String),
    #[error("character.json 格式错误：{0}")]
    BadJson(String),
    #[error("character.json 里的 id「{0}」和文件夹名「{1}」不一致")]
    IdMismatch(String, String),
    #[error("角色名不能为空")]
    EmptyName,
    #[error("表情映射里的图片名「{0}」不合法")]
    BadImageName(String),
}

pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 40
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-' || *c == b'_')
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, CharacterError> {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let meta = std::fs::symlink_metadata(path)
        .map_err(|e| CharacterError::Io(name.clone(), e.to_string()))?;
    // 不跟随符号链接，防止指向角色包以外的文件
    if !meta.is_file() {
        return Err(CharacterError::Io(name, "不是普通文件".into()));
    }
    if meta.len() > limit {
        return Err(CharacterError::TooLarge(name, limit));
    }
    std::fs::read(path).map_err(|e| CharacterError::Io(name, e.to_string()))
}

/// 读取并校验一个完整角色包（character.json + persona.md）
pub fn load_pack(dir: &Path) -> Result<(Manifest, String), CharacterError> {
    let dir_name = dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if !valid_id(&dir_name) {
        return Err(CharacterError::BadId(dir_name));
    }
    let raw = read_limited(&dir.join("character.json"), MAX_MANIFEST_BYTES)?;
    let manifest: Manifest =
        serde_json::from_slice(&raw).map_err(|e| CharacterError::BadJson(e.to_string()))?;
    if manifest.id != dir_name {
        return Err(CharacterError::IdMismatch(manifest.id, dir_name));
    }
    if manifest.name.trim().is_empty() {
        return Err(CharacterError::EmptyName);
    }
    for v in manifest.expressions.values() {
        if !valid_id(v) {
            return Err(CharacterError::BadImageName(v.clone()));
        }
    }
    let persona_raw = read_limited(&dir.join("persona.md"), MAX_PERSONA_BYTES)?;
    let persona = String::from_utf8_lossy(&persona_raw).into_owned();
    Ok((manifest, persona))
}

/// 扫描内置目录和用户目录，返回全部可用角色（按 id 排序）。坏掉的角色包记日志后跳过。
pub fn load_all(builtin_root: Option<&Path>, user_root: &Path) -> Vec<Character> {
    let mut map: BTreeMap<String, Character> = BTreeMap::new();
    if let Some(root) = builtin_root {
        for dir in subdirs(root) {
            match load_pack(&dir) {
                Ok((manifest, persona)) => {
                    let id = manifest.id.clone();
                    map.insert(
                        id.clone(),
                        Character {
                            manifest,
                            persona,
                            origin: Origin::Builtin,
                            builtin_dir: Some(dir.clone()),
                            user_dir: user_root.join(&id),
                        },
                    );
                }
                Err(e) => log::warn!("内置角色包 {} 无效：{e}", dir.display()),
            }
        }
    }
    for dir in subdirs(user_root) {
        if !dir.join("character.json").exists() {
            continue; // 只有图片的目录：给内置角色换图，不是新角色
        }
        match load_pack(&dir) {
            Ok((manifest, persona)) => {
                let id = manifest.id.clone();
                let builtin_dir = map.get(&id).and_then(|c| c.builtin_dir.clone());
                map.insert(
                    id,
                    Character {
                        manifest,
                        persona,
                        origin: Origin::User,
                        builtin_dir,
                        user_dir: dir.clone(),
                    },
                );
            }
            Err(e) => log::warn!("用户角色包 {} 无效：{e}", dir.display()),
        }
    }
    map.into_values().collect()
}

fn subdirs(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

/// 根据文件头判断图片类型，返回 MIME；不认识的返回 None
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else {
        None
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// 在用户目录 assets/ 里找 `<name>.<ext>`，校验后返回 data URL
fn user_image(user_dir: &Path, name: &str) -> Option<String> {
    if !valid_id(name) {
        return None;
    }
    let assets = user_dir.join("assets");
    for ext in IMAGE_EXTS {
        let p = assets.join(format!("{name}.{ext}"));
        if !p.exists() {
            continue;
        }
        match read_limited(&p, MAX_IMAGE_BYTES) {
            Ok(bytes) => match sniff_image(&bytes) {
                Some(mime) => return Some(data_url(mime, &bytes)),
                None => log::warn!("{} 不是有效的图片文件，已忽略", p.display()),
            },
            Err(e) => log::warn!("角色图片无法使用：{e}"),
        }
    }
    None
}

/// 给前端的形象数据
#[derive(Debug, Clone, Serialize)]
pub struct Appearance {
    /// 每个状态对应的图片（data URL）
    pub images: BTreeMap<String, String>,
    /// 是否在用占位形象（占位形象由前端用 CSS 表现不同状态）
    pub placeholder: bool,
}

/// 计算每个状态用哪张图：用户图片（按表情映射）→ 用户的 idle 图 → 占位 SVG
pub fn appearance(c: &Character) -> Appearance {
    let mut images = BTreeMap::new();
    let idle_user = user_image(
        &c.user_dir,
        c.manifest.expressions.get("idle").map_or("idle", |s| s),
    );
    for state in STATES {
        let name = c
            .manifest
            .expressions
            .get(state)
            .map_or(state, |s| s.as_str());
        if let Some(url) = user_image(&c.user_dir, name).or_else(|| idle_user.clone()) {
            images.insert(state.to_string(), url);
        }
    }
    if images.is_empty() {
        let svg = c
            .builtin_dir
            .as_ref()
            .and_then(|d| {
                read_limited(&d.join("assets").join("placeholder.svg"), MAX_SVG_BYTES).ok()
            })
            .unwrap_or_else(|| FALLBACK_SVG.as_bytes().to_vec());
        let url = data_url("image/svg+xml", &svg);
        for state in STATES {
            images.insert(state.to_string(), url.clone());
        }
        return Appearance {
            images,
            placeholder: true,
        };
    }
    Appearance {
        images,
        placeholder: false,
    }
}

/// 用户为哪些状态提供了自己的图片（设置界面显示用）
pub fn user_image_states(c: &Character) -> Vec<String> {
    STATES
        .iter()
        .filter(|state| {
            let name = c
                .manifest
                .expressions
                .get(**state)
                .map_or(**state, |s| s.as_str());
            user_image(&c.user_dir, name).is_some()
        })
        .map(|s| s.to_string())
        .collect()
}

/// 从台词列表里挑一句（列表为空时用默认句）
pub fn pick_line(list: &[String], fallback: &str, seed: u64) -> String {
    if list.is_empty() {
        fallback.to_string()
    } else {
        list[(seed as usize) % list.len()].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_pack(root: &Path, id: &str, json: &str) -> PathBuf {
        let d = root.join(id);
        fs::create_dir_all(d.join("assets")).unwrap();
        fs::write(d.join("character.json"), json).unwrap();
        fs::write(d.join("persona.md"), "性格：温柔").unwrap();
        d
    }

    #[test]
    fn id_rules() {
        assert!(valid_id("deepseek-whale"));
        assert!(valid_id("yui_2"));
        assert!(!valid_id(""));
        assert!(!valid_id("../evil"));
        assert!(!valid_id("Upper"));
        assert!(!valid_id("-start"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id(&"a".repeat(41)));
    }

    #[test]
    fn loads_valid_pack() {
        let t = tempfile::tempdir().unwrap();
        let d = write_pack(t.path(), "cat", r#"{"id":"cat","name":"小猫"}"#);
        let (m, p) = load_pack(&d).unwrap();
        assert_eq!(m.name, "小猫");
        assert_eq!(m.language, "zh-CN");
        assert_eq!(p, "性格：温柔");
    }

    #[test]
    fn rejects_bad_packs() {
        let t = tempfile::tempdir().unwrap();
        let d = write_pack(t.path(), "cat", r#"{"id":"dog","name":"小猫"}"#);
        assert!(matches!(load_pack(&d), Err(CharacterError::IdMismatch(..))));
        let d = write_pack(t.path(), "cat2", r#"{"id":"cat2","name":"  "}"#);
        assert_eq!(load_pack(&d), Err(CharacterError::EmptyName));
        let d = write_pack(
            t.path(),
            "cat3",
            r#"{"id":"cat3","name":"x","expressions":{"idle":"../../secret"}}"#,
        );
        assert!(matches!(
            load_pack(&d),
            Err(CharacterError::BadImageName(_))
        ));
        let d = write_pack(t.path(), "cat4", "{not json");
        assert!(matches!(load_pack(&d), Err(CharacterError::BadJson(_))));
        let d = write_pack(t.path(), "cat5", r#"{"id":"cat5","name":"x"}"#);
        fs::write(
            d.join("persona.md"),
            "x".repeat(MAX_PERSONA_BYTES as usize + 1),
        )
        .unwrap();
        assert!(matches!(load_pack(&d), Err(CharacterError::TooLarge(..))));
        let d = write_pack(t.path(), "Bad", r#"{"id":"Bad","name":"x"}"#);
        assert!(matches!(load_pack(&d), Err(CharacterError::BadId(_))));
    }

    #[test]
    fn user_pack_overrides_builtin_and_bad_packs_are_skipped() {
        let b = tempfile::tempdir().unwrap();
        let u = tempfile::tempdir().unwrap();
        write_pack(b.path(), "cat", r#"{"id":"cat","name":"内置猫"}"#);
        write_pack(b.path(), "dog", r#"{"id":"dog","name":"内置狗"}"#);
        write_pack(u.path(), "cat", r#"{"id":"cat","name":"用户猫"}"#);
        write_pack(u.path(), "broken", "{");
        fs::create_dir_all(u.path().join("dog").join("assets")).unwrap(); // 只有图片目录
        let all = load_all(Some(b.path()), u.path());
        let names: Vec<_> = all.iter().map(|c| c.manifest.name.as_str()).collect();
        assert_eq!(names, vec!["用户猫", "内置狗"]);
        assert_eq!(all[0].origin, Origin::User);
        assert!(all[0].builtin_dir.is_some(), "用户覆盖后仍能用内置占位图");
        assert_eq!(all[1].user_dir, u.path().join("dog"));
    }

    #[test]
    fn sniffs_images() {
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\nxxxx"), Some("image/png"));
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image(b"GIF89a..."), Some("image/gif"));
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"<svg onload=alert(1)>"), None);
        assert_eq!(sniff_image(b"MZ\x90\0"), None);
    }

    #[test]
    fn appearance_prefers_user_images_with_fallbacks() {
        let b = tempfile::tempdir().unwrap();
        let u = tempfile::tempdir().unwrap();
        let bd = write_pack(
            b.path(),
            "cat",
            r#"{"id":"cat","name":"猫","expressions":{"sad":"confused"}}"#,
        );
        fs::write(bd.join("assets").join("placeholder.svg"), "<svg/>").unwrap();
        let all = load_all(Some(b.path()), u.path());
        let a = appearance(&all[0]);
        assert!(a.placeholder);
        assert!(a.images["idle"].starts_with("data:image/svg+xml;base64,"));

        let ua = u.path().join("cat").join("assets");
        fs::create_dir_all(&ua).unwrap();
        fs::write(ua.join("idle.png"), b"\x89PNG\r\n\x1a\nIDLE").unwrap();
        fs::write(ua.join("confused.webp"), b"RIFF\0\0\0\0WEBPSAD").unwrap();
        fs::write(ua.join("happy.png"), b"not really a png").unwrap(); // 文件头不对，忽略
        fs::write(ua.join("talking.svg"), "<svg/>").unwrap(); // 用户 SVG 不读取
        let a = appearance(&all[0]);
        assert!(!a.placeholder);
        assert!(a.images["sad"].starts_with("data:image/webp"));
        assert!(a.images["happy"].starts_with("data:image/png")); // 回退到 idle
        assert!(a.images["talking"].starts_with("data:image/png"));
        assert_eq!(a.images.len(), STATES.len());
    }

    #[test]
    fn oversized_user_image_is_ignored() {
        let u = tempfile::tempdir().unwrap();
        let ua = u.path().join("cat").join("assets");
        fs::create_dir_all(&ua).unwrap();
        let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
        big.resize(MAX_IMAGE_BYTES as usize + 10, 0);
        fs::write(ua.join("idle.png"), big).unwrap();
        assert!(user_image(&u.path().join("cat"), "idle").is_none());
    }

    #[test]
    fn all_builtin_packs_are_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../characters");
        let empty = tempfile::tempdir().unwrap();
        let all = load_all(Some(&root), empty.path());
        let ids: Vec<&str> = all.iter().map(|c| c.manifest.id.as_str()).collect();
        for id in [
            "deepseek-whale",
            "nailong",
            "naiwa",
            "makise-kurisu",
            "shiina-mayuri",
            "nanami-chiaki",
            "fujisaki-chihiro",
            "yui-sao",
        ] {
            assert!(ids.contains(&id), "缺少内置角色 {id}");
        }
        for c in &all {
            let id = &c.manifest.id;
            assert!(c.persona.contains("示范对话"), "{id} 缺少示范对话");
            assert!(c.persona.contains("原创"), "{id} 应声明示范对话为原创");
            assert!(!c.manifest.lines.greeting.is_empty(), "{id} 缺少开场白");
            assert!(c.manifest.color.starts_with('#'), "{id} 缺少主题色");
            let a = appearance(c);
            assert!(a.placeholder);
            assert!(
                c.builtin_dir
                    .as_ref()
                    .unwrap()
                    .join("assets/placeholder.svg")
                    .is_file(),
                "{id} 缺少占位形象"
            );
            assert!(user_image_states(c).is_empty());
        }
    }

    #[test]
    fn pick_line_falls_back() {
        assert_eq!(pick_line(&[], "默认", 3), "默认");
        let l = vec!["a".to_string(), "b".to_string()];
        assert_eq!(pick_line(&l, "默认", 3), "b");
    }
}
