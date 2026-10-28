//! DeskMate 应用入口：装配插件、状态、窗口、托盘和命令。

pub mod abilities;
pub mod activity;
pub mod backends;
pub mod briefing;
pub mod canon;
pub mod care;
pub mod character;
pub mod character_cmds;
pub mod chat;
pub mod commands;
pub mod companion;
pub mod daily;
pub mod db;
pub mod desktop;
pub mod diary;
pub mod filter;
pub mod games;
pub mod games_llm;
pub mod geometry;
pub mod lines;
pub mod logging;
pub mod markers;
pub mod memory;
pub mod memory_cmds;
pub mod notify;
pub mod paths;
pub mod privacy;
pub mod proactive;
pub mod prompt;
pub mod quotes;
pub mod redact;
pub mod reminders;
pub mod screen;
pub mod secrets;
pub mod settings;
pub mod settings_cmds;
pub mod state;
pub mod task_cmds;
pub mod tasks;
pub mod update;
pub mod usage;
pub mod vision;
pub mod worker;

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

    // 数据库损坏时自动从最近的备份恢复
    let (conn, recovery_note) = memory::backup::open_with_recovery(&paths.db(), &paths.backups())?;
    let builtin = builtin_characters_dir(app);
    let state = AppState::new(paths, conn, builtin, Box::new(secrets::KeyringStore));
    // 第一次使用：桌宠出现后主动弹出气泡说明怎么用（只出现一次；测试实例不弹，免得干扰自动测试）
    // （测试实例设置 DESKMATE_TEST_ONBOARDING=1 时也弹，用来检查引导界面）
    let onboarding = (!paths::test_mode()
        || std::env::var_os("DESKMATE_TEST_ONBOARDING").is_some())
        && !state.settings().onboarded;
    let mut notes: Vec<String> = recovery_note.into_iter().collect();
    if let Some(note) = notes.first() {
        log::warn!("{note}");
    }
    if onboarding {
        notes.push(daily::onboarding_text(&state.settings().chat_backend));
        state.update_settings(|s| s.onboarded = true);
    }
    if !notes.is_empty() {
        state.set_notice(notes.join(" "));
    }
    app.manage(state);

    let handle = app.handle().clone();
    desktop::create_pet(&handle)?;
    desktop::build_tray(&handle)?;
    if let Some(tab) = std::env::args().find_map(|a| desktop::parse_settings_arg(&a)) {
        desktop::open_settings_tab(&handle, tab)?;
    }

    // 后台维护：备份、整理记忆（低频）
    worker::spawn(handle.clone());
    // 日常体验：全屏检测、睡眠、主动行为调度（低频）
    daily::spawn(handle.clone());
    // 提醒：补上关机期间错过的，之后按下一个到期时间触发
    reminders::spawn(handle.clone());
    // 活动感知：前台窗口切换事件 + 每 30 秒看一眼
    activity::spawn(handle.clone());
    // 检查更新：1 分钟后查一次 GitHub（每天最多一次，可关）
    update::spawn_startup_check(&handle);
    {
        let d = handle.state::<AppState>().settings().daily;
        if let Err(e) = daily::apply_hotkey(&handle, d.hotkey_enabled, &d.hotkey) {
            log::warn!("{e}");
        }
        let v = handle.state::<AppState>().settings().vision;
        if let Err(e) = daily::apply_vision_hotkey(&handle, v.enabled, &v.hotkey) {
            log::warn!("{e}");
        }
    }

    // 桌宠出现后再在后台准备聊天窗口，不拖慢启动
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(800)).await;
        if let Err(e) = desktop::ensure_chat(&handle) {
            log::warn!("创建聊天窗口失败：{e}");
            return;
        }
        if onboarding {
            // 等聊天页面加载、量好气泡高度，再不抢焦点地弹出来
            tokio::time::sleep(Duration::from_millis(1500)).await;
            daily::pop_chat(&handle);
        }
    });
    Ok(())
}

