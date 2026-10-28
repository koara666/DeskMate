//! 桌面相关：窗口创建与摆放、拖动吸附、托盘、右键菜单。
//! 几何计算在 `geometry.rs`（纯函数、有单元测试），这里只做和 Tauri 打交道的部分。

use crate::geometry::{self, Rect, Side};
use crate::settings::Corner;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow,
    WebviewWindowBuilder, Wry,
};

pub const PET_LOGICAL: (f64, f64) = (160.0, 180.0);
pub const CHAT_COMPACT: (f64, f64) = (340.0, 250.0);
pub const CHAT_EXPANDED: (f64, f64) = (380.0, 560.0);
const SNAP_LOGICAL: f64 = 24.0;
const MARGIN_LOGICAL: f64 = 8.0;
const GAP_LOGICAL: f64 = 4.0;
const TRAY_ID: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatMode {
    #[default]
    Compact,
    Expanded,
}

#[derive(Default)]
pub struct DesktopState {
    move_gen: AtomicU64,
    chat_mode: Mutex<ChatMode>,
    chat_side: Mutex<Option<Side>>,
    /// 设置窗口下次打开时要显示的页面（由 --settings=<页面> 启动参数设置）
    settings_tab: Mutex<Option<String>>,
}

impl DesktopState {
    pub fn chat_mode(&self) -> ChatMode {
        *self.chat_mode.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// 取走待显示的设置页面（只生效一次）
    pub fn take_settings_tab(&self) -> Option<String> {
        self.settings_tab
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    pub fn chat_side(&self) -> Side {
        self.chat_side
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unwrap_or(Side::Left)
    }
}

fn window_config(app: &AppHandle, label: &str) -> tauri::utils::config::WindowConfig {
    app.config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .cloned()
        .unwrap_or_else(|| panic!("tauri.conf.json 缺少 {label} 窗口配置"))
}

/// WebView2 浏览器参数（DECISIONS #25）：
/// - 默认关掉 GPU 加速：GPU 进程从约 95 MB 降到约 13 MB，透明窗口显示不变，说话动画的 CPU 反而更低；
///   设置里可以重新打开（重启后生效）。
/// - `--process-per-site`：三个窗口同源，共用一个渲染进程，省掉约 25 MB。
/// - 前面一段是 wry 的默认参数，自定义参数时必须保留。
pub fn browser_args(gpu: bool, cdp_port: Option<u16>) -> String {
    let mut a = String::from(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --process-per-site",
    );
    if !gpu {
        a.push_str(" --disable-gpu");
    }
    if let Some(port) = cdp_port {
        a.push_str(&format!(" --remote-debugging-port={port}"));
    }
    a
}

/// 同一个 WebView 数据目录下所有窗口的浏览器参数必须一致，所以启动时算一次，之后不再变
static BROWSER_ARGS: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// 按配置准备窗口；测试模式下使用独立的 WebView 数据目录（以及可选的调试端口）
fn window_builder<'a>(
    app: &'a AppHandle,
    label: &str,
) -> tauri::Result<WebviewWindowBuilder<'a, Wry, AppHandle>> {
    let mut b = WebviewWindowBuilder::from_config(app, &window_config(app, label))?;
    let st = app.state::<AppState>();
    if crate::paths::test_mode() {
        b = b.data_directory(st.paths.root().join("EBWebView"));
    }
    let args = BROWSER_ARGS
        .get_or_init(|| browser_args(st.settings().gpu_acceleration, crate::paths::cdp_port()));
    Ok(b.additional_browser_args(args))
}

fn work_rect(m: &tauri::Monitor) -> Rect {
    let wa = m.work_area();
    Rect::new(
        wa.position.x,
        wa.position.y,
        wa.size.width as i32,
        wa.size.height as i32,
    )
}

