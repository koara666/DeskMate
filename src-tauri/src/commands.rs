//! 前端可以调用的命令。这里只做参数校验和转发，业务逻辑放在各模块里。
//! 新增命令后，记得在 build.rs 的命令清单和 capabilities/*.json 里登记（按窗口最小授权）。

use crate::backends::ChatMessage;
use crate::character::{self, Appearance};
use crate::chat::{self, ChatEvent};
use crate::desktop::{self, ChatMode, Placement};
use crate::memory::conversation;
use crate::state::AppState;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

type CmdResult<T> = Result<T, String>;

const MAX_INPUT_CHARS: usize = 8000;

fn err(e: impl std::fmt::Display) -> String {
    crate::redact::redact_secrets(&e.to_string())
}

fn check_request_id(id: &str) -> CmdResult<()> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("请求编号不合法".into());
    }
    Ok(())
}

#[derive(Serialize)]
pub struct CharacterView {
    pub id: String,
    pub name: String,
    pub color: String,
}

fn character_view(state: &AppState) -> CharacterView {
    state
        .current_character()
        .map(|c| CharacterView {
            id: c.manifest.id,
            name: c.manifest.name,
            color: c.manifest.color,
        })
        .unwrap_or(CharacterView {
            id: String::new(),
            name: "小伙伴".into(),
            color: String::new(),
        })
}

// ---------------- 桌宠窗口 ----------------
//
// 注意：凡是可能创建窗口或操作窗口的命令都必须是 async。
// 同步命令在 Windows 上跑在主线程，此时创建 WebView 窗口会死锁，整个程序卡住（关不掉、点不动）。

#[derive(Serialize)]
pub struct PetInit {
    pub character: CharacterView,
    pub appearance: Option<Appearance>,
    pub idle_animation: bool,
    pub dnd: bool,
}

#[tauri::command]
pub fn pet_init(state: State<'_, AppState>) -> PetInit {
    let s = state.settings();
    PetInit {
        character: character_view(&state),
        appearance: state.current_character().map(|c| character::appearance(&c)),
        idle_animation: s.idle_animation,
        dnd: s.dnd,
    }
}

#[tauri::command]
pub async fn pet_click(app: AppHandle) -> CmdResult<()> {
    app.state::<AppState>().daily.touch();
    desktop::toggle_chat(&app).map_err(err)
}

#[tauri::command]
pub async fn pet_menu(app: AppHandle) -> CmdResult<()> {
    desktop::popup_pet_menu(&app).map_err(err)
}

// ---------------- 聊天窗口 ----------------

#[derive(Serialize)]
pub struct ChatInit {
    pub character: CharacterView,
    pub mode: ChatMode,
    pub placement: Placement,
    pub history: Vec<ChatMessage>,
    /// 没有聊天记录时角色说的开场白
    pub greeting: String,
    /// 需要告诉用户的系统提示（比如数据库从备份恢复了），只显示一次
    pub notice: Option<String>,
}

fn current_character_id(state: &AppState) -> String {
    state
        .current_character()
        .map(|c| c.manifest.id)
        .unwrap_or_else(|| "default".into())
}

#[tauri::command]
pub fn chat_init(state: State<'_, AppState>) -> ChatInit {
    let history =
        conversation::recent_for_character(&state.db(), &current_character_id(&state), 60)
            .unwrap_or_default()
            .into_iter()
            .map(|m| ChatMessage {
                role: m.role,
                content: m.content,
            })
            .collect();
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let greeting = state
        .current_character()
        .map(|c| character::pick_line(&c.manifest.lines.greeting, "有什么想聊的吗？", seed))
        .unwrap_or_else(|| "有什么想聊的吗？".into());
    ChatInit {
        greeting,
        character: character_view(&state),
        mode: state.desktop.chat_mode(),
        placement: state.desktop.chat_placement(),
        history,
        notice: state.take_notice(),
    }
}

#[tauri::command]
pub fn chat_send(
    app: AppHandle,
    text: String,
    request_id: String,
    on_event: Channel<ChatEvent>,
) -> CmdResult<()> {
    check_request_id(&request_id)?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("消息不能为空".into());
    }
    if text.chars().count() > MAX_INPUT_CHARS {
        return Err(format!("消息太长了（上限 {MAX_INPUT_CHARS} 字）"));
    }
    app.state::<AppState>().daily.touch();
    tauri::async_runtime::spawn(chat::run_turn(app, text, request_id, on_event));
    Ok(())
}

#[tauri::command]
pub fn chat_regenerate(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    on_event: Channel<ChatEvent>,
) -> CmdResult<()> {
    check_request_id(&request_id)?;
    let text = {
        let db = state.db();
        conversation::latest(&db, &current_character_id(&state))
            .ok()
            .flatten()
            .and_then(|conv| conversation::pop_last_turn(&db, conv).ok().flatten())
    }
    .ok_or_else(|| "没有可以重新生成的消息".to_string())?;
    tauri::async_runtime::spawn(chat::run_turn(app, text, request_id, on_event));
    Ok(())
}

#[tauri::command]
pub fn chat_stop(state: State<'_, AppState>, request_id: String) -> bool {
    state.chat.stop(&request_id)
}

#[tauri::command]
pub async fn chat_set_mode(app: AppHandle, mode: ChatMode) -> CmdResult<()> {
    desktop::set_chat_mode(&app, mode).map_err(err)
}

/// 气泡模式：前端告诉后端内容需要多高（逻辑像素）
#[tauri::command]
pub async fn chat_fit(app: AppHandle, height: f64) -> CmdResult<()> {
    desktop::fit_compact(&app, height).map_err(err)
}

#[tauri::command]
pub async fn chat_hide(app: AppHandle) -> CmdResult<()> {
    desktop::hide_chat(&app).map_err(err)
}

// ---------------- 通用 ----------------

#[tauri::command]
pub async fn open_settings(app: AppHandle) -> CmdResult<()> {
    desktop::open_settings(&app).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_validation() {
        assert!(check_request_id("abc-123").is_ok());
        assert!(check_request_id("").is_err());
        assert!(check_request_id("a b").is_err());
        assert!(check_request_id(&"a".repeat(65)).is_err());
    }
}
