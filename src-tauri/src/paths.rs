//! 数据目录：默认 `%APPDATA%\DeskMate`，开发时可用环境变量 `DESKMATE_DATA_DIR` 覆盖。
//! 数据绝不放在程序目录。

use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "DeskMate";
pub const DB_FILE: &str = "deskmate.db";

#[derive(Debug, Clone)]
pub struct DataPaths {
    pub root: PathBuf,
}

impl DataPaths {
    /// `system_data_dir` 是系统的漫游数据目录（Windows 上是 %APPDATA%）。
    pub fn resolve(system_data_dir: Option<PathBuf>) -> Result<Self, String> {
        let root = match std::env::var_os("DESKMATE_DATA_DIR") {
            Some(p) if !p.is_empty() => PathBuf::from(p),
            _ => system_data_dir
                .ok_or_else(|| "找不到系统用户数据目录".to_string())?
                .join(APP_DIR_NAME),
        };
        Ok(Self { root })
    }

    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [
            self.root.clone(),
            self.backups(),
            self.logs(),
            self.characters(),
            self.tasks(),
        ] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    pub fn db(&self) -> PathBuf {
        self.root.join(DB_FILE)
    }
    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    /// 用户自己的角色包 / 角色图片
    pub fn characters(&self) -> PathBuf {
        self.root.join("characters")
    }
    /// 任务执行记录（每个任务一个 jsonl）
    pub fn tasks(&self) -> PathBuf {
        self.root.join("tasks")
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}
