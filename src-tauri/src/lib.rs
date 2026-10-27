//! DeskMate 应用入口：装配插件、状态、窗口、托盘和命令。

pub mod backends;
pub mod character;
pub mod chat;
pub mod commands;
pub mod db;
pub mod desktop;
pub mod filter;
pub mod geometry;
pub mod logging;
pub mod paths;
pub mod prompt;
pub mod redact;
pub mod secrets;
pub mod settings;
pub mod settings_cmds;
pub mod state;
pub mod usage;

use paths::DataPaths;
use state::AppState;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{Manager, RunEvent, WindowEvent};

/// 内置角色包目录：安装包里在资源目录下；开发时直接用仓库里的 characters/
fn builtin_characters_dir(app: &tauri::App) -> Option<PathBuf> {
    let bundled = app.path().resource_dir().ok().map(|d| d.join("characters"));
    if let Some(d) = bundled.filter(|d| d.is_dir()) {
        return Some(d);
    }
    if cfg!(debug_assertions) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("characters");
        if dev.is_dir() {
            return Some(dev);
        }
    }
    None
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let paths = DataPaths::resolve(app.path().data_dir().ok())?;
    paths.ensure()?;
    logging::init(&paths.logs());
    log::info!("DeskMate {} 启动", env!("CARGO_PKG_VERSION"));

    let conn = db::open(&paths.db(), &paths.backups())?;
    let builtin = builtin_characters_dir(app);
    app.manage(AppState::new(
        paths,
        conn,
        builtin,
        Box::new(secrets::KeyringStore),
    ));

    let handle = app.handle().clone();
    desktop::create_pet(&handle)?;
    desktop::build_tray(&handle)?;

    // 桌宠出现后再在后台准备聊天窗口，不拖慢启动
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(800)).await;
        if let Err(e) = desktop::ensure_chat(&handle) {
            log::warn!("创建聊天窗口失败：{e}");
        }
    });
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 第二个实例启动时，只把已有的桌宠显示出来
            if let Some(w) = desktop::pet(app) {
                let _ = w.show();
            }
        }))
        .setup(setup)
        .on_menu_event(|app, e| desktop::handle_menu(app, e.id().as_ref()))
        .on_window_event(|w, e| match (w.label(), e) {
            ("pet", WindowEvent::Moved(_)) => desktop::on_pet_moved(w.app_handle()),
            // 桌宠和聊天窗口不真正关闭（比如误按 Alt+F4），只隐藏
            ("pet" | "chat", WindowEvent::CloseRequested { api, .. }) => {
                api.prevent_close();
                let _ = w.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::pet_init,
            commands::pet_click,
            commands::pet_menu,
            commands::chat_init,
            commands::chat_send,
            commands::chat_regenerate,
            commands::chat_stop,
            commands::chat_set_mode,
            commands::chat_hide,
            commands::open_settings,
            settings_cmds::settings_get,
            settings_cmds::settings_save_general,
            settings_cmds::backend_save,
            settings_cmds::backend_add,
            settings_cmds::backend_delete,
            settings_cmds::backend_set_key,
            settings_cmds::backend_clear_key,
            settings_cmds::backend_select,
            settings_cmds::backend_test,
            settings_cmds::budget_save,
            settings_cmds::usage_get,
            settings_cmds::open_data_dir,
        ])
        .build(tauri::generate_context!())
        .expect("DeskMate 启动失败");

    app.run(|_app, event| {
        // 关掉设置窗口等情况不退出程序；只有菜单里的「退出」（带退出码）才真正退出
        if let RunEvent::ExitRequested { api, code, .. } = event
            && code.is_none()
        {
            api.prevent_exit();
        }
    });
}
