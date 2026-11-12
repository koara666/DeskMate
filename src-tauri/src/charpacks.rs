//! 角色按需下载：用户挑哪位就下载哪位（想要更多角色时软件本体也不会变大）。只有初始角色（登记表里 bundled: true，七海千秋）
//! 的角色包随安装包带着，全新安装时直接装上（`install_bundled`），装好以后和下载来的一样：能删、有新版本会自动更新。
//!
//! - 目录：`scripts/builtin_characters.json` 是唯一的登记表，编译时嵌进程序——角色名、简介、
//!   下载地址的版本和 SHA-256 都在里面，所以没下载之前也能列出来，下载的内容必须和登记的校验值一致。
//! - 每位角色一个 `tar.bz2`，放在 GitHub 发布页 `character-<短名>-v<N>`，里面是角色的整个文件夹
//!   （设定、台词、Q 版形象、预录语音），格式和以前随安装包的一样，装好后放在数据目录的 `packs/<id>/`。
//! - 解压只认白名单里的文件名和类型，不信压缩包里的任何路径；先解到临时文件夹、校验通过再整个换进去。
//! - 升级前用着某个角色的用户，启动时在后台把 TA 下好（见 [`startup_plan`]）；全新安装的人先去设置里挑。

use crate::character;
use crate::desktop;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

type CmdResult<T> = Result<T, String>;

const BASE: &str = "https://github.com/koara666/DeskMate/releases/download";
const REGISTRY: &str = include_str!("../../scripts/builtin_characters.json");
/// 压缩包（下载下来的）最大多大
const MAX_PACK_BYTES: u64 = 64 * 1024 * 1024;
/// 解压后一共最大多大 / 单个文件最大多大 / 最多多少个文件
const MAX_UNPACKED_BYTES: u64 = 96 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 1500;
/// 装好的角色文件夹里记版本和校验值的文件
const MARKER: &str = ".pack.json";
/// 安装包里的小预览图（还没下载时选角色用）：`characters/_catalog/<id>.png`
const MAX_PREVIEW_BYTES: u64 = 512 * 1024;

// ---------------- 目录（登记表） ----------------

/// 登记表里的一位可下载角色
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    /// 发布页标签里用的短名
    pub short: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub order: i64,
    pub version: u32,
    pub sha256: String,
    pub size: u64,
    /// 初始角色：构建时把角色包放进安装包，全新安装时直接装上（见 `install_bundled`）
    pub bundled: bool,
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(default)]
    label: String,
    short: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    order: i64,
    #[serde(default)]
    bundled: bool,
    pack: Option<RawPack>,
}

#[derive(Deserialize)]
struct RawPack {
    version: u32,
    sha256: String,
    size: u64,
}

fn parse_registry(text: &str) -> Vec<Entry> {
    let Ok(map) =
        serde_json::from_str::<std::collections::BTreeMap<String, serde_json::Value>>(text)
    else {
        return Vec::new();
    };
    let mut list: Vec<Entry> = map
        .into_iter()
        .filter(|(k, _)| !k.starts_with('_'))
        .filter_map(|(id, v)| {
            let r: RawEntry = serde_json::from_value(v).ok()?;
            let p = r.pack?;
            Some(Entry {
                name: if r.name.is_empty() { r.label } else { r.name },
                id,
                short: r.short,
                description: r.description,
                color: r.color,
                order: r.order,
                version: p.version,
                sha256: p.sha256,
                size: p.size,
                bundled: r.bundled,
            })
        })
        .collect();
    list.sort_by(|a, b| (a.order, &a.id).cmp(&(b.order, &b.id)));
    list
}

/// 可下载的角色（登记表里写了角色包的）
pub fn catalog() -> &'static [Entry] {
    static CATALOG: OnceLock<Vec<Entry>> = OnceLock::new();
    CATALOG.get_or_init(|| parse_registry(REGISTRY))
}

fn entry(id: &str) -> Option<&'static Entry> {
    catalog().iter().find(|e| e.id == id)
}

/// 下载地址的前缀。测试实例可以用 `DESKMATE_PACK_URL_BASE` 指到本机的假服务器（正式使用时这个变量不起作用）
fn base_url() -> String {
    if crate::paths::test_mode()
        && let Ok(b) = std::env::var("DESKMATE_PACK_URL_BASE")
        && b.starts_with("http://127.0.0.1")
    {
        return b.trim_end_matches('/').to_string();
    }
    BASE.to_string()
}

fn pack_url(e: &Entry) -> String {
    format!(
        "{}/character-{}-v{}/{}.tar.bz2",
        base_url(),
        e.short,
        e.version,
        e.id
    )
}

fn url_allowed(url: &str) -> bool {
    url.starts_with("https://")
        || (crate::paths::test_mode() && url.starts_with("http://127.0.0.1"))
}

