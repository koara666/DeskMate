//! 角色包加载与校验。角色包在运行时**只读**：本模块只有读取函数，没有任何写入接口。
//!
//! 角色包来源（同 id 时后者覆盖前者）：
//! 1. 内置：安装目录 `characters/<id>/`（仓库里的 `characters/`）
//! 2. 用户：`%APPDATA%\DeskMate\characters\<id>\`
//!    - 有 `character.json` + `persona.md`（+ 可选的 `canon.md`）：完整的用户角色包（新增角色或替换内置设定）
//!    - 只有 `assets/`：给内置角色换上用户自己的图片
//!    - 可选的 `quotes.md`（原作台词库，DECISIONS #31）：内置角色也从这里读，台词原文不进公开仓库
//!
//! 安全（防恶意角色包，见 SECURITY.md T6）：id 和图片名只允许 `[a-z0-9_-]`；
//! 只读取固定文件名；文件大小有上限；图片校验文件头；用户目录里的 SVG 不读取。

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::canon::{self, CanonEntry};
use crate::lines::Lines as LinePool;
use crate::quotes::{self, Quotes};

/// 桌宠的表情状态
pub const STATES: [&str; 6] = ["idle", "talking", "thinking", "happy", "sad", "sleep"];

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PERSONA_BYTES: u64 = 64 * 1024;
const MAX_CANON_BYTES: u64 = 64 * 1024;
const MAX_QUOTES_BYTES: u64 = 128 * 1024;
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
    /// 在列表里的排序（越小越靠前），没写时排在最后
    #[serde(default = "default_order")]
    pub order: i32,
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