fn window_rect(w: &WebviewWindow) -> tauri::Result<Rect> {
    let p = w.outer_position()?;
    let s = w.outer_size()?;
    Ok(Rect::new(p.x, p.y, s.width as i32, s.height as i32))
}

/// 所有显示器的工作区（去掉任务栏），以及主显示器的工作区
fn monitors(w: &WebviewWindow) -> tauri::Result<(Vec<Rect>, Rect)> {
    let works: Vec<Rect> = w.available_monitors()?.iter().map(work_rect).collect();
    let primary = w
        .primary_monitor()?
        .map(|m| work_rect(&m))
        .or_else(|| works.first().copied())
        .unwrap_or(Rect::new(0, 0, 1280, 720));
    Ok((works, primary))
}

/// 窗口当前所在显示器的工作区
fn current_work(w: &WebviewWindow) -> tauri::Result<Rect> {
    let (works, primary) = monitors(w)?;
    let r = window_rect(w)?;
    Ok(geometry::pick_monitor(r, &works).map_or(primary, |i| works[i]))
}

fn px(logical: f64, scale: f64) -> i32 {
    (logical * scale).round() as i32
}

pub fn pet(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("pet")
}

pub fn chat(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("chat")
}

/// 启动时创建桌宠窗口：恢复上次位置（显示器不在了就回到主屏默认角落）
pub fn create_pet(app: &AppHandle) -> tauri::Result<()> {
    let s = app.state::<AppState>().settings();
    let w = window_builder(app, "pet")?
        .always_on_top(s.always_on_top)
        .build()?;
    w.set_size(LogicalSize::new(
        PET_LOGICAL.0 * s.pet_scale,
        PET_LOGICAL.1 * s.pet_scale,
    ))?;
    let scale = w.scale_factor()?;
    let size = w.outer_size()?;
    let (works, primary) = monitors(&w)?;
    let (x, y) = geometry::restore_position(
        s.pet_position.map(|p| (p.x, p.y)),
        (size.width as i32, size.height as i32),
        &works,
        primary,
        s.corner,
        px(MARGIN_LOGICAL, scale),
    );
    w.set_position(PhysicalPosition::new(x, y))?;
    w.show()?;
    Ok(())
}

/// 预先创建（隐藏的）聊天窗口，第一次点击时不用等
pub fn ensure_chat(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = chat(app) {
        return Ok(w);
    }
    let s = app.state::<AppState>().settings();
    window_builder(app, "chat")?
        .always_on_top(s.always_on_top)
        .build()
}

/// 桌宠窗口移动时调用：聊天窗口跟着走；停止移动后吸附边缘并保存位置
pub fn on_pet_moved(app: &AppHandle) {
    let _ = position_chat(app);
    let st = app.state::<AppState>();
    let generation = st.desktop.move_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let st = app.state::<AppState>();
            if st.desktop.move_gen.load(Ordering::SeqCst) != generation {
                return; // 还在动，交给后面的事件处理
            }
            if !mouse_button_down() {
                break;
            }
        }
        if let Err(e) = finalize_pet_position(&app) {
            log::warn!("保存桌宠位置失败：{e}");
        }
    });
}

#[cfg(windows)]
fn mouse_button_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    // 最高位为 1 表示按键正被按住
    unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn mouse_button_down() -> bool {
    false
}

fn finalize_pet_position(app: &AppHandle) -> tauri::Result<()> {
    let Some(w) = pet(app) else { return Ok(()) };
    let r = window_rect(&w)?;
    let work = current_work(&w)?;
    let (x, y) = geometry::snap_to_edges(r, work, px(SNAP_LOGICAL, w.scale_factor()?));
    if (x, y) != (r.x, r.y) {
        w.set_position(PhysicalPosition::new(x, y))?;
    }
    let st = app.state::<AppState>();
    if st.settings().pet_position != Some(crate::settings::SavedPosition { x, y }) {
        st.update_settings(|s| s.pet_position = Some(crate::settings::SavedPosition { x, y }));
    }
    Ok(())
}

