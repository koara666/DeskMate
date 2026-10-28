//! 系统通知（全屏时到了的提醒等）。P1 接入真正的系统通知，现在只记日志。

use tauri::AppHandle;

pub fn send(_app: &AppHandle, text: &str) {
    log::info!("系统通知（{} 字）", text.chars().count());
}
