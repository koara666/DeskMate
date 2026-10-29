//! 检查更新（v2.1 P10，DECISIONS #48）。
//!
//! - 启动 1 分钟后查一次 GitHub Releases 上最新的正式版本（每天最多一次，设置里可以关、也可以手动查）。
//!   请求里只有程序名和版本号（GitHub 要求带 User-Agent），不带任何用户信息。
//! - 有新版本：她在气泡里说一句，给按钮「看看更新了什么 / 下载并安装 / 跳过这个版本」。
//! - 下载并安装：**只下载上次检查时 GitHub 返回的那个安装包**（前端不能指定地址），只允许 GitHub 的下载域名；
//!   校验大小和 SHA-256（GitHub 给每个附件算的 `digest`，或者 Release 里的 `SHA256SUMS.txt`），对不上就删掉不装；
//!   放在系统临时文件夹，用户点了才运行安装程序（安静模式，装完自动重新打开），DeskMate 先退出。
//! - 不做静默的自动更新：不点就什么都不会下载。

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// 最新正式版本的接口（GitHub 公开接口，不需要登录）
pub const RELEASES_API: &str = "https://api.github.com/repos/koara666/DeskMate/releases/latest";
/// 允许下载的域名（GitHub 的 Release 附件会跳转到这几个地方）
const ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];
/// 安装包最大多少（防止异常的大文件）
const MAX_INSTALLER: u64 = 60 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// 启动时自动检查（默认开）
    pub auto_check: bool,
    /// 用户说「跳过这个版本」的版本号
    pub skipped: String,
    /// 上次自动检查的时间（Unix 秒）
    pub last_check: i64,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: true,
            skipped: String::new(),
            last_check: 0,
        }
    }
}

impl UpdateSettings {
    pub fn sanitize(mut self) -> Self {
        self.skipped = self.skipped.trim().chars().take(32).collect();
        self
    }
}

/// Release 里的安装包
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// 小写十六进制；GitHub 没给时去 SHA256SUMS.txt 里找
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub installer: Option<Asset>,
    /// Release 里的 SHA256SUMS.txt（没有就是 None）
    pub sums_url: Option<String>,
}

/// 给界面看的
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateView {
    pub version: String,
    pub current: String,
    /// 更新说明（GitHub Release 的正文，纯文本显示，最多 1500 字）
    pub notes: String,
    pub page: String,
    /// 有能直接装的安装包
    pub can_install: bool,
}

#[derive(Default)]
pub struct UpdateState {
    latest: Mutex<Option<Release>>,
    installing: AtomicBool,
}

/// 版本号 → (主, 次, 修订, 是不是预览版)。`v0.3.0`、`0.3.0-beta.1` 都认。
pub fn parse_version(s: &str) -> Option<(u64, u64, u64, bool)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match s.split_once(['-', '+']) {
        Some((c, p)) => (c, !p.is_empty()),
        None => (s, false),
    };
    let mut it = core.split('.');
    let a = it.next()?.parse().ok()?;
    let b = it.next().unwrap_or("0").parse().ok()?;
    let c = it.next().unwrap_or("0").parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((a, b, c, pre))
}

/// `latest` 比 `current` 新吗（预览版比同号的正式版旧）
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some((a, b, c, p)), Some((x, y, z, q))) => (x, y, z, !q) > (a, b, c, !p),
        _ => false,
    }
}

/// 只允许 https 的 GitHub 下载域名（测试模式下额外允许本机假服务器）
pub fn url_allowed(url: &str) -> bool {
    if crate::paths::test_mode() && url.starts_with("http://127.0.0.1:") {
        return true;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    ALLOWED_HOSTS.contains(&host)
}

/// 解析 GitHub 的 `releases/latest` 返回（纯函数）
pub fn parse_release(v: &serde_json::Value) -> Option<Release> {
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    parse_version(tag)?;
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    let notes: String = v["body"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(1500)
        .collect();
    let page = v["html_url"].as_str().unwrap_or("").to_string();
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    let mut installer = None;
    let mut sums_url = None;
    for a in &assets {
        let (Some(name), Some(url)) = (a["name"].as_str(), a["browser_download_url"].as_str())
        else {
            continue;
        };
        if !url_allowed(url) {
            continue;
        }
        if name.eq_ignore_ascii_case("SHA256SUMS.txt") {
            sums_url = Some(url.to_string());
        } else if name.to_ascii_lowercase().ends_with("_x64-setup.exe") && installer.is_none() {
            let sha256 = a["digest"]
                .as_str()
                .and_then(|d| d.strip_prefix("sha256:"))
                .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
                .map(|h| h.to_ascii_lowercase());
            installer = Some(Asset {
                name: name.to_string(),
                url: url.to_string(),
                size: a["size"].as_u64().unwrap_or(0),
                sha256,
            });
        }
    }
    Some(Release {
        version,
        notes,
        page: if url_allowed(&page) {
            page
        } else {
            String::new()
        },
        installer,
        sums_url,
    })
}

/// 在 SHA256SUMS.txt 里找某个文件的哈希（`<哈希>  <文件名>` 或 `<哈希> *<文件名>`）
pub fn find_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let h = it.next()?;
        let f = it.next()?.trim_start_matches('*');
        (f == name && h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| h.to_ascii_lowercase())
    })
}

