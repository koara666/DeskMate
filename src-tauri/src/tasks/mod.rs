//! 任务执行：把需要动手的事交给本机的 Claude Code / Codex 命令行（需求第 6、8 节）。
//!
//! 安全要点（详见 SECURITY.md T4/T5/T8）：
//! - 只调用官方命令行程序；不读取、不转发它们的登录令牌；不把 DeskMate 保存的 API Key 传给它们。
//! - 直接启动真正的可执行文件（不经过 cmd.exe），任务内容从 stdin 传入，命令行里只有固定参数和校验过的值。
//! - 工作目录必须在「任务文件夹」之内（`sandbox`），并由 CLI 自己的权限/沙盒参数强制执行。
//! - 超时自动终止整个进程树；单任务花费上限；全局每日/每月上限照样生效。
//! - 每个任务完整记录到 `%APPDATA%\DeskMate\tasks\<任务id>.jsonl`（输出前脱敏）。

pub mod claude;
pub mod codex;
pub mod sandbox;

use crate::redact::redact_secrets;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Executor {
    Claude,
    Codex,
}

impl Executor {
    pub fn id(&self) -> &'static str {
        match self {
            Executor::Claude => "claude-code",
            Executor::Codex => "codex",
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Executor::Claude => "Claude Code",
            Executor::Codex => "Codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Read,
    Edit,
    Command,
    Network,
    Outside,
    Note,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskStep {
    pub kind: StepKind,
    pub text: String,
}

/// 需要用户批准的一项操作
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalItem {
    pub kind: StepKind,
    pub detail: String,
    /// 批准后放行用的精确规则（`--allowedTools`）；None 表示没法精确放行
    pub rule: Option<String>,
    /// 访问工作目录以外的路径
    pub path: Option<String>,
}

/// 发给聊天窗口的任务事件
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    Started {
        task_id: String,
        executor: String,
    },
    Step {
        kind: StepKind,
        text: String,
    },
    NeedsApproval {
        summary: String,
        items: Vec<ApprovalItem>,
    },
    Done {
        summary: String,
        cost_usd: f64,
        changed: Vec<String>,
        backup: Option<String>,
    },
    Failed {
        message: String,
        detail: String,
    },
}

/// 任务设置（存在设置里）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskSettings {
    pub executor: Executor,
    /// 任务文件夹（空 = 默认 D:\DeskMate-Work）
    pub root: String,
    pub timeout_minutes: u32,
    /// 单个任务的花费上限（美元，Claude Code 用 --max-budget-usd 强制）
    pub budget_usd: f64,
    pub claude_model: String,
    pub codex_model: String,
    /// 手动指定的程序路径（空 = 自动查找）
    pub claude_path: String,
    pub codex_path: String,
}

impl Default for TaskSettings {
    fn default() -> Self {
        Self {
            executor: Executor::Claude,
            root: String::new(),
            timeout_minutes: 10,
            budget_usd: 0.5,
            claude_model: String::new(),
            codex_model: String::new(),
            claude_path: String::new(),
            codex_path: String::new(),
        }
    }
}

fn safe_model(m: &str) -> bool {
    m.is_empty()
        || (m.len() <= 80
            && !m.starts_with('-')
            && m.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._:/[]".contains(c)))
}

impl TaskSettings {
    pub fn sanitize(mut self) -> Self {
        self.timeout_minutes = self.timeout_minutes.clamp(1, 120);
        if !self.budget_usd.is_finite() || self.budget_usd <= 0.0 {
            self.budget_usd = 0.5;
        }
        self.budget_usd = self.budget_usd.min(50.0);
        if !safe_model(&self.claude_model) {
            self.claude_model.clear();
        }
        if !safe_model(&self.codex_model) {
            self.codex_model.clear();
        }
        self
    }

    pub fn root_path(&self) -> PathBuf {
        if self.root.trim().is_empty() {
            sandbox::default_root()
        } else {
            PathBuf::from(self.root.trim())
        }
    }
}

// ---------------- 查找命令行程序 ----------------

/// 要启动的程序：可执行文件 + 固定的前置参数（比如 node + codex.js）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub exe: PathBuf,
    pub pre_args: Vec<String>,
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 在 PATH（以及 npm 全局目录）里查找程序。
/// 不用 where.exe：它的输出是控制台代码页（中文系统是 GBK），用户名带中文时路径会被解码坏。
/// 这里直接用系统给的 Unicode 路径拼接、检查文件是否存在。
pub fn find_in_path(
    path_var: &std::ffi::OsStr,
    extra_dirs: &[PathBuf],
    name: &str,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(path_var).collect();
    dirs.extend(extra_dirs.iter().cloned());
    let mut found = Vec::new();
    for d in dirs {
        if d.as_os_str().is_empty() {
            continue;
        }
        for ext in ["exe", "cmd", "bat"] {
            let p = d.join(format!("{name}.{ext}"));
            if p.is_file() && !found.contains(&p) {
                found.push(p);
            }
        }
    }
    found
}

fn where_exe(name: &str) -> Vec<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    // 从开始菜单启动时 PATH 可能不含 npm 全局目录，额外看一眼
    let extra: Vec<PathBuf> = std::env::var_os("APPDATA")
        .map(|a| vec![PathBuf::from(a).join("npm")])
        .unwrap_or_default();
    find_in_path(&path, &extra, name)
}