// ---------------- 已装的 ----------------

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Marker {
    version: u32,
    sha256: String,
}

fn pack_dir(st: &AppState, id: &str) -> PathBuf {
    st.paths.packs().join(id)
}

/// 已经装好的版本（没装 / 装坏了返回 None）
fn installed_version(st: &AppState, id: &str) -> Option<u32> {
    installed_version_in(&st.paths.packs(), id)
}

fn installed_version_in(packs: &Path, id: &str) -> Option<u32> {
    let dir = packs.join(id);
    let raw = character::read_limited(&dir.join(MARKER), 4096).ok()?;
    let m: Marker = serde_json::from_slice(&raw).ok()?;
    dir.join("character.json").is_file().then_some(m.version)
}

// ---------------- 下载进度 ----------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    pub id: String,
    pub running: bool,
    pub done: u64,
    pub total: u64,
    pub error: String,
}

#[derive(Default)]
struct Job {
    progress: Progress,
    cancel: Option<CancellationToken>,
}

/// 各角色的下载任务（同一位角色同一时间只下一次）
#[derive(Default)]
pub struct PackState {
    jobs: Mutex<HashMap<String, Job>>,
}

impl PackState {
    fn jobs(&self) -> MutexGuard<'_, HashMap<String, Job>> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn progress(&self, id: &str) -> Progress {
        self.jobs()
            .get(id)
            .map(|j| j.progress.clone())
            .unwrap_or_else(|| Progress {
                id: id.to_string(),
                ..Default::default()
            })
    }
}

fn emit_progress(app: &AppHandle, p: Progress) {
    let _ = app.emit("charpack-progress", p);
}

// ---------------- 给设置页看的 ----------------

#[derive(Debug, Serialize)]
pub struct PackView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub color: String,
    /// 下载大小（字节）
    pub size: u64,
    /// 安装包自带的小预览图（data URL），没有就是空
    pub preview: String,
    pub installed: bool,
    pub installed_version: u32,
    pub latest_version: u32,
    /// 装的是旧版本，有新的可以更新
    pub update: bool,
    pub running: bool,
    pub done: u64,
    pub total: u64,
    pub error: String,
}

fn preview(st: &AppState, id: &str) -> String {
    use base64::Engine;
    let Some(dir) = st.builtin_characters_dir.as_ref() else {
        return String::new();
    };
    match character::read_limited(
        &dir.join("_catalog").join(format!("{id}.png")),
        MAX_PREVIEW_BYTES,
    ) {
        Ok(b) if character::sniff_image(&b) == Some("image/png") => format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        ),
        _ => String::new(),
    }
}

pub fn list(st: &AppState) -> Vec<PackView> {
    catalog()
        .iter()
        .map(|e| {
            let have = installed_version(st, &e.id);
            let p = st.packs.progress(&e.id);
            PackView {
                id: e.id.clone(),
                name: e.name.clone(),
                description: e.description.clone(),
                color: e.color.clone(),
                size: e.size,
                preview: preview(st, &e.id),
                installed: have.is_some(),
                installed_version: have.unwrap_or(0),
                latest_version: e.version,
                update: have.is_some_and(|v| v < e.version),
                running: p.running,
                done: p.done,
                total: if p.total > 0 { p.total } else { e.size },
                error: p.error,
            }
        })
        .collect()
}

// ---------------- 下载 ----------------

/// 开始下载（已经在下了就什么也不做）。进度通过 `charpack-progress` 事件发给设置窗口。
pub fn start(app: &AppHandle, id: &str) -> Result<(), String> {
    let e = entry(id).ok_or("找不到这个角色")?.clone();
    let st = app.state::<AppState>();
    let cancel = CancellationToken::new();
    {
        let mut jobs = st.packs.jobs();
        if jobs.get(id).is_some_and(|j| j.progress.running) {
            return Ok(());
        }
        jobs.insert(
            id.to_string(),
            Job {
                progress: Progress {
                    id: id.to_string(),
                    running: true,
                    total: e.size,
                    ..Default::default()
                },
                cancel: Some(cancel.clone()),
            },
        );
    }
    emit_progress(app, st.packs.progress(id));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = run(&app, &e, &cancel).await;
        let st = app.state::<AppState>();
        {
            let mut jobs = st.packs.jobs();
            if let Some(j) = jobs.get_mut(&e.id) {
                j.progress.running = false;
                j.cancel = None;
                match &result {
                    Ok(()) => {
                        j.progress.done = j.progress.total;
                        j.progress.error.clear();
                    }
                    Err(m) => j.progress.error = m.clone(),
                }
            }
        }
        match &result {
            Ok(()) => log::info!("角色 {} 下载安装完成（v{}）", e.id, e.version),
            Err(m) => log::warn!("角色 {} 下载失败：{m}", e.id),
        }
        emit_progress(&app, st.packs.progress(&e.id));
    });
    Ok(())
}

