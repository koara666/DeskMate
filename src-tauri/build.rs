// 应用命令清单：登记后，每个窗口只能调用 capabilities/*.json 里明确授权的命令（最小权限）。
// 新增 #[tauri::command] 时在这里加一行，并在对应窗口的 capability 里授权 allow-<命令名>。
const COMMANDS: &[&str] = &[
    "pet_init",
    "pet_click",
    "pet_menu",
    "chat_init",
    "chat_send",
    "chat_regenerate",
    "chat_stop",
    "chat_set_mode",
    "chat_hide",
    "open_settings",
    "settings_get",
    "settings_save_general",
    "backend_save",
    "backend_add",
    "backend_delete",
    "backend_set_key",
    "backend_clear_key",
    "backend_select",
    "backend_test",
    "budget_save",
    "usage_get",
    "open_data_dir",
    "characters_list",
    "characters_reload",
    "character_select",
    "prefs_save",
    "open_character_dir",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build 脚本失败");
}