fn is_ext(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// 从 npm 生成的 .cmd 入口推出真正的程序（避免经过 cmd.exe）
fn resolve_shim(exec: Executor, shim_dir: &Path) -> Option<Program> {
    match exec {
        Executor::Claude => {
            let exe = shim_dir.join(r"node_modules\@anthropic-ai\claude-code\bin\claude.exe");
            exe.is_file().then(|| Program {
                exe,
                pre_args: vec![],
            })
        }
        Executor::Codex => {
            let js = shim_dir.join(r"node_modules\@openai\codex\bin\codex.js");
            if !js.is_file() {
                return None;
            }
            let node = Some(shim_dir.join("node.exe"))
                .filter(|p| p.is_file())
                .or_else(|| where_exe("node").into_iter().find(|p| is_ext(p, "exe")))?;
            Some(Program {
                exe: node,
                pre_args: vec![js.to_string_lossy().into_owned()],
            })
        }
    }
}

/// 找到可以直接启动的程序；找不到返回 None
pub fn locate(exec: Executor, configured: &str) -> Option<Program> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if p.is_file() && is_ext(&p, "exe") {
            return Some(Program {
                exe: p,
                pre_args: vec![],
            });
        }
        if let Some(dir) = p.parent()
            && is_ext(&p, "cmd")
        {
            return resolve_shim(exec, dir);
        }
        return None;
    }
    let name = match exec {
        Executor::Claude => "claude",
        Executor::Codex => "codex",
    };
    for p in where_exe(name) {
        if is_ext(&p, "exe") {
            return Some(Program {
                exe: p,
                pre_args: vec![],
            });
        }
        if let Some(dir) = p.parent()
            && let Some(prog) = resolve_shim(exec, dir)
        {
            return Some(prog);
        }
    }
    None
}

/// 启动 claude / codex 时不传下去的环境变量（DECISIONS #29）：
/// - API Key / 第三方接口地址：Claude Code 在 `-p` 模式下只要看到 `ANTHROPIC_API_KEY` 就优先用它，
///   不走订阅登录——Key 无效时报「Invalid API key」，有效时按 API 计费花钱，都不是「用订阅额度」该有的样子。
/// - 父级 Claude Code 会话的变量：DeskMate 碰巧是从 Claude Code 里启动的时候才会有，
///   传下去会让子进程以为自己嵌套在别的会话里，还会把那个会话的通信令牌带过去。
pub const SCRUBBED_ENV: [&str; 11] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
];

fn command(prog: &Program) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(&prog.exe);
    c.args(&prog.pre_args);
    for k in SCRUBBED_ENV {
        c.env_remove(k);
    }
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// `--version` 的第一行（只查版本，不执行任何任务、不消耗额度）
pub async fn version(prog: &Program) -> Option<String> {
    let mut c = command(prog);
    c.arg("--version")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(15), c.output())
        .await
        .ok()?
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
}

// ---------------- 任务记录 ----------------

pub fn new_task_id() -> String {
    let now = chrono::Local::now();
    let rnd: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("t{}-{:05}", now.format("%Y%m%d-%H%M%S"), rnd % 100_000)
}

pub fn valid_task_id(id: &str) -> bool {
    id.len() <= 40
        && id.starts_with('t')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskRecord {
    pub id: String,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub executor: String,
    pub goal: String,
    pub workdir: String,
    pub status: String,
    pub summary: String,
    pub cost_usd: f64,
}

pub fn insert_task(
    conn: &Connection,
    id: &str,
    exec: Executor,
    goal: &str,
    workdir: &str,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO tasks(id, created_at, executor, goal, workdir, status) VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
        params![id, now, exec.id(), crate::memory::sensitive::scrub(goal), workdir],
    )?;
    Ok(())
}