/// 取消正在进行的下载
pub fn cancel(st: &AppState, id: &str) {
    if let Some(c) = st.packs.jobs().get(id).and_then(|j| j.cancel.clone()) {
        c.cancel();
    }
}

async fn run(app: &AppHandle, e: &Entry, cancel: &CancellationToken) -> Result<(), String> {
    let st = app.state::<AppState>();
    let work = st.paths.packs().join(format!(".dl-{}", e.id));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("创建文件夹失败：{e}"))?;
    let result = download_and_install(app, &st, e, &work, cancel).await;
    let _ = std::fs::remove_dir_all(&work);
    result?;
    st.reload_characters();
    // 当前选的角色还没装好（全新安装第一次挑、或升级后等着下载的那位）：下好就直接用上，不用再点一次「切换」
    let wanted = st.settings().character_id;
    if !st.characters().iter().any(|c| c.manifest.id == wanted) {
        desktop::select_character(app, &e.id);
    } else {
        desktop::notify_character_changed(app);
    }
    Ok(())
}

async fn download_and_install(
    app: &AppHandle,
    st: &AppState,
    e: &Entry,
    work: &Path,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let archive = work.join("pack.tar.bz2");
    let url = pack_url(e);
    if !url_allowed(&url) {
        return Err("下载地址不安全".into());
    }
    let (app2, id) = (app.clone(), e.id.clone());
    fetch(&st.http, &url, &e.sha256, &archive, cancel, move |done| {
        let st = app2.state::<AppState>();
        let snapshot = {
            let mut jobs = st.packs.jobs();
            match jobs.get_mut(&id) {
                Some(j) => {
                    j.progress.done = done;
                    j.progress.clone()
                }
                None => return,
            }
        };
        emit_progress(&app2, snapshot);
    })
    .await?;
    let (e2, work2) = (e.clone(), work.to_path_buf());
    let packs = st.paths.packs();
    tauri::async_runtime::spawn_blocking(move || install(&packs, &e2, &work2))
        .await
        .map_err(|e| e.to_string())?
}

