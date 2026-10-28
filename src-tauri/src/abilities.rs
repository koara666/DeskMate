//! 内置能力层（DECISIONS #33、#36）：互动中偶尔需要的轻量操作，优先于外部 agent。
//!
//! - 打开文件夹 / 文件 / 网址 / 用户登记的常用程序：角色在回复里写 `[[打开：…]]`，
//!   气泡里出现按钮，**用户点了才打开**。程序、脚本、快捷方式等可执行的文件一律不直接打开，
//!   只有用户在「设置 → 高级」里登记过的程序才能按名字打开。
//! - 在用户允许的文件夹里读文本文件：`[[读取：路径]]`，用户点了才读；内容作为不可信资料交给模型。
//! - 系统信息：电量（时间已在提示词里）。
//!
//! 安全：路径必须是完整路径；先规范化（解析 `..` 和链接）再检查是否在允许的文件夹里；
//! 网址只接受 http / https；不经过 cmd.exe。

use crate::markers::{Marker, MarkerKind};
use crate::tasks::sandbox;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 用户登记的常用程序（按名字打开）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProgramEntry {
    pub name: String,
    pub path: String,
}

/// 「高级」设置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AdvancedSettings {
    /// 允许委托外部 agent（Claude Code / Codex），默认关闭（DECISIONS #33）
    pub allow_delegation: bool,
    /// 允许角色读取文本文件的文件夹
    pub read_dirs: Vec<String>,
    /// 可以按名字打开的常用程序
    pub programs: Vec<ProgramEntry>,
}

const MAX_DIRS: usize = 20;
const MAX_PROGRAMS: usize = 20;

impl AdvancedSettings {
    pub fn sanitize(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.read_dirs = self
            .read_dirs
            .iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty() && d.chars().count() <= 260)
            .filter(|d| seen.insert(d.to_lowercase()))
            .take(MAX_DIRS)
            .collect();
        let mut names = std::collections::HashSet::new();
        self.programs = self
            .programs
            .into_iter()
            .map(|p| ProgramEntry {
                name: p.name.trim().chars().take(20).collect(),
                path: p.path.trim().trim_matches('"').to_string(),
            })
            .filter(|p| !p.name.is_empty() && !p.path.is_empty() && p.path.chars().count() <= 260)
            .filter(|p| names.insert(p.name.to_lowercase()))
            .take(MAX_PROGRAMS)
            .collect();
        self
    }
}

/// 能直接打开的文件类型（文档、图片、音视频、文本）。程序、脚本、快捷方式不在其中。
const OPEN_EXTS: &[&str] = &[
    "txt", "md", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "csv", "rtf", "odt", "ods",
    "odp", "epub", "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "mp3", "wav", "flac", "m4a",
    "ogg", "mp4", "mkv", "avi", "mov", "webm", "log",
];

/// 能读取的文本类型
const READ_EXTS: &[&str] = &[
    "txt", "md", "markdown", "csv", "log", "json", "ini", "toml", "yaml", "yml", "xml", "srt",
    "ass", "rs", "py", "js", "ts", "tsx", "jsx", "java", "c", "cpp", "h", "go", "html", "css",
];

const MAX_READ_BYTES: u64 = 1024 * 1024;
const MAX_READ_CHARS: usize = 6000;

/// 解析后的打开目标
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenTarget {
    Url { url: String },
    Folder { path: String },
    File { path: String },
    Program { name: String, path: String },
}

impl OpenTarget {
    /// 给用户看的一句话
    pub fn describe(&self) -> String {
        match self {
            OpenTarget::Url { url } => format!("网页 {url}"),
            OpenTarget::Folder { path } => format!("文件夹 {path}"),
            OpenTarget::File { path } => format!("文件 {path}"),
            OpenTarget::Program { name, .. } => format!("程序「{name}」"),
        }
    }
}