pub fn finish_task(
    conn: &Connection,
    id: &str,
    status: &str,
    summary: &str,
    cost: f64,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE tasks SET status = ?2, summary = ?3, cost_usd = cost_usd + ?4, finished_at = ?5 WHERE id = ?1",
        params![id, status, redact_secrets(&summary.chars().take(2000).collect::<String>()), cost, now],
    )?;
    Ok(())
}

const TASK_COLS: &str =
    "id, created_at, finished_at, executor, goal, workdir, status, summary, cost_usd";

fn row_to_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: r.get(0)?,
        created_at: r.get(1)?,
        finished_at: r.get(2)?,
        executor: r.get(3)?,
        goal: r.get(4)?,
        workdir: r.get(5)?,
        status: r.get(6)?,
        summary: r.get(7)?,
        cost_usd: r.get(8)?,
    })
}

pub fn list_tasks(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<TaskRecord>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks ORDER BY created_at DESC LIMIT ?1"
    ))?;
    stmt.query_map([limit], row_to_task)?.collect()
}

pub fn get_task(conn: &Connection, id: &str) -> rusqlite::Result<Option<TaskRecord>> {
    conn.query_row(
        &format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"),
        [id],
        row_to_task,
    )
    .optional()
}

/// 任务日志（JSONL）。每行输出前脱敏。
pub struct TaskLog {
    file: Option<std::fs::File>,
}

impl TaskLog {
    pub fn open(dir: &Path, task_id: &str) -> Self {
        let _ = std::fs::create_dir_all(dir);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("{task_id}.jsonl")))
            .ok();
        Self { file }
    }

    pub fn write(&mut self, kind: &str, data: serde_json::Value) {
        if let Some(f) = self.file.as_mut() {
            let line = serde_json::json!({
                "ts": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                "type": kind,
                "data": data,
            })
            .to_string();
            let _ = writeln!(f, "{}", redact_secrets(&line));
        }
    }
}

// ---------------- 运行 ----------------

#[derive(Debug, Clone, PartialEq)]
pub enum RunEnd {
    /// 进程正常结束
    Exited {
        code: Option<i32>,
    },
    Cancelled,
    TimedOut,
    SpawnFailed(String),
}

/// 结束整个进程树（命令行程序可能启动了子进程）
fn kill_tree(pid: Option<u32>) {
    #[cfg(windows)]
    if let Some(pid) = pid {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    #[cfg(not(windows))]
    let _ = pid;
}

/// 启动程序，把 `prompt` 写进 stdin，逐行回调 stdout；超时或取消时结束进程树。
/// 返回结束原因和 stderr 的末尾（已脱敏）。
pub async fn run_process(
    prog: &Program,
    args: &[String],
    workdir: &Path,
    prompt: &str,
    timeout: Duration,
    cancel: &CancellationToken,
    mut on_line: impl FnMut(&str),
) -> (RunEnd, String) {
    let mut c = command(prog);
    c.args(args)
        .current_dir(workdir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return (RunEnd::SpawnFailed(e.to_string()), String::new()),
    };
    let pid = child.id();
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    let mut stderr = child.stderr.take();
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(e) = stderr.as_mut() {
            let _ = e.read_to_end(&mut buf).await;
        }
        let s = String::from_utf8_lossy(&buf).into_owned();
        let tail: String = s
            .chars()
            .rev()
            .take(2000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        redact_secrets(&tail)
    });
    let stdout = child.stdout.take();
    let deadline = Instant::now() + timeout;
    let mut end = None;
    if let Some(out) = stdout {
        // 按字节读行再宽松解码：输出里夹着非 UTF-8 内容（比如中文系统的 GBK）也不会中断读取
        let mut reader = BufReader::new(out);
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::select! {
                _ = cancel.cancelled() => { end = Some(RunEnd::Cancelled); break; }
                _ = tokio::time::sleep(remaining) => { end = Some(RunEnd::TimedOut); break; }
                n = reader.read_until(b'\n', &mut buf) => match n {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf);
                        on_line(line.trim_end_matches(['\r', '\n']));
                    }
                },
            }
        }
    }
    let end = match end {
        Some(e) => {
            kill_tree(pid);
            let _ = child.kill().await;
            e
        }
        None => {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_secs(5));
            match tokio::time::timeout(remaining, child.wait()).await {
                Ok(Ok(st)) => RunEnd::Exited { code: st.code() },
                _ => {
                    kill_tree(pid);
                    let _ = child.kill().await;
                    RunEnd::TimedOut
                }
            }
        }
    };
    let tail = tokio::time::timeout(Duration::from_secs(3), err_task)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    (end, tail)
}