/// 下载到 `dest`（先写临时文件，校验通过再改名）。`on_progress` 收到已下载的字节数。
async fn fetch(
    client: &reqwest::Client,
    url: &str,
    sha256: &str,
    dest: &Path,
    cancel: &CancellationToken,
    mut on_progress: impl FnMut(u64),
) -> Result<(), String> {
    use futures_util::StreamExt;
    let net = |e: reqwest::Error| {
        format!(
            "下载失败（{}）。检查网络（需要能访问 GitHub，开着代理的话确认它在运行），然后点「重试」",
            crate::redact::redact_secrets(&e.to_string())
        )
    };
    let resp = tokio::select! {
        _ = cancel.cancelled() => return Err("已取消".into()),
        r = client.get(url).send() => r.map_err(net)?,
    };
    if resp.status().as_u16() == 404 {
        return Err("下载地址上还没有这个角色包（404）。过一会儿再试，或者更新到最新版本".into());
    }
    let resp = resp.error_for_status().map_err(net)?;
    if resp.content_length().is_some_and(|n| n > MAX_PACK_BYTES) {
        return Err("角色包比预期大得多，已停止".into());
    }
    let part = dest.with_extension("part");
    let mut file = std::fs::File::create(&part).map_err(|e| format!("写文件失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut last = Instant::now();
    let mut stream = resp.bytes_stream();
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err("已取消".into());
            }
            c = stream.next() => c,
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.map_err(net)?;
        done += chunk.len() as u64;
        if done > MAX_PACK_BYTES {
            return Err("角色包比预期大得多，已停止".into());
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| format!("写文件失败（磁盘满了？）：{e}"))?;
        if last.elapsed() > Duration::from_millis(200) {
            on_progress(done);
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != sha256 {
        let _ = std::fs::remove_file(&part);
        return Err("下载下来的文件校验没通过（可能被网络环境改动了），点「重试」".into());
    }
    std::fs::rename(&part, dest).map_err(|e| format!("保存文件失败：{e}"))
}

// ---------------- 解压与安装 ----------------

fn file_name_ok(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 80
        && !n.starts_with('.')
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// 压缩包里的一个条目（`<id>/…` 之后的部分）是不是该有的文件
fn entry_allowed(parts: &[&str]) -> bool {
    match parts {
        [f] => matches!(
            *f,
            "character.json" | "persona.md" | "canon.md" | "quotes.md"
        ),
        ["lines", n] => file_name_ok(n) && n.ends_with(".md"),
        ["assets", n] => {
            file_name_ok(n)
                && n.rsplit('.')
                    .next()
                    .is_some_and(|x| matches!(x, "png" | "webp" | "jpg" | "jpeg" | "gif" | "svg"))
        }
        ["voice", "index.json"] => true,
        ["voice", n] => n
            .strip_suffix(".opus")
            .is_some_and(|h| h.len() == 32 && h.chars().all(|c| c.is_ascii_hexdigit())),
        _ => false,
    }
}

/// 解压到 `dest`（里面会有 `<id>/`）。任何一个条目不对就整个失败。
fn extract(archive: &Path, id: &str, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(f));
    let bad = |what: &str| format!("角色包里有不认识的内容（{what}），已停止");
    let (mut count, mut total) = (0usize, 0u64);
    for entry in tar.entries().map_err(|e| format!("解压失败：{e}"))? {
        let mut entry = entry.map_err(|e| format!("解压失败：{e}"))?;
        let kind = entry.header().entry_type();
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let shown = path.display().to_string();
        let comps: Vec<String> = path
            .components()
            .map(|c| match c {
                std::path::Component::Normal(s) => Ok(s.to_string_lossy().into_owned()),
                // 目录条目可能是 "./"
                std::path::Component::CurDir => Ok(String::new()),
                _ => Err(bad(&shown)),
            })
            .collect::<Result<_, _>>()?;
        let comps: Vec<&str> = comps
            .iter()
            .map(String::as_str)
            .filter(|c| !c.is_empty())
            .collect();
        if kind.is_dir() {
            if comps.first().is_some_and(|c| *c != id) {
                return Err(bad(&shown));
            }
            continue;
        }
        if !kind.is_file() {
            return Err(bad(&shown));
        }
        let Some((first, rest)) = comps.split_first() else {
            return Err(bad(&shown));
        };
        if *first != id || !entry_allowed(rest) {
            return Err(bad(&shown));
        }
        count += 1;
        let size = entry.size();
        total += size;
        if count > MAX_ENTRIES || size > MAX_ENTRY_BYTES || total > MAX_UNPACKED_BYTES {
            return Err("角色包比预期大得多，已停止".into());
        }
        let target = comps.iter().fold(dest.to_path_buf(), |p, c| p.join(c));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建文件夹失败：{e}"))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| format!("写文件失败：{e}"))?;
        // 声明的大小可能是假的：按实际读到的算
        let copied = std::io::copy(&mut (&mut entry).take(MAX_ENTRY_BYTES + 1), &mut out)
            .map_err(|e| format!("解压失败（磁盘满了？）：{e}"))?;
        if copied > MAX_ENTRY_BYTES {
            return Err("角色包比预期大得多，已停止".into());
        }
    }
    if count == 0 {
        return Err("角色包是空的".into());
    }
    Ok(())
}

/// 把下载好的压缩包解压、校验、整个换进 `packs/<id>/`
fn install(packs: &Path, e: &Entry, work: &Path) -> Result<(), String> {
    let stage = work.join("stage");
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    extract(&work.join("pack.tar.bz2"), &e.id, &stage)?;
    let staged = stage.join(&e.id);
    // 能不能当成角色用：设定文件格式对、id 和文件夹名一致
    character::load_pack(&staged).map_err(|err| format!("角色包里的设定文件不对：{err}"))?;
    let marker = serde_json::to_vec(&Marker {
        version: e.version,
        sha256: e.sha256.clone(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(staged.join(MARKER), marker).map_err(|e| e.to_string())?;

    let target = packs.join(&e.id);
    let old = packs.join(format!(".old-{}", e.id));
    let _ = std::fs::remove_dir_all(&old);
    let had_old = target.exists();
    if had_old {
        std::fs::rename(&target, &old)
            .map_err(|e| format!("替换旧版本失败（文件被占用？）：{e}"))?;
    }
    if let Err(err) = std::fs::rename(&staged, &target) {
        if had_old {
            let _ = std::fs::rename(&old, &target);
        }
        return Err(format!("安装失败：{err}"));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

// ---------------- 安装包里带的初始角色 ----------------

/// 初始角色（登记表里 `bundled: true` 的那位，现在是七海千秋）
pub fn bundled_entry() -> Option<&'static Entry> {
    catalog().iter().find(|e| e.bundled)
}

/// 安装包里那份角色包放在哪：`<内置角色目录>/_bundled/<id>.tar.bz2`（构建时由 scripts/chibi/bundle_default_pack.py 放进去）
fn bundled_file(builtin: &Path, e: &Entry) -> PathBuf {
    builtin.join("_bundled").join(format!("{}.tar.bz2", e.id))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// 从本机的一个角色包文件装进 `packs/<id>/`：校验值要和登记表里的一样，之后和下载来的完全一样走 `install`
/// （所以装好以后照样能在设置里删掉、有新版本照样自动更新）
fn install_local(packs: &Path, e: &Entry, archive: &Path) -> Result<(), String> {
    let size = std::fs::metadata(archive)
        .map_err(|_| "安装包里没有初始角色的角色包".to_string())?
        .len();
    if size > MAX_PACK_BYTES {
        return Err("初始角色的角色包大得不正常".into());
    }
    if sha256_file(archive)? != e.sha256 {
        return Err("初始角色的角色包校验没通过".into());
    }
    let work = packs.join(format!(".bundled-{}", e.id));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("创建文件夹失败：{e}"))?;
    let result = std::fs::copy(archive, work.join("pack.tar.bz2"))
        .map_err(|e| format!("复制角色包失败：{e}"))
        .and_then(|_| install(packs, e, &work));
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// 全新安装时：把安装包里带的初始角色装上并选上它，打开就能看到桌宠（之后换角色、删掉都随用户）。
/// 返回装上的角色 id；失败（开发版没放角色包、文件坏了）时调用方照旧打开「设置 → 角色」让用户自己挑。
pub fn install_bundled(st: &AppState) -> Result<String, String> {
    let e = bundled_entry().ok_or("登记表里没有初始角色")?;
    let builtin = st
        .builtin_characters_dir
        .as_ref()
        .ok_or("找不到安装目录里的角色文件夹")?;
    let packs = st.paths.packs();
    std::fs::create_dir_all(&packs).map_err(|e| format!("创建文件夹失败：{e}"))?;
    install_local(&packs, e, &bundled_file(builtin, e))?;
    st.reload_characters();
    st.update_settings(|s| s.character_id = e.id.clone());
    Ok(e.id.clone())
}

/// 删掉下载来的角色（释放磁盘）。正在用的不能删。
pub fn remove(app: &AppHandle, id: &str) -> Result<(), String> {
    let st = app.state::<AppState>();
    if entry(id).is_none() {
        return Err("找不到这个角色".into());
    }
    if st.current_character().is_some_and(|c| c.manifest.id == id) {
        return Err("正在使用的角色不能删除，先换成别的角色".into());
    }
    if st.packs.progress(id).running {
        return Err("正在下载，先取消".into());
    }
    let dir = pack_dir(&st, id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删除失败：{e}"))?;
    }
    st.reload_characters();
    desktop::notify_character_changed(app);
    Ok(())
}

// ---------------- 启动时 ----------------

/// 启动时要不要下载（纯函数，方便测试）
#[derive(Debug, PartialEq, Eq)]
pub enum Startup {
    /// 什么也不用做
    Nothing,
    /// 全新安装、还没有任何角色：打开设置的角色页让用户挑
    Choose,
    /// 这些角色要在后台下载（升级前用着的角色，或已装角色有新版本）
    Download(Vec<String>),
}

pub fn startup_plan(fresh: bool, current: &str, installed: &[(String, u32)]) -> Startup {
    let mut want: Vec<String> = Vec::new();
    for e in catalog() {
        let have = installed.iter().find(|(id, _)| *id == e.id).map(|x| x.1);
        match have {
            Some(v) if v < e.version => want.push(e.id.clone()),
            None if !fresh && e.id == current => want.push(e.id.clone()),
            _ => {}
        }
    }
    if !want.is_empty() {
        return Startup::Download(want);
    }
    if fresh && installed.is_empty() {
        Startup::Choose
    } else {
        Startup::Nothing
    }
}

/// 程序启动后调用一次。`fresh` = 第一次启动（用户还没做过任何选择）。
pub fn on_startup(app: &AppHandle, fresh: bool) {
    let st = app.state::<AppState>();
    let installed: Vec<(String, u32)> = catalog()
        .iter()
        .filter_map(|e| Some((e.id.clone(), installed_version(&st, &e.id)?)))
        .collect();
    let plan = startup_plan(fresh, &st.settings().character_id, &installed);
    log::info!("角色包：已装 {installed:?}，启动安排 {plan:?}");
    match plan {
        Startup::Nothing => {}
        Startup::Choose => desktop::open_settings_tab_async(app, Some("characters".into())),
        Startup::Download(ids) => {
            for id in ids {
                if let Err(e) = start(app, &id) {
                    log::warn!("启动下载 {id} 失败：{e}");
                }
            }
        }
    }
}

// ---------------- 命令 ----------------

#[tauri::command]
pub fn charpack_list(state: State<'_, AppState>) -> Vec<PackView> {
    list(&state)
}

#[tauri::command]
pub fn charpack_download(app: AppHandle, id: String) -> CmdResult<()> {
    start(&app, &id)
}

#[tauri::command]
pub fn charpack_cancel(state: State<'_, AppState>, id: String) {
    cancel(&state, &id);
}

#[tauri::command]
pub fn charpack_remove(app: AppHandle, id: String) -> CmdResult<()> {
    remove(&app, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn pack_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let enc = bzip2::write::BzEncoder::new(&mut out, bzip2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            for (name, data) in files {
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o644);
                h.set_entry_type(tar::EntryType::Regular);
                // 绕过 tar 库自己的路径检查，测试恶意路径
                h.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
                h.set_cksum();
                b.append(&h, *data).unwrap();
            }
            b.into_inner().unwrap().finish().unwrap();
        }
        out
    }

    fn write_pack(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        let p = dir.join("pack.tar.bz2");
        std::fs::File::create(&p)
            .unwrap()
            .write_all(&pack_bytes(files))
            .unwrap();
        p
    }

    const MANIFEST: &[u8] = r#"{"id":"cat","name":"猫"}"#.as_bytes();

    #[test]
    fn registry_entries_are_downloadable_and_named_by_the_shared_rule() {
        assert!(!catalog().is_empty());
        for e in catalog() {
            assert!(character::valid_id(&e.id), "{}", e.id);
            assert!(
                e.version >= 1 && e.size > 0 && e.size < MAX_PACK_BYTES,
                "{}",
                e.id
            );
            assert_eq!(e.sha256.len(), 64, "{}", e.id);
            assert!(!e.name.is_empty() && !e.description.is_empty(), "{}", e.id);
            let url = pack_url(e);
            assert!(
                url.starts_with(
                    "https://github.com/koara666/DeskMate/releases/download/character-"
                )
            );
            // 标签不能以 v 加数字开头，不然会被当成软件版本触发发布
            assert!(!url.contains("/download/v"), "{url}");
        }
    }

    #[test]
    fn only_expected_files_are_allowed() {
        let hex = "0123456789abcdef0123456789abcdef";
        for ok in [
            vec!["character.json"],
            vec!["persona.md"],
            vec!["lines", "voice.md"],
            vec!["assets", "idle.png"],
            vec!["assets", "idle_night.webp"],
            vec!["voice", "index.json"],
        ] {
            assert!(entry_allowed(&ok), "{ok:?}");
        }
        assert!(entry_allowed(&["voice", &format!("{hex}.opus")]));
        for bad in [
            vec![],
            vec!["deskmate.exe"],
            vec!["lines", "x.exe"],
            vec!["lines", "..", "x.md"],
            vec!["assets", "run.bat"],
            vec!["assets", ".hidden.png"],
            vec!["assets", "sub", "a.png"],
            vec!["voice", "short.opus"],
            vec!["voice", "index.json", "x"],
            vec!["README.md"],
            vec![".pack.json"],
        ] {
            assert!(!entry_allowed(&bad), "{bad:?}");
        }
    }

    #[test]
    fn extracts_a_good_pack() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_pack(
            dir.path(),
            &[
                ("cat/character.json", MANIFEST),
                ("cat/persona.md", b"hi"),
                ("cat/lines/a.md", b"## x\n- y"),
                ("cat/assets/idle.png", b"\x89PNG\r\n\x1a\nxx"),
            ],
        );
        let out = dir.path().join("out");
        extract(&a, "cat", &out).unwrap();
        assert!(out.join("cat/character.json").is_file());
        assert!(out.join("cat/lines/a.md").is_file());
        assert!(out.join("cat/assets/idle.png").is_file());
    }

    #[test]
    fn rejects_bad_entries() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        for files in [
            // 路径跳出去
            vec![("cat/../evil.md", &b"x"[..])],
            // 绝对路径
            vec![("/etc/passwd", &b"x"[..])],
            // 别的角色的文件夹
            vec![("dog/character.json", MANIFEST)],
            // 不该有的文件
            vec![("cat/character.json", MANIFEST), ("cat/run.exe", b"MZ")],
            vec![("cat/assets/x.dll", b"MZ")],
            // 没有任何文件
            vec![],
        ] {
            let a = write_pack(dir.path(), &files);
            assert!(extract(&a, "cat", &out).is_err(), "{files:?}");
        }
    }

    #[test]
    fn rejects_links_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        // 符号链接
        let mut bytes = Vec::new();
        {
            let enc = bzip2::write::BzEncoder::new(&mut bytes, bzip2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            h.set_cksum();
            b.append_link(&mut h, "cat/assets/idle.png", "C:/Windows/win.ini")
                .unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let p = dir.path().join("link.tar.bz2");
        std::fs::write(&p, bytes).unwrap();
        assert!(extract(&p, "cat", &out).is_err());
        // 单个文件太大
        let big = vec![0u8; (MAX_ENTRY_BYTES + 1) as usize];
        let a = write_pack(dir.path(), &[("cat/assets/idle.png", &big)]);
        assert!(extract(&a, "cat", &out).is_err());
    }

    fn entry_for(id: &str) -> Entry {
        Entry {
            id: id.into(),
            short: "cat".into(),
            name: "猫".into(),
            description: "d".into(),
            color: String::new(),
            order: 1,
            version: 3,
            sha256: "0".repeat(64),
            size: 10,
            bundled: false,
        }
    }

    #[test]
    fn exactly_one_bundled_starter_and_it_is_the_default_character() {
        let starters: Vec<&Entry> = catalog().iter().filter(|e| e.bundled).collect();
        assert_eq!(starters.len(), 1, "安装包里只带一位初始角色");
        assert_eq!(
            starters[0].id,
            crate::settings::Settings::default().character_id,
            "初始角色就是默认选着的那位"
        );
        assert_eq!(
            bundled_entry().map(|e| e.id.as_str()),
            Some("nanami-chiaki")
        );
    }

    #[test]
    fn bundled_pack_installs_like_a_download_and_rejects_a_bad_file() {
        let dir = tempfile::tempdir().unwrap();
        let packs = dir.path().join("packs");
        std::fs::create_dir_all(&packs).unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let archive = write_pack(
            &src,
            &[("cat/character.json", MANIFEST), ("cat/persona.md", b"hi")],
        );
        let mut e = entry_for("cat");
        // 校验值对不上：不装
        assert!(install_local(&packs, &e, &archive).is_err());
        assert!(!packs.join("cat").exists());
        // 文件不在（开发版没放）：不装
        assert!(install_local(&packs, &e, &src.join("missing.tar.bz2")).is_err());
        // 对得上：装进 packs/cat，和下载来的一样有版本记号，临时文件夹清掉
        e.sha256 = sha256_file(&archive).unwrap();
        install_local(&packs, &e, &archive).unwrap();
        assert_eq!(
            std::fs::read_to_string(packs.join("cat/persona.md")).unwrap(),
            "hi"
        );
        assert_eq!(installed_version_in(&packs, "cat"), Some(3));
        assert!(!packs.join(".bundled-cat").exists());
        assert!(archive.exists(), "安装包里的原文件不动");
    }

    #[test]
    fn install_swaps_in_a_valid_pack_and_keeps_the_old_one_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let packs = dir.path().join("packs");
        std::fs::create_dir_all(&packs).unwrap();
        let e = entry_for("cat");

        let work = dir.path().join("w1");
        std::fs::create_dir_all(&work).unwrap();
        write_pack(
            &work,
            &[("cat/character.json", MANIFEST), ("cat/persona.md", b"v1")],
        );
        install(&packs, &e, &work).unwrap();
        assert_eq!(
            std::fs::read_to_string(packs.join("cat/persona.md")).unwrap(),
            "v1"
        );
        let m: Marker =
            serde_json::from_slice(&std::fs::read(packs.join("cat").join(MARKER)).unwrap())
                .unwrap();
        assert_eq!(m.version, 3);

        // 新包的设定文件不对（id 和文件夹名对不上）：安装失败，旧的原样留着
        let work2 = dir.path().join("w2");
        std::fs::create_dir_all(&work2).unwrap();
        write_pack(
            &work2,
            &[
                (
                    "cat/character.json",
                    r#"{"id":"dog","name":"狗"}"#.as_bytes(),
                ),
                ("cat/persona.md", b"v2"),
            ],
        );
        assert!(install(&packs, &e, &work2).is_err());
        assert_eq!(
            std::fs::read_to_string(packs.join("cat/persona.md")).unwrap(),
            "v1"
        );

        // 正常的新版本整个换掉旧的
        let work3 = dir.path().join("w3");
        std::fs::create_dir_all(&work3).unwrap();
        write_pack(
            &work3,
            &[("cat/character.json", MANIFEST), ("cat/persona.md", b"v3")],
        );
        install(&packs, &e, &work3).unwrap();
        assert_eq!(
            std::fs::read_to_string(packs.join("cat/persona.md")).unwrap(),
            "v3"
        );
        assert!(!packs.join(".old-cat").exists());
    }

    #[test]
    fn downloaded_packs_load_as_builtin_characters_and_stage_folders_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let packs = dir.path().join("packs");
        std::fs::create_dir_all(packs.join(".dl-cat/stage")).unwrap();
        let work = dir.path().join("w");
        std::fs::create_dir_all(&work).unwrap();
        write_pack(
            &work,
            &[
                ("cat/character.json", MANIFEST),
                ("cat/persona.md", b"p"),
                ("cat/voice/index.json", br#"{"version":1,"lines":{}}"#),
            ],
        );
        install(&packs, &entry_for("cat"), &work).unwrap();
        let none = tempfile::tempdir().unwrap();
        let all = character::load_all_in(&[packs.as_path()], none.path());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].manifest.id, "cat");
        assert_eq!(all[0].origin, character::Origin::Builtin);
        assert_eq!(
            all[0].builtin_dir.as_deref(),
            Some(packs.join("cat").as_path())
        );
    }

    /// 本机起一个只会返回固定内容的小服务器
    async fn serve(body: Vec<u8>, slow: bool) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let b = body.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = s.read(&mut buf).await;
                    let head = format!(
                        "HTTP/1.1 200 OK
Content-Length: {}
Connection: close

",
                        b.len()
                    );
                    let _ = s.write_all(head.as_bytes()).await;
                    let (first, rest) = b.split_at(b.len() / 2);
                    let _ = s.write_all(first).await;
                    let _ = s.flush().await;
                    if slow {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                    let _ = s.write_all(rest).await;
                });
            }
        });
        format!("http://127.0.0.1:{port}/x")
    }

    fn sha(b: &[u8]) -> String {
        Sha256::digest(b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect()
    }

    #[tokio::test]
    async fn fetch_saves_only_what_matches_the_pinned_hash() {
        let body = b"hello pack, hello pack".to_vec();
        let url = serve(body.clone(), false).await;
        let client = reqwest::Client::new();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.tar.bz2");
        let mut seen = 0;
        fetch(
            &client,
            &url,
            &sha(&body),
            &dest,
            &CancellationToken::new(),
            |n| seen = n,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        let _ = seen;
        // 校验值不对：不留文件，报错
        let dest2 = dir.path().join("b.tar.bz2");
        let err = fetch(
            &client,
            &url,
            &"0".repeat(64),
            &dest2,
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("校验"), "{err}");
        assert!(!dest2.exists() && !dest2.with_extension("part").exists());
    }

    #[tokio::test]
    async fn fetch_can_be_cancelled_midway() {
        let body = vec![7u8; 4096];
        let url = serve(body.clone(), true).await;
        let client = reqwest::Client::new();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.tar.bz2");
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            c2.cancel();
        });
        let err = fetch(&client, &url, &sha(&body), &dest, &cancel, |_| {})
            .await
            .unwrap_err();
        assert_eq!(err, "已取消");
        assert!(!dest.exists() && !dest.with_extension("part").exists());
    }

    #[test]
    fn plain_http_is_never_allowed_outside_test_mode_to_other_hosts() {
        assert!(url_allowed("https://github.com/x"));
        assert!(!url_allowed("http://github.com/x"));
        assert!(!url_allowed("http://evil.example/x"));
        assert!(!url_allowed("ftp://x"));
    }

    /// 本机打过包（scripts/chibi/build_character_pack.py）就核对：登记表里的校验值和大小是这份包的，
    /// 并且程序真的能把它解开、当成角色加载（CI 里没有包就跳过）
    #[test]
    fn locally_built_packs_match_the_registry_and_load() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dist-packs");
        for e in catalog() {
            let Ok(bytes) = std::fs::read(dir.join(format!("{}.tar.bz2", e.id))) else {
                continue;
            };
            assert_eq!(
                sha(&bytes),
                e.sha256,
                "{}：登记表的校验值不是这份包的，重新 --update-registry",
                e.id
            );
            assert_eq!(bytes.len() as u64, e.size, "{}", e.id);
            let work = tempfile::tempdir().unwrap();
            std::fs::write(work.path().join("pack.tar.bz2"), &bytes).unwrap();
            let packs = tempfile::tempdir().unwrap();
            install(packs.path(), e, work.path()).unwrap();
            let none = tempfile::tempdir().unwrap();
            let all = character::load_all_in(&[packs.path()], none.path());
            let c = all
                .iter()
                .find(|c| c.manifest.id == e.id)
                .expect("装好的包能当角色加载");
            assert_eq!(c.manifest.name, e.name);
            assert!(!c.persona.is_empty());
            assert!(c.pool.scene_count() > 50, "台词池应该有很多场景");
            assert_eq!(installed_version_in(packs.path(), &e.id), Some(e.version));
        }
    }

    #[test]
    fn startup_plan_downloads_what_the_user_was_using_but_makes_new_users_choose() {
        let first = catalog()[0].id.clone();
        let other = catalog().last().unwrap().id.clone();
        // 全新安装：什么都没装 → 让用户挑
        assert_eq!(startup_plan(true, &first, &[]), Startup::Choose);
        // 升级前用着 first，现在没装 → 后台下载 first（不下别的）
        assert_eq!(
            startup_plan(false, &first, &[]),
            Startup::Download(vec![first.clone()])
        );
        // 升级前用着的不在目录里（比如用户自己的角色）：什么都不做
        assert_eq!(startup_plan(false, "my-own", &[]), Startup::Nothing);
        // 已经装好最新的：什么都不做
        let latest = catalog()
            .iter()
            .map(|e| (e.id.clone(), e.version))
            .collect::<Vec<_>>();
        assert_eq!(startup_plan(false, &first, &latest), Startup::Nothing);
        assert_eq!(startup_plan(true, &first, &latest), Startup::Nothing);
        // 装了旧版本的：更新它（和用没用着无关）
        if catalog().len() > 1 {
            let mut old = latest.clone();
            old.iter_mut().find(|x| x.0 == other).unwrap().1 = 0;
            assert_eq!(
                startup_plan(false, &first, &old),
                Startup::Download(vec![other])
            );
        }
    }
}