fn api_url() -> String {
    if crate::paths::test_mode()
        && let Ok(u) = std::env::var("DESKMATE_UPDATE_URL")
        && !u.is_empty()
    {
        return u;
    }
    RELEASES_API.to_string()
}

fn user_agent() -> String {
    format!("DeskMate/{}", env!("CARGO_PKG_VERSION"))
}

/// 查最新版本（不管是不是更新）
pub async fn fetch_latest(http: &reqwest::Client, url: &str) -> Result<Release, String> {
    let resp = http
        .get(url)
        .header("User-Agent", user_agent())
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("连不上 GitHub：{e}"))?;
    if resp.status().as_u16() == 404 {
        return Err("GitHub 上还没有发布过正式版本。".into());
    }
    // 公开接口每个网络每小时只能查 60 次，查多了会被暂时拒绝
    if matches!(resp.status().as_u16(), 403 | 429) {
        return Err("GitHub 暂时不让查了（同一个网络这一小时查得太多），过一会儿再试。".into());
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub 返回了 {}", resp.status().as_u16()));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| "GitHub 返回的内容看不懂".to_string())?;
    parse_release(&v).ok_or_else(|| "最新的版本信息不完整".to_string())
}

fn view(r: &Release) -> UpdateView {
    UpdateView {
        version: r.version.clone(),
        current: env!("CARGO_PKG_VERSION").into(),
        notes: r.notes.clone(),
        page: r.page.clone(),
        can_install: r.installer.as_ref().is_some_and(|a| a.size > 0),
    }
}

/// 手动检查（设置页按钮）：返回 Some = 有新版本
pub async fn check_now(st: &AppState) -> Result<Option<UpdateView>, String> {
    let r = fetch_latest(&st.http, &api_url()).await?;
    let newer = is_newer(env!("CARGO_PKG_VERSION"), &r.version);
    let v = view(&r);
    *st.update.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    Ok(newer.then_some(v))
}

/// 启动后：1 分钟后自动查一次（每天最多一次）；有新版本并且没被跳过，就让她说一句
pub fn spawn_startup_check(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let st = app.state::<AppState>();
        let s = st.settings().update;
        let now = crate::memory::now_secs();
        let forced = crate::paths::test_mode() && std::env::var_os("DESKMATE_UPDATE_URL").is_some();
        if !s.auto_check
            || (crate::paths::test_quiet() && !forced)
            || now - s.last_check < 20 * 3600
        {
            return;
        }
        st.update_settings(|s| s.update.last_check = now);
        match check_now(&st).await {
            Ok(Some(v)) if v.version != s.skipped => announce(&app, v),
            Ok(_) => log::info!("检查更新：已经是最新版本"),
            Err(e) => log::info!("检查更新没成功：{e}"),
        }
    });
}

/// 她说一句「有新版本了」（带按钮）
pub fn announce(app: &AppHandle, v: UpdateView) {
    let st = app.state::<AppState>();
    let text = st
        .line("update.available", &[("版本", v.version.clone())])
        .unwrap_or_else(|| format!("DeskMate 有新版本 {} 了，要更新吗？", v.version));
    let mut item = crate::proactive::Item::new(
        crate::proactive::Kind::Briefing,
        text,
        crate::memory::now_secs() + 12 * 3600,
    );
    item.ephemeral = true;
    item.key = format!("update:{}", v.version);
    item.update = Some(v);
    crate::proactive::submit(app, item);
    crate::companion::emit_pose(app, "update.available");
}

/// 这个版本的新功能（升级后她说一次）
pub const WHATS_NEW: &str = "这次最明显的是：我换上了 Q 版小人的新形象，声音也能直接听到了——「设置 → 声音」里选「预录语音」，再打开「让她出声」就行，什么都不用装。我现场想的话还不会出声，想要的话可以装 GPT-SoVITS（步骤见 GitHub 上的 docs/VOICE.md）。";