// ---------------- Codex 写模式前的备份 ----------------

const BACKUP_MAX_BYTES: u64 = 300 * 1024 * 1024;
const BACKUP_MAX_FILES: usize = 10_000;
const BACKUP_KEEP: usize = 5;

/// 统计目录大小（不跟随符号链接）。超过上限返回 Err。
fn scan(dir: &Path, bytes: &mut u64, files: &mut usize) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let ft = e.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            scan(&e.path(), bytes, files)?;
        } else if ft.is_file() {
            *bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            *files += 1;
        }
        if *bytes > BACKUP_MAX_BYTES || *files > BACKUP_MAX_FILES {
            return Err("工作目录太大（超过 300 MB 或 1 万个文件），无法先备份，所以不允许 Codex 修改文件。可以换一个更小的文件夹。".into());
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let ft = e.file_type()?;
        let target = to.join(e.file_name());
        if ft.is_symlink() {
            continue;
        } else if ft.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else if ft.is_file() {
            std::fs::copy(e.path(), &target)?;
        }
    }
    Ok(())
}

/// 把工作目录完整复制到备份目录，返回备份路径；只保留最近 5 份
pub fn backup_workdir(
    workdir: &Path,
    backups_root: &Path,
    task_id: &str,
) -> Result<PathBuf, String> {
    let (mut bytes, mut files) = (0u64, 0usize);
    scan(workdir, &mut bytes, &mut files)?;
    let dest = backups_root.join(task_id);
    copy_dir(workdir, &dest).map_err(|e| format!("备份工作目录失败：{e}"))?;
    if let Ok(rd) = std::fs::read_dir(backups_root) {
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        if dirs.len() > BACKUP_KEEP {
            for d in &dirs[..dirs.len() - BACKUP_KEEP] {
                let _ = std::fs::remove_dir_all(d);
            }
        }
    }
    Ok(dest)
}