/// 改变桌宠大小后重新摆放（保持在屏幕内并吸附边缘）
pub fn apply_pet_scale(app: &AppHandle) -> tauri::Result<()> {
    let Some(w) = pet(app) else { return Ok(()) };
    let scale = app.state::<AppState>().settings().pet_scale;
    let r = window_rect(&w)?;
    w.set_size(LogicalSize::new(
        PET_LOGICAL.0 * scale,
        PET_LOGICAL.1 * scale,
    ))?;
    // 以底边中点为锚点缩放，看起来像「站在原地变大变小」
    let s = w.outer_size()?;
    let (cx, bottom) = (r.x + r.w / 2, r.bottom());
    w.set_position(PhysicalPosition::new(
        cx - s.width as i32 / 2,
        bottom - s.height as i32,
    ))?;
    finalize_pet_position(app)?;
    position_chat(app)
}

/// 停靠到某个角落
pub fn dock(app: &AppHandle, corner: Corner) -> tauri::Result<()> {
    let Some(w) = pet(app) else { return Ok(()) };
    let size = w.outer_size()?;
    let work = current_work(&w)?;
    let (x, y) = geometry::corner_position(
        corner,
        (size.width as i32, size.height as i32),
        work,
        px(MARGIN_LOGICAL, w.scale_factor()?),
    );
    w.set_position(PhysicalPosition::new(x, y))?;
    app.state::<AppState>().update_settings(|s| {
        s.corner = corner;
        s.pet_position = Some(crate::settings::SavedPosition { x, y });
    });
    Ok(())
}

/// 按当前模式摆放聊天窗口，并告诉它气泡尾巴朝哪边
pub fn position_chat(app: &AppHandle) -> tauri::Result<()> {
    let (Some(p), Some(c)) = (pet(app), chat(app)) else {
        return Ok(());
    };
    if !c.is_visible()? {
        return Ok(());
    }
    let st = app.state::<AppState>();
    let scale = p.scale_factor()?;
    let (lw, lh) = match st.desktop.chat_mode() {
        ChatMode::Compact => CHAT_COMPACT,
        ChatMode::Expanded => CHAT_EXPANDED,
    };
    let size = (px(lw, scale), px(lh, scale));
    let pr = window_rect(&p)?;
    let work = current_work(&p)?;
    let (x, y, side) = geometry::chat_position(pr, size, work, px(GAP_LOGICAL, scale));
    c.set_size(LogicalSize::new(lw, lh))?;
    c.set_position(PhysicalPosition::new(x, y))?;
    let mut last = st
        .desktop
        .chat_side
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if *last != Some(side) {
        *last = Some(side);
        let _ = app.emit_to("chat", "chat-placement", side);
    }
    Ok(())
}

pub fn show_chat(app: &AppHandle, focus: bool) -> tauri::Result<()> {
    let c = ensure_chat(app)?;
    c.show()?;
    position_chat(app)?;
    if focus {
        c.set_focus()?;
        let _ = app.emit_to("chat", "chat-focus-input", ());
    }
    Ok(())
}

pub fn hide_chat(app: &AppHandle) -> tauri::Result<()> {
    if let Some(c) = chat(app) {
        c.hide()?;
    }
    Ok(())
}

pub fn toggle_chat(app: &AppHandle) -> tauri::Result<()> {
    match chat(app) {
        Some(c) if c.is_visible()? => hide_chat(app),
        _ => show_chat(app, true),
    }
}

pub fn set_chat_mode(app: &AppHandle, mode: ChatMode) -> tauri::Result<()> {
    *app.state::<AppState>()
        .desktop
        .chat_mode
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = mode;
    position_chat(app)
}