/// 升级后 20 秒：她介绍一次新功能（走调度器，勿扰 / 全屏时等合适的时候）
pub fn spawn_whats_new(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        let st = app.state::<AppState>();
        let head = st
            .line(
                "update.whats_new",
                &[("版本", env!("CARGO_PKG_VERSION").into())],
            )
            .unwrap_or_else(|| format!("更新到 {} 了，多了这些：", env!("CARGO_PKG_VERSION")));
        let mut item = crate::proactive::Item::new(
            crate::proactive::Kind::Briefing,
            format!("{head}\n{WHATS_NEW}"),
            crate::memory::now_secs() + 24 * 3600,
        );
        item.key = "whats_new".into();
        crate::proactive::submit(&app, item);
    });
}

/// 「跳过这个版本」
pub fn skip(st: &AppState, version: &str) {
    let v: String = version.chars().take(32).collect();
    st.update_settings(|s| s.update.skipped = v);
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let d = sha2::Sha256::digest(data);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// 安装进度（发给聊天窗口：`update-progress`）
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// downloading / verifying / launching / failed
    pub phase: &'static str,
    pub percent: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
}

fn emit(app: &AppHandle, phase: &'static str, percent: u32, message: String) {
    let _ = app.emit(
        "update-progress",
        Progress {
            phase,
            percent,
            message,
        },
    );
}

/// 下载上次检查到的安装包、校验，然后运行安装程序并退出
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let rel = st
        .update
        .latest
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("还没检查过更新")?;
    if !is_newer(env!("CARGO_PKG_VERSION"), &rel.version) {
        return Err("已经是最新版本了".into());
    }
    let asset = rel
        .installer
        .clone()
        .ok_or("这个版本没有可以直接安装的安装包，请到 GitHub 页面下载。")?;
    if !url_allowed(&asset.url) || asset.size == 0 || asset.size > MAX_INSTALLER {
        return Err("安装包的信息不对，没有下载。".into());
    }
    if st.update.installing.swap(true, Ordering::SeqCst) {
        return Err("已经在下载了".into());
    }
    let r = download_and_run(app, &st, &rel, &asset).await;
    if let Err(e) = &r {
        emit(app, "failed", 0, e.clone());
        st.update.installing.store(false, Ordering::SeqCst);
    }
    r
}

