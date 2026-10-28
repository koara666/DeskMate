//! 语音（v2.1 P14，DECISIONS #51）。这一版先留出接口：头顶气泡菜单里的「声音」开关在语音做好以后才出现。

use crate::settings::Settings;
use tauri::AppHandle;

#[derive(Default)]
pub struct VoiceState {}

/// 头顶气泡菜单里「声音」开关的状态：None = 还没配置语音，不显示这个气泡
pub fn menu_state(_s: &Settings) -> Option<bool> {
    None
}

/// 切换声音开关，返回切换后的状态
pub fn toggle(_app: &AppHandle) -> bool {
    false
}