pub fn toggle_pet_visible(app: &AppHandle) -> tauri::Result<()> {
    let Some(p) = pet(app) else { return Ok(()) };
    if p.is_visible()? {
        hide_chat(app)?;
        p.hide()?;
    } else {
        p.show()?;
    }
    Ok(())
}

pub fn set_always_on_top(app: &AppHandle, on: bool) -> tauri::Result<()> {
    app.state::<AppState>()
        .update_settings(|s| s.always_on_top = on);
    for w in [pet(app), chat(app)].into_iter().flatten() {
        w.set_always_on_top(on)?;
    }
    Ok(())
}

/// 设置窗口允许直接打开的页面
pub const SETTINGS_TABS: [&str; 7] = [
    "general",
    "characters",
    "memory",
    "models",
    "tasks",
    "usage",
    "about",
];

/// 解析启动参数：`--settings` 或 `--settings=<页面>`；不是这个参数时返回 None
pub fn parse_settings_arg(arg: &str) -> Option<Option<String>> {
    if arg == "--settings" {
        return Some(None);
    }
    let tab = arg.strip_prefix("--settings=")?;
    Some(SETTINGS_TABS.contains(&tab).then(|| tab.to_string()))
}

/// 打开设置窗口并跳到指定页面。可能从主线程调用（启动参数、第二个实例），所以放到后台执行。
pub fn open_settings_tab_async(app: &AppHandle, tab: Option<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = open_settings_tab(&app, tab) {
            log::warn!("打开设置窗口失败：{e}");
        }
    });
}

/// 打开设置窗口并跳到指定页面（不要在主线程上直接调用）
pub fn open_settings_tab(app: &AppHandle, tab: Option<String>) -> tauri::Result<()> {
    if let Some(t) = tab {
        *app.state::<AppState>()
            .desktop
            .settings_tab
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(t.clone());
        let _ = app.emit_to("settings", "settings-tab", t);
    }
    open_settings(app)
}

pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("settings") {
        w.show()?;
        w.unminimize()?;
        return w.set_focus();
    }
    window_builder(app, "settings")?.build()?;
    Ok(())
}

/// 设置变化后通知所有窗口刷新
pub fn broadcast_settings_changed(app: &AppHandle) {
    let _ = app.emit("settings-changed", ());
    refresh_tray_menu(app);
}

/// 角色或角色图片变化后通知所有窗口刷新
pub fn notify_character_changed(app: &AppHandle) {
    let _ = app.emit("character-changed", ());
    refresh_tray_menu(app);
}

/// 切换当前角色（托盘菜单、右键菜单、设置界面共用）
pub fn select_character(app: &AppHandle, id: &str) {
    let id = id.to_string();
    app.state::<AppState>()
        .update_settings(|s| s.character_id = id);
    notify_character_changed(app);
}

// ---------------- 菜单（托盘和右键共用） ----------------