async fn download_and_run(
    app: &AppHandle,
    st: &AppState,
    rel: &Release,
    asset: &Asset,
) -> Result<(), String> {
    use futures_util::StreamExt;
    // 期望的哈希：GitHub 给的 digest，或者 SHA256SUMS.txt
    let expected = match &asset.sha256 {
        Some(h) => h.clone(),
        None => {
            let url = rel
                .sums_url
                .as_ref()
                .ok_or("这个版本没有校验信息，为了安全不自动安装，请到 GitHub 页面下载。")?;
            let sums = st
                .http
                .get(url)
                .header("User-Agent", user_agent())
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| format!("下载校验信息失败：{e}"))?
                .text()
                .await
                .map_err(|e| format!("下载校验信息失败：{e}"))?;
            find_sum(&sums, &asset.name).ok_or("校验信息里没有这个安装包，不自动安装。")?
        }
    };
    emit(app, "downloading", 0, String::new());
    let resp = st
        .http
        .get(&asset.url)
        .header("User-Agent", user_agent())
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("下载失败：{e}"))?;
    // 跳转后的地址也必须是 GitHub 的
    if !url_allowed(resp.url().as_str()) {
        return Err("下载地址被跳转到了别的网站，已停止。".into());
    }
    let mut data: Vec<u8> = Vec::with_capacity(asset.size as usize);
    let mut stream = resp.bytes_stream();
    let mut last_pct = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断了：{e}"))?;
        data.extend_from_slice(&chunk);
        if data.len() as u64 > asset.size {
            return Err("下载的文件比说好的大，已停止。".into());
        }
        let pct = (data.len() as u64 * 100 / asset.size) as u32;
        if pct >= last_pct + 5 {
            last_pct = pct;
            emit(app, "downloading", pct, String::new());
        }
    }
    emit(app, "verifying", 100, String::new());
    if data.len() as u64 != asset.size {
        return Err("下载的文件不完整，没有安装。".into());
    }
    if sha256_hex(&data) != expected {
        return Err("安装包校验没通过（可能下载出错或被改过），已丢弃，没有安装。".into());
    }
    // 放在系统临时文件夹里（不是数据目录）
    let dir = std::env::temp_dir().join("DeskMate-update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建临时文件夹失败：{e}"))?;
    let safe_name: String = asset
        .name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    let path = dir.join(if safe_name.ends_with(".exe") {
        safe_name
    } else {
        "DeskMate-setup.exe".into()
    });
    std::fs::write(&path, &data).map_err(|e| format!("保存安装包失败：{e}"))?;
    drop(data);
    log::info!("更新：{} 已下载并校验，开始安装", rel.version);
    emit(app, "launching", 100, String::new());
    if crate::paths::test_mode() {
        // 测试实例不真的运行安装程序
        log::info!("测试模式：不运行安装程序（{}）", path.display());
        return Ok(());
    }
    // 安静模式（只显示进度条）、装完重新打开 DeskMate
    std::process::Command::new(&path)
        .args(["/P", "/R", "/UPDATE"])
        .spawn()
        .map_err(|e| format!("运行安装程序失败：{e}"))?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.2.0", "0.3.0"));
        assert!(is_newer("0.2.0", "v0.2.1"));
        assert!(is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.3.0", "0.2.9"));
        assert!(!is_newer("0.3.0", "0.3.0-beta.1"), "预览版比正式版旧");
        assert!(is_newer("0.3.0-beta.1", "0.3.0"));
        assert!(!is_newer("0.2.0", "latest"));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0, false)));
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn only_github_downloads() {
        assert!(url_allowed(
            "https://github.com/koara666/DeskMate/releases/download/v0.3.0/a.exe"
        ));
        assert!(url_allowed("https://objects.githubusercontent.com/x"));
        assert!(!url_allowed("http://github.com/x"), "必须 https");
        assert!(!url_allowed("https://github.com.evil.com/x"));
        assert!(!url_allowed("https://evil.com/github.com/x"));
        assert!(
            !url_allowed("http://127.0.0.1:8080/x"),
            "正式实例不允许本机地址"
        );
    }

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "tag_name": "v0.3.0",
            "draft": false,
            "prerelease": false,
            "html_url": "https://github.com/koara666/DeskMate/releases/tag/v0.3.0",
            "body": "- 新功能",
            "assets": [
                {"name": "SHA256SUMS.txt", "size": 100,
                 "browser_download_url": "https://github.com/koara666/DeskMate/releases/download/v0.3.0/SHA256SUMS.txt"},
                {"name": "DeskMate_0.3.0_x64-setup.exe", "size": 3_600_000,
                 "digest": "sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
                 "browser_download_url": "https://github.com/koara666/DeskMate/releases/download/v0.3.0/DeskMate_0.3.0_x64-setup.exe"},
                {"name": "evil_x64-setup.exe", "size": 1,
                 "browser_download_url": "https://evil.com/evil_x64-setup.exe"}
            ]
        })
    }

    #[test]
    fn parse_github_release() {
        let r = parse_release(&sample()).unwrap();
        assert_eq!(r.version, "0.3.0");
        let a = r.installer.unwrap();
        assert_eq!(a.name, "DeskMate_0.3.0_x64-setup.exe");
        assert_eq!(a.size, 3_600_000);
        assert_eq!(
            a.sha256.as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert!(r.sums_url.is_some());
        let mut pre = sample();
        pre["prerelease"] = serde_json::json!(true);
        assert!(parse_release(&pre).is_none(), "预览版不提示");
        let mut bad = sample();
        bad["tag_name"] = serde_json::json!("nightly");
        assert!(parse_release(&bad).is_none());
        // 别的域名的附件不要
        let mut other = sample();
        other["assets"] = serde_json::json!([other["assets"][2].clone()]);
        assert!(parse_release(&other).unwrap().installer.is_none());
    }

    #[test]
    fn sums_file() {
        let h = "a".repeat(64);
        let sums = format!(
            "{h}  DeskMate_0.3.0_x64-setup.exe\n{}  other.exe\n",
            "b".repeat(64)
        );
        assert_eq!(
            find_sum(&sums, "DeskMate_0.3.0_x64-setup.exe"),
            Some(h.clone())
        );
        assert_eq!(find_sum(&format!("{h} *x.exe"), "x.exe"), Some(h));
        assert_eq!(find_sum(&sums, "nope.exe"), None);
        assert_eq!(find_sum("zz  x.exe", "x.exe"), None);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn fetch_from_fake_server() {
        let body = serde_json::to_string(&sample()).unwrap();
        let (base, rx) =
            crate::backends::http::fake_server::serve(200, "application/json", vec![body]).await;
        let http = crate::backends::http::client();
        let r = fetch_latest(&http, &format!("{base}/repos/x/releases/latest"))
            .await
            .unwrap();
        assert_eq!(r.version, "0.3.0");
        let req = rx.await.unwrap();
        let head = req.head.to_ascii_lowercase();
        assert!(head.contains("user-agent: deskmate/"), "只带程序名和版本");
        assert!(!head.contains("authorization"));
        let (base, _) =
            crate::backends::http::fake_server::serve(404, "application/json", vec!["{}".into()])
                .await;
        assert!(
            fetch_latest(&http, &base)
                .await
                .unwrap_err()
                .contains("还没有发布")
        );
    }
}