fn default_order() -> i32 {
    1000
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
    /// 原作往事（canon.md，可选），按关键词取用，见 `canon.rs`
    pub canon: Arc<Vec<CanonEntry>>,
    /// 原作台词库（quotes.md，可选），见 `quotes.rs`
    pub quotes: Arc<Quotes>,
    /// 预写台词池（lines/*.md，可选），见 `lines.rs`
    pub pool: Arc<LinePool>,
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

pub(crate) fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, CharacterError> {
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

/// 读取角色包里可选的 canon.md。没有就是空的；读不了只记日志，不影响角色本身。
fn load_canon(dir: &Path) -> Arc<Vec<CanonEntry>> {
    let path = dir.join("canon.md");
    if !path.exists() {
        return Arc::default();
    }
    match read_limited(&path, MAX_CANON_BYTES) {
        Ok(raw) => Arc::new(canon::parse(&String::from_utf8_lossy(&raw))),
        Err(e) => {
            log::warn!("{} 的往事文件无效，已忽略：{e}", dir.display());
            Arc::default()
        }
    }
}

/// 读取可选的 quotes.md：按顺序找，用第一个存在的。没有就是空的；读不了只记日志，不影响角色本身。
fn load_quotes(dirs: &[&Path]) -> Arc<Quotes> {
    let Some(path) = dirs
        .iter()
        .map(|d| d.join("quotes.md"))
        .find(|p| p.exists())
    else {
        return Arc::default();
    };
    match read_limited(&path, MAX_QUOTES_BYTES) {
        Ok(raw) => {
            let q = quotes::parse(&String::from_utf8_lossy(&raw));
            if q.fixed_dropped > 0 {
                log::warn!(
                    "{} 的常驻台词超出预算，后面 {} 段没有用上",
                    path.display(),
                    q.fixed_dropped
                );
            }
            Arc::new(q)
        }
        Err(e) => {
            log::warn!("{} 无效，已忽略：{e}", path.display());
            Arc::default()
        }
    }
}

/// 读取台词池：内置包的 lines/，再用用户目录 lines/ 里有的场景整节覆盖
fn load_lines(dir: &Path, user_dir: Option<&Path>) -> Arc<LinePool> {
    let mut l = LinePool::load_dir(&dir.join("lines"));
    if let Some(u) = user_dir {
        l.overlay(LinePool::load_dir(&u.join("lines")));
    }
    Arc::new(l)
}

/// 程序内置的通用台词（角色没写的场景用它）：内置角色目录下的 `_common/lines/`
pub fn load_common_lines(builtin_root: Option<&Path>) -> Arc<LinePool> {
    Arc::new(
        builtin_root
            .map(|r| LinePool::load_dir(&r.join("_common").join("lines")))
            .unwrap_or_default(),
    )
}

/// 扫描内置目录和用户目录，返回全部可用角色（按 id 排序）。坏掉的角色包记日志后跳过。
pub fn load_all(builtin_root: Option<&Path>, user_root: &Path) -> Vec<Character> {
    let mut map: BTreeMap<String, Character> = BTreeMap::new();
    if let Some(root) = builtin_root {
        for dir in subdirs(root) {
            match load_pack(&dir) {
                Ok((manifest, persona)) => {
                    let id = manifest.id.clone();
                    let user_dir = user_root.join(&id);
                    map.insert(
                        id.clone(),
                        Character {
                            manifest,
                            persona,
                            canon: load_canon(&dir),
                            // 用户目录优先：炼化出来的台词原文只放在用户目录
                            quotes: load_quotes(&[&user_dir, &dir]),
                            pool: load_lines(&dir, Some(&user_dir)),
                            origin: Origin::Builtin,
                            builtin_dir: Some(dir.clone()),
                            user_dir,
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
                        canon: load_canon(&dir),
                        quotes: load_quotes(&[&dir]),
                        pool: load_lines(&dir, None),
                        origin: Origin::User,
                        builtin_dir,
                        user_dir: dir.clone(),
                    },
                );
            }
            Err(e) => log::warn!("用户角色包 {} 无效：{e}", dir.display()),
        }
    }
    let mut list: Vec<Character> = map.into_values().collect();
    list.sort_by(|a, b| {
        (a.manifest.order, &a.manifest.id).cmp(&(b.manifest.order, &b.manifest.id))
    });
    list
}

fn subdirs(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        // `_` 开头的是程序自己的资料（比如通用台词 `_common`），不是角色
        .filter(|e| !e.file_name().to_string_lossy().starts_with('_'))
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

/// 用户图片走自定义协议（DECISIONS #26）：界面只拿到一个短地址，图片由 WebView 按需读取并缓存，
/// 不再把整张图转成 base64 经进程通信发给每个窗口。
pub const IMG_SCHEME: &str = "deskmate-img";

/// 自定义协议在 WebView 里的地址前缀（Windows 上是 http://<协议>.localhost）
pub fn image_url_base() -> &'static str {
    if cfg!(windows) {
        "http://deskmate-img.localhost"
    } else {
        "deskmate-img://localhost"
    }
}

/// 找到的用户图片（已检查大小和文件头，但还没读整个文件）
struct UserImage {
    path: PathBuf,
    /// 修改时间 + 大小：图片换了地址就变，WebView 不会用到旧缓存
    stamp: String,
}

/// 只读文件开头几个字节（判断图片类型用）
fn read_head(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::with_capacity(16);
    std::fs::File::open(path)?.take(16).read_to_end(&mut buf)?;
    Ok(buf)
}

/// 在用户目录 assets/ 里找 `<name>.<ext>`：不跟随符号链接、限制大小、校验文件头
fn find_user_image(user_dir: &Path, name: &str) -> Option<UserImage> {
    if !valid_id(name) {
        return None;
    }
    let assets = user_dir.join("assets");
    for ext in IMAGE_EXTS {
        let p = assets.join(format!("{name}.{ext}"));
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if !meta.is_file() {
            log::warn!("{} 不是普通文件，已忽略", p.display());
            continue;
        }
        if meta.len() > MAX_IMAGE_BYTES {
            log::warn!(
                "{} 太大了（上限 {MAX_IMAGE_BYTES} 字节），已忽略",
                p.display()
            );
            continue;
        }
        match read_head(&p) {
            Ok(head) if sniff_image(&head).is_some() => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis());
                return Some(UserImage {
                    path: p,
                    stamp: format!("{mtime}-{}", meta.len()),
                });
            }
            Ok(_) => log::warn!("{} 不是有效的图片文件，已忽略", p.display()),
            Err(e) => log::warn!("读取角色图片失败：{e}"),
        }
    }
    None
}

/// 用户图片的访问地址；没有可用的图片时返回 None
fn user_image_url(c: &Character, name: &str) -> Option<String> {
    let img = find_user_image(&c.user_dir, name)?;
    Some(format!(
        "{}/{}/{name}?v={}",
        image_url_base(),
        c.manifest.id,
        img.stamp
    ))
}

/// 自定义协议的处理：路径是 `/<角色 id>/<图片名>`，返回图片内容和类型。
/// 读取时重新做全部校验（地址可能被篡改，文件也可能在两次读取之间被换掉）。
pub fn serve_image(c: Option<&Character>, path: &str) -> Option<(Vec<u8>, &'static str)> {
    let mut parts = path.trim_start_matches('/').split('/');
    let (id, name) = (parts.next()?, parts.next()?);
    if parts.next().is_some() || !valid_id(id) {
        return None;
    }
    let c = c.filter(|c| c.manifest.id == id)?;
    let img = find_user_image(&c.user_dir, name)?;
    let bytes = read_limited(&img.path, MAX_IMAGE_BYTES).ok()?;
    let mime = sniff_image(&bytes)?;
    Some((bytes, mime))
}

/// 给前端的形象数据
#[derive(Debug, Clone, Serialize)]
pub struct Appearance {
    /// 每个状态对应的图片地址（用户图片走自定义协议；内置占位 SVG 很小，直接用 data URL）
    pub images: BTreeMap<String, String>,
    /// 是否在用占位形象（占位形象由前端用 CSS 表现不同状态）
    pub placeholder: bool,
}

/// 计算每个状态用哪张图：用户图片（按表情映射）→ 用户的 idle 图 → 占位 SVG
pub fn appearance(c: &Character) -> Appearance {
    let mut images = BTreeMap::new();
    let idle_user = user_image_url(c, c.manifest.expressions.get("idle").map_or("idle", |s| s));
    for state in STATES {
        let name = c
            .manifest
            .expressions
            .get(state)
            .map_or(state, |s| s.as_str());
        // 同一张图用同一个地址，WebView 只读取、解码一次
        if let Some(url) = user_image_url(c, name).or_else(|| idle_user.clone()) {
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
            find_user_image(&c.user_dir, name).is_some()
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
    fn canon_is_optional_and_size_limited() {
        let b = tempfile::tempdir().unwrap();
        let u = tempfile::tempdir().unwrap();
        let d = write_pack(b.path(), "cat", r#"{"id":"cat","name":"猫"}"#);
        fs::write(d.join("canon.md"), "## 往事\n关键词：小鱼\n吃过小鱼。\n").unwrap();
        let d = write_pack(b.path(), "dog", r#"{"id":"dog","name":"狗"}"#);
        fs::write(d.join("canon.md"), "x".repeat(MAX_CANON_BYTES as usize + 1)).unwrap();
        write_pack(b.path(), "owl", r#"{"id":"owl","name":"猫头鹰"}"#);
        let all = load_all(Some(b.path()), u.path());
        assert_eq!(all.len(), 3, "往事文件坏了不影响角色本身");
        let canon_of = |id: &str| {
            all.iter()
                .find(|c| c.manifest.id == id)
                .unwrap()
                .canon
                .len()
        };
        assert_eq!(canon_of("cat"), 1);
        assert_eq!(canon_of("dog"), 0);
        assert_eq!(canon_of("owl"), 0);
        // 用户完整角色包覆盖内置时，用的是用户包自己的往事
        write_pack(u.path(), "cat", r#"{"id":"cat","name":"用户猫"}"#);
        let all = load_all(Some(b.path()), u.path());
        assert!(
            all.iter()
                .find(|c| c.manifest.id == "cat")
                .unwrap()
                .canon
                .is_empty()
        );
    }

    #[test]
    fn quotes_come_from_user_dir_first() {
        let b = tempfile::tempdir().unwrap();
        let u = tempfile::tempdir().unwrap();
        // 内置角色：用户目录里只放了台词库（没有 character.json），也要读到
        write_pack(b.path(), "cat", r#"{"id":"cat","name":"猫"}"#);
        fs::create_dir_all(u.path().join("cat")).unwrap();
        fs::write(u.path().join("cat/quotes.md"), "## 打招呼\n猫：喵。\n").unwrap();
        // 用户目录优先于内置目录
        let d = write_pack(b.path(), "dog", r#"{"id":"dog","name":"狗"}"#);
        fs::write(
            d.join("quotes.md"),
            "## 内置\n狗：汪。\n## 内置2\n狗：汪汪。\n",
        )
        .unwrap();
        fs::create_dir_all(u.path().join("dog")).unwrap();
        fs::write(
            u.path().join("dog/quotes.md"),
            "## 用户\n关键词：骨头\n狗：骨头！\n",
        )
        .unwrap();
        // 太大的台词库忽略，角色照常可用
        write_pack(b.path(), "owl", r#"{"id":"owl","name":"猫头鹰"}"#);
        fs::create_dir_all(u.path().join("owl")).unwrap();
        fs::write(
            u.path().join("owl/quotes.md"),
            "x".repeat(MAX_QUOTES_BYTES as usize + 1),
        )
        .unwrap();
        // 用户完整角色包：读自己目录里的
        let d = write_pack(u.path(), "fox", r#"{"id":"fox","name":"狐狸"}"#);
        fs::write(d.join("quotes.md"), "## 片段\n狐狸：嗯。\n").unwrap();

        let all = load_all(Some(b.path()), u.path());
        assert_eq!(all.len(), 4);
        let q = |id: &str| {
            let c = all.iter().find(|c| c.manifest.id == id).unwrap();
            (c.quotes.fixed_count, c.quotes.keyed.len())
        };
        assert_eq!(q("cat"), (1, 0));
        assert_eq!(q("dog"), (0, 1), "用了用户目录的，没有和内置的混在一起");
        assert_eq!(q("owl"), (0, 0));
        assert_eq!(q("fox"), (1, 0));
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
        let base = format!("{}/cat/", image_url_base());
        assert!(a.images["sad"].starts_with(&format!("{base}confused?v=")));
        assert!(a.images["idle"].starts_with(&format!("{base}idle?v=")));
        assert_eq!(a.images["happy"], a.images["idle"]); // 文件头不对 → 回退到 idle，且是同一个地址
        assert_eq!(a.images["talking"], a.images["idle"]); // 用户 SVG 不读取
        assert_eq!(a.images.len(), STATES.len());
        assert_eq!(user_image_states(&all[0]), vec!["idle", "sad"]);

        // 协议处理：只给对应角色的、校验通过的图片
        let c = Some(&all[0]);
        let (bytes, mime) = serve_image(c, "/cat/confused").unwrap();
        assert_eq!(
            (bytes.as_slice(), mime),
            (&b"RIFF\0\0\0\0WEBPSAD"[..], "image/webp")
        );
        assert_eq!(serve_image(c, "/cat/idle").unwrap().1, "image/png");
        assert!(serve_image(c, "/cat/happy").is_none(), "文件头不对");
        assert!(serve_image(c, "/cat/talking").is_none(), "SVG 不提供");
        assert!(serve_image(c, "/dog/idle").is_none(), "不是当前角色");
        assert!(serve_image(c, "/cat/../cat/idle").is_none());
        assert!(serve_image(c, "/cat/idle/x").is_none());
        assert!(serve_image(c, "/cat/IDLE").is_none());
        assert!(serve_image(None, "/cat/idle").is_none());
    }

    #[test]
    fn oversized_user_image_is_ignored() {
        let u = tempfile::tempdir().unwrap();
        let ua = u.path().join("cat").join("assets");
        fs::create_dir_all(&ua).unwrap();
        let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
        big.resize(MAX_IMAGE_BYTES as usize + 10, 0);
        fs::write(ua.join("idle.png"), big).unwrap();
        assert!(find_user_image(&u.path().join("cat"), "idle").is_none());
    }

    #[test]
    fn all_builtin_packs_are_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../characters");
        let empty = tempfile::tempdir().unwrap();
        let all = load_all(Some(&root), empty.path());
        let ids: Vec<&str> = all.iter().map(|c| c.manifest.id.as_str()).collect();
        assert_eq!(all[0].manifest.id, "deepseek-whale", "按 order 排序");
        assert_eq!(all[7].manifest.id, "yui-sao");
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
        // 有原作往事的角色：每条都能被关键词想起
        for id in ["makise-kurisu", "nanami-chiaki"] {
            let c = all.iter().find(|c| c.manifest.id == id).unwrap();
            assert!(c.canon.len() >= 10, "{id} 往事太少");
            for e in c.canon.iter() {
                let hit = crate::canon::recall(&c.canon, &e.keywords[0], "");
                assert!(
                    hit.iter().any(|h| h.starts_with(&format!("{}：", e.title))),
                    "{id} 的「{}」用第一个关键词想不起来（太长或被别的条目挤掉）",
                    e.title
                );
            }
        }
    }

    #[test]
    fn pick_line_falls_back() {
        assert_eq!(pick_line(&[], "默认", 3), "默认");
        let l = vec!["a".to_string(), "b".to_string()];
        assert_eq!(pick_line(&l, "默认", 3), "b");
    }
}