fn build_menu(app: &AppHandle, for_tray: bool) -> tauri::Result<Menu<Wry>> {
    let st = app.state::<AppState>();
    let s = st.settings();
    let pet_visible = pet(app).and_then(|p| p.is_visible().ok()).unwrap_or(true);

    let current = st.current_character().map(|c| c.manifest.id);
    let mut char_items = Vec::new();
    for c in st.characters() {
        char_items.push(CheckMenuItem::with_id(
            app,
            format!("char:{}", c.manifest.id),
            &c.manifest.name,
            true,
            current.as_deref() == Some(c.manifest.id.as_str()),
            None::<&str>,
        )?);
    }
    let char_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = char_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    let characters = Submenu::with_id_and_items(app, "characters", "切换角色", true, &char_refs)?;

    let corners = Submenu::with_id_and_items(
        app,
        "corners",
        "停靠位置",
        true,
        &[
            &MenuItem::with_id(app, "corner:top_left", "左上角", true, None::<&str>)?,
            &MenuItem::with_id(app, "corner:top_right", "右上角", true, None::<&str>)?,
            &MenuItem::with_id(app, "corner:bottom_left", "左下角", true, None::<&str>)?,
            &MenuItem::with_id(app, "corner:bottom_right", "右下角", true, None::<&str>)?,
        ],
    )?;

    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let toggle_text = if pet_visible {
        "隐藏桌宠"
    } else {
        "显示桌宠"
    };
    let toggle = MenuItem::with_id(app, "toggle_pet", toggle_text, true, None::<&str>)?;
    let open_chat = MenuItem::with_id(app, "open_chat", "聊天", true, None::<&str>)?;
    let dnd = CheckMenuItem::with_id(app, "dnd", "勿扰模式", true, s.dnd, None::<&str>)?;
    let top = CheckMenuItem::with_id(
        app,
        "always_on_top",
        "总在最前",
        true,
        s.always_on_top,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 DeskMate", true, None::<&str>)?;

    if for_tray {
        Menu::with_items(
            app,
            &[&toggle, &dnd, &characters, &sep, &settings, &sep2, &quit],
        )
    } else {
        Menu::with_items(
            app,
            &[
                &open_chat,
                &characters,
                &corners,
                &sep,
                &dnd,
                &top,
                &settings,
                &sep2,
                &toggle,
                &quit,
            ],
        )
    }
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, true)?;
    let mut b = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("DeskMate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = toggle_pet_visible(app);
                refresh_tray_menu(app);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

pub fn refresh_tray_menu(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Ok(menu) = build_menu(app, true)
    {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn popup_pet_menu(app: &AppHandle) -> tauri::Result<()> {
    let Some(p) = pet(app) else { return Ok(()) };
    let menu = build_menu(app, false)?;
    p.popup_menu(&menu)
}

/// 托盘菜单和右键菜单的点击处理
pub fn handle_menu(app: &AppHandle, id: &str) {
    // 菜单回调在主线程：放到后台执行，避免在主线程里创建窗口导致死锁
    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move { handle_menu_now(&app, &id) });
}

fn handle_menu_now(app: &AppHandle, id: &str) {
    let st = app.state::<AppState>();
    let r: tauri::Result<()> = (|| {
        match id {
            "toggle_pet" => toggle_pet_visible(app)?,
            "open_chat" => show_chat(app, true)?,
            "dnd" => {
                st.update_settings(|s| s.dnd = !s.dnd);
            }
            "always_on_top" => {
                let on = !st.settings().always_on_top;
                set_always_on_top(app, on)?;
            }
            "settings" => open_settings(app)?,
            "quit" => app.exit(0),
            _ => {
                if let Some(cid) = id.strip_prefix("char:") {
                    select_character(app, cid);
                } else if let Some(c) = id.strip_prefix("corner:") {
                    let corner = match c {
                        "top_left" => Corner::TopLeft,
                        "top_right" => Corner::TopRight,
                        "bottom_left" => Corner::BottomLeft,
                        _ => Corner::BottomRight,
                    };
                    dock(app, corner)?;
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = r {
        log::warn!("菜单操作失败（{id}）：{e}");
    }
    broadcast_settings_changed(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_args_keep_wry_defaults() {
        let off = browser_args(false, None);
        assert!(off.starts_with("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"));
        assert!(off.contains("--process-per-site"));
        assert!(off.contains("--disable-gpu"));
        assert!(!off.contains("remote-debugging"));
        let on = browser_args(true, Some(9333));
        assert!(!on.contains("--disable-gpu"));
        assert!(on.ends_with("--remote-debugging-port=9333"));
    }

    #[test]
    fn settings_arg_parsing() {
        assert_eq!(parse_settings_arg("--settings"), Some(None));
        assert_eq!(
            parse_settings_arg("--settings=characters"),
            Some(Some("characters".into()))
        );
        assert_eq!(parse_settings_arg("--settings=../../etc"), Some(None));
        assert_eq!(parse_settings_arg("--other"), None);
    }
}
