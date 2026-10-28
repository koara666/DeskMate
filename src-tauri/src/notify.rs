//! 系统通知：全屏时到了的提醒、标记为「重要」的提醒（DECISIONS #38）。
//! 只在 Rust 端使用通知插件，不开放给任何窗口。Windows 上只有安装后的程序才显示自己的名字和图标。

use crate::state::AppState;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

pub fn send(app: &AppHandle, text: &str) {
    let title = app
        .state::<AppState>()
        .current_character()
        .map(|c| c.manifest.name)
        .unwrap_or_else(|| "DeskMate".into());
    let body: String = text.chars().take(200).collect();
    match app.notification().builder().title(title).body(body).show() {
        Ok(()) => log::info!("已发系统通知"),
        Err(e) => log::warn!("系统通知发送失败：{e}"),
    }
}