fn clean_target(raw: &str) -> &str {
    raw.trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`' | '「' | '」' | '“' | '”'))
        .trim()
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// 检查网址：只接受 http / https，不能有空白和控制字符
pub fn check_url(s: &str) -> Result<String, String> {
    let lower = s.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("只能打开 http / https 网址".into());
    }
    if s.len() > 2000 || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("网址格式不对".into());
    }
    let rest = &s[s.find("://").map(|i| i + 3).unwrap_or(0)..];
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') {
        return Err("网址格式不对".into());
    }
    Ok(s.to_string())
}

/// 解析角色想打开的东西（不打开，只检查）
pub fn resolve_open(raw: &str, programs: &[ProgramEntry]) -> Result<OpenTarget, String> {
    let t = clean_target(raw);
    if t.is_empty() {
        return Err("没说要打开什么".into());
    }
    let lower = t.to_ascii_lowercase();
    if lower.contains("://") {
        return check_url(t).map(|url| OpenTarget::Url { url });
    }
    if let Some(p) = programs.iter().find(|p| p.name.eq_ignore_ascii_case(t)) {
        let path = PathBuf::from(&p.path);
        if !path.is_file() {
            return Err(format!("登记的程序「{}」找不到了：{}", p.name, p.path));
        }
        return Ok(OpenTarget::Program {
            name: p.name.clone(),
            path: p.path.clone(),
        });
    }
    let path = PathBuf::from(t);
    if !path.is_absolute() {
        return Err(format!(
            "「{t}」不是完整路径，也不是登记过的常用程序（可以在「设置 → 高级」里登记）"
        ));
    }
    let meta = std::fs::metadata(&path).map_err(|_| format!("找不到 {t}"))?;
    let canon = sandbox::simplify(&std::fs::canonicalize(&path).map_err(|e| e.to_string())?);
    if meta.is_dir() {
        return Ok(OpenTarget::Folder {
            path: canon.display().to_string(),
        });
    }
    // 规范化后再看扩展名：防止快捷方式、链接指向程序
    if OPEN_EXTS.contains(&ext_of(&canon).as_str()) {
        Ok(OpenTarget::File {
            path: canon.display().to_string(),
        })
    } else {
        Err("出于安全考虑，程序、脚本、快捷方式这类文件不能直接打开（常用程序可以在「设置 → 高级」里登记后按名字打开）".into())
    }
}

/// 真正打开（用户点了按钮之后）
pub fn open(target: &OpenTarget) -> Result<(), String> {
    let s = match target {
        OpenTarget::Url { url } => url.as_str(),
        OpenTarget::Folder { path } | OpenTarget::File { path } => path.as_str(),
        OpenTarget::Program { path, .. } => path.as_str(),
    };
    shell_open(s)
}

#[cfg(windows)]
fn shell_open(s: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |x: &str| x.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let verb = wide("open");
    let file = wide(s);
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // 返回值大于 32 表示成功
    if r as usize > 32 {
        Ok(())
    } else {
        Err(format!("打开失败（错误码 {}）", r as usize))
    }
}

#[cfg(not(windows))]
fn shell_open(_s: &str) -> Result<(), String> {
    Err("这个系统上还不支持".into())
}

/// 读到的文件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadResult {
    pub name: String,
    pub text: String,
    pub truncated: bool,
}

/// 检查路径能不能读（在允许的文件夹里、是文本类型），返回规范化后的路径
pub fn check_read(raw: &str, dirs: &[String]) -> Result<PathBuf, String> {
    let t = clean_target(raw);
    let path = PathBuf::from(t);
    if !path.is_absolute() {
        return Err(format!("「{t}」不是完整路径"));
    }
    if dirs.is_empty() {
        return Err("还没有允许读取的文件夹：到「设置 → 高级」里添加一个".into());
    }
    let canon =
        sandbox::simplify(&std::fs::canonicalize(&path).map_err(|_| format!("找不到 {t}"))?);
    if !canon.is_file() {
        return Err(format!("{t} 不是文件"));
    }
    let allowed = dirs.iter().any(|d| {
        std::fs::canonicalize(d)
            .map(|c| sandbox::is_within(&canon, &sandbox::simplify(&c)))
            .unwrap_or(false)
    });
    if !allowed {
        return Err(format!(
            "{t} 不在允许读取的文件夹里（可以在「设置 → 高级」里添加）"
        ));
    }
    if !READ_EXTS.contains(&ext_of(&canon).as_str()) {
        return Err("只能读文本类的文件（txt、md、csv、代码等）".into());
    }
    Ok(canon)
}

/// 把字节解码成文本：UTF-8（可带 BOM）或 UTF-16 LE（带 BOM）。看不懂的编码返回 None。
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => Some(s.to_string()),
        // 只是在截断处切坏了最后一个字
        Err(e) if e.valid_up_to() + 4 >= b.len() => {
            Some(String::from_utf8_lossy(&b[..e.valid_up_to()]).into_owned())
        }
        Err(_) => None,
    }
}

/// 读取（用户点了按钮之后）
pub fn read_text(raw: &str, dirs: &[String]) -> Result<ReadResult, String> {
    use std::io::Read;
    let path = check_read(raw, dirs)?;
    let mut f = std::fs::File::open(&path).map_err(|e| format!("读取失败：{e}"))?;
    let mut buf = Vec::new();
    f.by_ref()
        .take(MAX_READ_BYTES)
        .read_to_end(&mut buf)
        .map_err(|e| format!("读取失败：{e}"))?;
    let text = decode_text(&buf)
        .ok_or("看不懂这个文件的编码（只支持 UTF-8 / UTF-16），可以另存为 UTF-8 再试")?;
    let total = text.chars().count();
    Ok(ReadResult {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        text: text.chars().take(MAX_READ_CHARS).collect(),
        truncated: total > MAX_READ_CHARS,
    })
}

/// 检查「允许读取的文件夹」能不能用（设置保存时）
pub fn validate_read_dir(dir: &str, data_dir: &Path) -> Result<String, String> {
    let p = PathBuf::from(dir.trim());
    if !p.is_absolute() {
        return Err(format!("「{dir}」不是完整路径，比如 D:\\笔记"));
    }
    let canon =
        sandbox::simplify(&std::fs::canonicalize(&p).map_err(|_| format!("找不到文件夹 {dir}"))?);
    if !canon.is_dir() {
        return Err(format!("{dir} 不是文件夹"));
    }
    if sandbox::is_within(data_dir, &canon) || sandbox::is_within(&canon, data_dir) {
        return Err("不能允许读取 DeskMate 自己的数据文件夹".into());
    }
    Ok(canon.display().to_string())
}

/// 聊天气泡里的一个动作按钮
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionView {
    pub kind: MarkerKind,
    /// 角色写的原文（点按钮时原样传回来，后端重新检查）
    pub target: String,
    /// 按钮旁边的说明
    pub label: String,
    /// 不能做的原因（有原因时界面不显示按钮，只显示原因）
    pub error: Option<String>,
}

/// 把回复里的打开 / 读取标记变成按钮（任务标记不在这里处理）
pub fn action_view(m: &Marker, adv: &AdvancedSettings) -> Option<ActionView> {
    let (label, error) = match m.kind {
        MarkerKind::Task => return None,
        MarkerKind::Open => match resolve_open(&m.text, &adv.programs) {
            Ok(t) => (format!("打开{}", t.describe()), None),
            Err(e) => (format!("打开 {}", m.text), Some(e)),
        },
        MarkerKind::Read => match check_read(&m.text, &adv.read_dirs) {
            Ok(p) => (format!("读取文件 {}", p.display()), None),
            Err(e) => (format!("读取 {}", m.text), Some(e)),
        },
    };
    Some(ActionView {
        kind: m.kind,
        target: m.text.clone(),
        label,
        error,
    })
}

// ---------------- 系统信息 ----------------

/// 电量描述，比如「电量 45%（没在充电）」；台式机或查不到时返回 None
#[cfg(windows)]
pub fn battery_text() -> Option<String> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut s) } == 0 {
        return None;
    }
    battery_desc(s.ACLineStatus, s.BatteryFlag, s.BatteryLifePercent)
}

#[cfg(not(windows))]
pub fn battery_text() -> Option<String> {
    None
}

/// 纯函数：AC 状态（1 = 接着电源）、电池标志（128 = 没有电池，255 = 未知）、电量百分比（255 = 未知）
pub fn battery_desc(ac: u8, flag: u8, percent: u8) -> Option<String> {
    if flag == 128 || flag == 255 || percent > 100 {
        return None;
    }
    let charging = if ac == 1 {
        "接着电源"
    } else {
        "没在充电"
    };
    Some(format!("电量 {percent}%（{charging}）"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_only_http() {
        assert!(check_url("https://www.bilibili.com/").is_ok());
        assert!(check_url("http://example.com/a?b=1").is_ok());
        assert!(check_url("file:///C:/Windows").is_err());
        assert!(check_url("javascript:alert(1)").is_err());
        assert!(check_url("https://exa mple.com").is_err());
        assert!(check_url("https://user@evil.com").is_err());
        assert!(check_url("https://").is_err());
        assert_eq!(
            resolve_open("「https://example.com」", &[]),
            Ok(OpenTarget::Url {
                url: "https://example.com".into()
            })
        );
        assert!(resolve_open("ms-settings:privacy", &[]).is_err());
    }

    #[test]
    fn open_folders_and_documents_but_not_programs() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("作业.docx");
        let exe = dir.path().join("evil.exe");
        let bat = dir.path().join("run.BAT");
        for f in [&doc, &exe, &bat] {
            std::fs::write(f, b"x").unwrap();
        }
        assert!(matches!(
            resolve_open(&dir.path().display().to_string(), &[]),
            Ok(OpenTarget::Folder { .. })
        ));
        assert!(matches!(
            resolve_open(&doc.display().to_string(), &[]),
            Ok(OpenTarget::File { .. })
        ));
        assert!(resolve_open(&exe.display().to_string(), &[]).is_err());
        assert!(resolve_open(&bat.display().to_string(), &[]).is_err());
        assert!(resolve_open("作业.docx", &[]).is_err(), "相对路径不行");
        assert!(resolve_open("D:\\不存在\\x.txt", &[]).is_err());
        // 登记过的程序可以按名字打开
        let progs = vec![ProgramEntry {
            name: "记事本".into(),
            path: exe.display().to_string(),
        }];
        assert!(matches!(
            resolve_open("记事本", &progs),
            Ok(OpenTarget::Program { .. })
        ));
        assert!(resolve_open("计算器", &progs).is_err());
    }

    #[test]
    fn read_only_inside_allowed_dirs() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("笔记");
        let other = root.path().join("别的");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let a = allowed.join("a.txt");
        std::fs::write(&a, "第一行\n第二行").unwrap();
        let b = other.join("b.txt");
        std::fs::write(&b, "秘密").unwrap();
        let bin = allowed.join("c.exe");
        std::fs::write(&bin, "MZ").unwrap();
        let dirs = vec![allowed.display().to_string()];
        let r = read_text(&a.display().to_string(), &dirs).unwrap();
        assert_eq!(r.name, "a.txt");
        assert_eq!(r.text, "第一行\n第二行");
        assert!(!r.truncated);
        assert!(read_text(&b.display().to_string(), &dirs).is_err());
        assert!(read_text(&bin.display().to_string(), &dirs).is_err());
        assert!(read_text(&a.display().to_string(), &[]).is_err());
        // 用 .. 绕出去也不行
        let sneaky = format!("{}\\..\\别的\\b.txt", allowed.display());
        assert!(read_text(&sneaky, &dirs).is_err());
        // 太长的截断
        let long = allowed.join("long.md");
        std::fs::write(&long, "字".repeat(7000)).unwrap();
        let r = read_text(&long.display().to_string(), &dirs).unwrap();
        assert!(r.truncated);
        assert_eq!(r.text.chars().count(), 6000);
    }

    #[test]
    fn decodes_utf8_bom_and_utf16() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFhi").as_deref(), Some("hi"));
        let mut u16le = vec![0xFF, 0xFE];
        for u in "你好".encode_utf16() {
            u16le.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_text(&u16le).as_deref(), Some("你好"));
        // GBK 的「你好」
        assert_eq!(
            decode_text(&[0xC4, 0xE3, 0xBA, 0xC3, 0x41, 0x42, 0x43, 0x44, 0x45]),
            None
        );
        // 截断在一个字中间：去掉半个字
        let s = "你好".as_bytes();
        assert_eq!(decode_text(&s[..4]).as_deref(), Some("你"));
    }

    #[test]
    fn read_dir_validation() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("DeskMate");
        std::fs::create_dir_all(&data).unwrap();
        assert!(validate_read_dir(&root.path().join("笔记").display().to_string(), &data).is_err());
        std::fs::create_dir_all(root.path().join("笔记")).unwrap();
        assert!(validate_read_dir(&root.path().join("笔记").display().to_string(), &data).is_ok());
        assert!(validate_read_dir(&data.display().to_string(), &data).is_err());
        assert!(
            validate_read_dir(&root.path().display().to_string(), &data).is_err(),
            "包含数据目录"
        );
        assert!(validate_read_dir("笔记", &data).is_err());
    }

    #[test]
    fn battery_description() {
        assert_eq!(
            battery_desc(0, 0, 45).as_deref(),
            Some("电量 45%（没在充电）")
        );
        assert_eq!(
            battery_desc(1, 8, 80).as_deref(),
            Some("电量 80%（接着电源）")
        );
        assert_eq!(battery_desc(1, 128, 255), None, "台式机");
        assert_eq!(battery_desc(1, 0, 255), None);
    }

    #[test]
    fn settings_sanitize() {
        let s = AdvancedSettings {
            allow_delegation: false,
            read_dirs: vec![" D:\\a ".into(), "d:\\A".into(), "".into()],
            programs: vec![
                ProgramEntry {
                    name: " 记事本 ".into(),
                    path: "\"C:\\Windows\\notepad.exe\"".into(),
                },
                ProgramEntry {
                    name: "记事本".into(),
                    path: "x".into(),
                },
                ProgramEntry {
                    name: "".into(),
                    path: "y".into(),
                },
            ],
        }
        .sanitize();
        assert_eq!(s.read_dirs, vec!["D:\\a"]);
        assert_eq!(s.programs.len(), 1);
        assert_eq!(s.programs[0].path, "C:\\Windows\\notepad.exe");
        assert!(
            !AdvancedSettings::default().allow_delegation,
            "默认不允许委托"
        );
    }
}