pub fn run() {
    let mut builder = tauri::Builder::default();
    // 测试模式下不加单实例锁，才能和正在使用的 DeskMate 同时运行
    if !paths::test_mode() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 第二个实例启动时：把已有的桌宠显示出来；带 --settings 时打开设置
            if let Some(w) = desktop::pet(app) {
                let _ = desktop::show_window(&w);
            }
            if let Some(tab) = argv.iter().find_map(|a| desktop::parse_settings_arg(a)) {
                desktop::open_settings_tab_async(app, tab);
            }
        }));
    }
    let app = builder
        // 系统通知：只在 Rust 端用（全屏时的提醒），不给任何窗口授权
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| daily::on_hotkey(app, shortcut, event.state))
                .build(),
        )
        .setup(setup)
        // 用户角色图片：按地址读取（在后台线程读文件，不卡界面）
        .register_asynchronous_uri_scheme_protocol(character::IMG_SCHEME, |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            let path = req.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || {
                let st = app.state::<AppState>();
                let id = path.trim_start_matches('/').split('/').next().unwrap_or("");
                let found = st.character(id);
                let resp = match character::serve_image(found.as_ref(), &path) {
                    Some((bytes, mime)) => tauri::http::Response::builder()
                        .header("Content-Type", mime)
                        .header("X-Content-Type-Options", "nosniff")
                        // 地址里带了图片的修改时间，换图后地址就变，可以放心缓存
                        .header("Cache-Control", "max-age=31536000, immutable")
                        .body(bytes),
                    None => tauri::http::Response::builder()
                        .status(404)
                        .body(Vec::new()),
                };
                if let Ok(r) = resp {
                    responder.respond(r);
                }
            });
        })
        .on_menu_event(|app, e| desktop::handle_menu(app, e.id().as_ref()))
        .on_window_event(|w, e| match (w.label(), e) {
            ("pet", WindowEvent::Moved(_)) => desktop::on_pet_moved(w.app_handle()),
            // 桌宠和聊天窗口不真正关闭（比如误按 Alt+F4），只隐藏
            ("pet" | "chat", WindowEvent::CloseRequested { api, .. }) => {
                api.prevent_close();
                if let Some(ww) = w.app_handle().get_webview_window(w.label()) {
                    let _ = desktop::hide_window(&ww);
                } else {
                    let _ = w.hide();
                }
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
            commands::chat_fit,
            commands::chat_hide,
            commands::open_settings,
            commands::ability_open,
            commands::ability_read,
            commands::proactive_feedback,
            commands::reminder_save,
            commands::pet_touch,
            commands::event_choose,
            commands::game_say,
            commands::game_finish,
            commands::game_stats,
            commands::fortune_today,
            commands::fortune_draw,
            commands::llm_game_start,
            commands::llm_game_turn,
            commands::llm_game_give_up,
            settings_cmds::reminder_list,
            settings_cmds::reminder_delete,
            settings_cmds::reminder_set_status,
            settings_cmds::companion_save,
            settings_cmds::advanced_save,
            settings_cmds::briefing_save,
            settings_cmds::city_search,
            settings_cmds::briefing_now,
            settings_cmds::activity_overview,
            settings_cmds::activity_save,
            settings_cmds::activity_delete,
            settings_cmds::care_save,
            settings_cmds::diary_list,
            settings_cmds::diary_delete,
            settings_cmds::companion_status,
            settings_cmds::nickname_save,
            settings_cmds::vision_save,
            settings_cmds::vision_cost,
            settings_cmds::vision_preview,
            settings_cmds::vision_records,
            settings_cmds::update_check,
            settings_cmds::update_save,
            commands::update_install,
            commands::update_skip,
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
            character_cmds::characters_list,
            character_cmds::characters_reload,
            character_cmds::character_select,
            character_cmds::prefs_save,
            character_cmds::open_character_dir,
            memory_cmds::memory_overview,
            memory_cmds::memory_list,
            memory_cmds::memory_update,
            memory_cmds::memory_delete,
            memory_cmds::memory_add,
            memory_cmds::memory_set_paused,
            memory_cmds::memory_clear_all,
            memory_cmds::memory_export,
            memory_cmds::memory_import_list,
            memory_cmds::memory_import,
            task_cmds::task_defaults,
            task_cmds::task_settings_save,
            task_cmds::task_start,
            task_cmds::task_approve,
            task_cmds::task_decline,
            task_cmds::task_cancel,
            task_cmds::task_list,
            task_cmds::task_open_log,
            task_cmds::chat_task_report,
            settings_cmds::daily_save,
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
