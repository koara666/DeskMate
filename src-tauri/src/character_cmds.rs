//! 设置窗口「角色」页用的命令：选择角色、禁词与自定义要求、用户图片。
//! 角色包本身只读：这里没有任何修改 character.json / persona.md 的命令。

use crate::character::{self, Origin, STATES};
use crate::desktop;
use crate::settings::CharacterPrefs;
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, State};

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
pub struct CharacterItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub color: String,
    pub origin: Origin,
    /// 待机形象（data URL）
    pub preview: String,
    /// 用户提供了图片的状态
    pub user_images: Vec<String>,
    /// 原作台词库（quotes.md）里常驻的段数 / 按关键词取用的段数
    pub quotes_fixed: usize,
    pub quotes_keyed: usize,
    /// 这个角色专属的禁词和自定义要求
    pub prefs: CharacterPrefs,
}

#[derive(Serialize)]
pub struct CharactersView {
    pub current: String,
    pub list: Vec<CharacterItem>,
    pub global: CharacterPrefs,
    /// 支持的表情状态（图片文件名）
    pub states: Vec<String>,
    /// 用户角色目录
    pub user_root: String,
}

fn view(state: &AppState) -> CharactersView {
    let s = state.settings();
    let current = state
        .current_character()
        .map(|c| c.manifest.id)
        .unwrap_or_default();
    let list = state
        .characters()
        .iter()
        .map(|c| CharacterItem {
            id: c.manifest.id.clone(),
            name: c.manifest.name.clone(),
            description: c.manifest.description.clone(),
            source: c.manifest.source.clone(),
            color: c.manifest.color.clone(),
            origin: c.origin,
            preview: character::appearance(c)
                .images
                .get("idle")
                .cloned()
                .unwrap_or_default(),
            user_images: character::user_image_states(c),
            quotes_fixed: c.quotes.fixed_count,
            quotes_keyed: c.quotes.keyed.len(),
            prefs: s
                .character_prefs
                .get(&c.manifest.id)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    CharactersView {
        current,
        list,
        global: s.global_prefs.clone(),
        states: STATES.iter().map(|s| s.to_string()).collect(),
        user_root: state.paths.characters().display().to_string(),
    }
}

#[tauri::command]
pub fn characters_list(state: State<'_, AppState>) -> CharactersView {
    view(&state)
}

/// 重新扫描角色目录（用户放好图片或新角色包之后）
#[tauri::command]
pub fn characters_reload(app: AppHandle, state: State<'_, AppState>) -> CharactersView {
    state.reload_characters();
    desktop::notify_character_changed(&app);
    view(&state)
}

#[tauri::command]
pub fn character_select(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if !state.characters().iter().any(|c| c.manifest.id == id) {
        return Err("找不到这个角色".into());
    }
    desktop::select_character(&app, &id);
    Ok(())
}

/// 保存全局偏好和某个角色的专属偏好
#[tauri::command]
pub fn prefs_save(
    app: AppHandle,
    state: State<'_, AppState>,
    global: CharacterPrefs,
    character_id: String,
    prefs: CharacterPrefs,
) -> CmdResult<()> {
    if !character::valid_id(&character_id) {
        return Err("角色 id 不合法".into());
    }
    state.update_settings(|s| {
        s.global_prefs = global;
        s.character_prefs.insert(character_id, prefs);
    });
    desktop::broadcast_settings_changed(&app);
    Ok(())
}

/// 打开某个角色的用户图片目录（不存在就创建）
#[tauri::command]
pub fn open_character_dir(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if !character::valid_id(&id) {
        return Err("角色 id 不合法".into());
    }
    let dir = state.paths.characters().join(&id).join("assets");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