/// 发给执行 agent 的任务说明（只有任务本身 + 必要背景，不带聊天记录和角色设定）
pub fn task_prompt(goal: &str, workdir: &Path) -> String {
    format!(
        "任务：{}\n\n工作目录：{}（只能在这个目录里操作）\n\n完成后用简洁的中文说明：做了什么、新建/修改/删除了哪些文件、有没有没完成的部分。\n如果某个操作被拒绝了，不要想办法绕过，在说明里列出还需要用户批准的操作。",
        goal.trim(),
        workdir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_sanitized() {
        let s = TaskSettings {
            timeout_minutes: 0,
            budget_usd: f64::NAN,
            claude_model: "--dangerously-skip-permissions".into(),
            codex_model: "gpt-5.5".into(),
            ..Default::default()
        }
        .sanitize();
        assert_eq!(s.timeout_minutes, 1);
        assert_eq!(s.budget_usd, 0.5);
        assert_eq!(
            s.claude_model, "",
            "以 - 开头的模型名会被当成参数，必须拒绝"
        );
        assert_eq!(s.codex_model, "gpt-5.5");
    }

    #[test]
    fn finds_npm_shims_in_unicode_paths() {
        // 用户名带中文的目录（之前用 where.exe 时就是这种路径解码出错，找不到程序）
        let t = tempfile::tempdir().unwrap();
        let npm = t.path().join("京东").join("npm");
        let pkg = npm.join(r"node_modules\@anthropic-ai\claude-code\bin");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(npm.join("claude.cmd"), "@echo off").unwrap();
        std::fs::write(pkg.join("claude.exe"), "fake").unwrap();
        let path_var = std::env::join_paths([PathBuf::from(r"C:\nope"), npm.clone()]).unwrap();
        let found = find_in_path(&path_var, &[], "claude");
        assert_eq!(found, vec![npm.join("claude.cmd")]);
        let prog = resolve_shim(Executor::Claude, found[0].parent().unwrap()).unwrap();
        assert_eq!(prog.exe, pkg.join("claude.exe"));
        assert!(prog.pre_args.is_empty());
        // 手动指定 .cmd 路径也能解析
        let p = locate(
            Executor::Claude,
            &npm.join("claude.cmd").display().to_string(),
        )
        .unwrap();
        assert_eq!(p.exe, pkg.join("claude.exe"));
        assert!(find_in_path(&path_var, &[], "codex").is_empty());
    }

    #[test]
    fn task_ids() {
        let id = new_task_id();
        assert!(valid_task_id(&id), "{id}");
        assert!(!valid_task_id("../x"));
        assert!(!valid_task_id("x1"));
    }

    #[test]
    fn task_records() {
        let c = crate::db::open_in_memory().unwrap();
        insert_task(
            &c,
            "t1",
            Executor::Claude,
            "整理截图，密码是 abc12345",
            "D:\\W",
            10,
        )
        .unwrap();
        finish_task(&c, "t1", "done", "整理好了", 0.05, 20).unwrap();
        let t = get_task(&c, "t1").unwrap().unwrap();
        assert_eq!(t.status, "done");
        assert!(!t.goal.contains("abc12345"), "任务描述入库前脱敏");
        assert!((t.cost_usd - 0.05).abs() < 1e-9);
        assert_eq!(list_tasks(&c, 10).unwrap().len(), 1);
    }

    #[test]
    fn backup_copies_and_refuses_huge_dirs() {
        let t = tempfile::tempdir().unwrap();
        let w = t.path().join("w");
        std::fs::create_dir_all(w.join("sub")).unwrap();
        std::fs::write(w.join("a.txt"), "A").unwrap();
        std::fs::write(w.join("sub").join("b.txt"), "B").unwrap();
        let b = backup_workdir(&w, &t.path().join("bk"), "t1").unwrap();
        assert_eq!(
            std::fs::read_to_string(b.join("sub").join("b.txt")).unwrap(),
            "B"
        );
    }

    #[test]
    fn prompt_has_only_task_and_workdir() {
        let p = task_prompt("把截图按日期分类", Path::new("D:\\W"));
        assert!(p.contains("把截图按日期分类"));
        assert!(p.contains("D:\\W"));
        assert!(p.contains("不要想办法绕过"));
    }

    #[test]
    fn cli_does_not_inherit_api_keys_or_parent_session() {
        let prog = Program {
            exe: PathBuf::from("claude"),
            pre_args: vec![],
        };
        let c = command(&prog);
        let removed: Vec<String> = c
            .as_std()
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        for k in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDECODE",
            "CLAUDE_CODE_MESSAGING_TOKEN",
        ] {
            assert!(
                removed.iter().any(|r| r.eq_ignore_ascii_case(k)),
                "{k} 应该被去掉"
            );
        }
        // 用户自己的 Claude Code 配置（比如 Windows 上的 Git Bash 路径）照常传下去
        assert!(
            !removed
                .iter()
                .any(|r| r.eq_ignore_ascii_case("CLAUDE_CODE_GIT_BASH_PATH"))
        );
    }

    #[tokio::test]
    async fn run_process_times_out_and_cancels() {
        // 用系统自带的 ping 模拟一个长时间运行的程序（不联网：只 ping 本机）
        let prog = Program {
            exe: PathBuf::from("ping"),
            pre_args: vec![],
        };
        let args = vec!["-n".to_string(), "30".into(), "127.0.0.1".into()];
        let dir = std::env::temp_dir();
        let start = Instant::now();
        let (end, _) = run_process(
            &prog,
            &args,
            &dir,
            "",
            Duration::from_millis(800),
            &CancellationToken::new(),
            |_| {},
        )
        .await;
        assert_eq!(end, RunEnd::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(10));
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        let mut lines = 0;
        let (end, _) = run_process(
            &prog,
            &args,
            &dir,
            "",
            Duration::from_secs(30),
            &cancel,
            |_| {
                lines += 1;
                if lines == 2 {
                    c2.cancel();
                }
            },
        )
        .await;
        assert_eq!(end, RunEnd::Cancelled);
        let bad = Program {
            exe: PathBuf::from("definitely-not-a-real-program-xyz"),
            pre_args: vec![],
        };
        let (end, _) = run_process(
            &bad,
            &[],
            &dir,
            "",
            Duration::from_secs(5),
            &CancellationToken::new(),
            |_| {},
        )
        .await;
        assert!(matches!(end, RunEnd::SpawnFailed(_)));
    }
}
