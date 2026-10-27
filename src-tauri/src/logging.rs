//! 极简文件日志：自动轮换（单个文件 1 MB，最多保留 3 个），输出前脱敏。
//! 规则：日志里不记录密钥、请求头、请求体和完整对话内容（对话只存在数据库里）。

use crate::redact::redact_secrets;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BYTES: u64 = 1024 * 1024;
const KEEP: usize = 3;

struct FileLogger {
    dir: PathBuf,
    file: Mutex<Option<File>>,
    level: log::LevelFilter,
}

impl FileLogger {
    fn path(&self, n: usize) -> PathBuf {
        if n == 0 {
            self.dir.join("deskmate.log")
        } else {
            self.dir.join(format!("deskmate.{n}.log"))
        }
    }

    fn rotate_if_needed(&self, f: &mut Option<File>) {
        let size = std::fs::metadata(self.path(0))
            .map(|m| m.len())
            .unwrap_or(0);
        if size < MAX_BYTES && f.is_some() {
            return;
        }
        if size >= MAX_BYTES {
            *f = None;
            let _ = std::fs::remove_file(self.path(KEEP - 1));
            for n in (0..KEEP - 1).rev() {
                let _ = std::fs::rename(self.path(n), self.path(n + 1));
            }
        }
        *f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(0))
            .ok();
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= self.level && (m.level() <= log::Level::Warn || !is_noisy(m.target()))
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            r.level(),
            r.target(),
            redact_secrets(&r.args().to_string())
        );
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
        self.rotate_if_needed(&mut f);
        if let Some(file) = f.as_mut() {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = f.flush();
        }
    }
}

/// 第三方库的调试日志太多，只保留警告以上
fn is_noisy(target: &str) -> bool {
    ["tao", "wry", "reqwest", "hyper", "rustls", "tracing"]
        .iter()
        .any(|p| target.starts_with(p))
}

pub fn init(dir: &Path) {
    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let logger = FileLogger {
        dir: dir.to_path_buf(),
        file: Mutex::new(None),
        level,
    };
    if log::set_logger(Box::leak(Box::new(logger))).is_ok() {
        log::set_max_level(level);
    }
}
