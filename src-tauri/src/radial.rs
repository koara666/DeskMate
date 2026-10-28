//! 头顶气泡菜单（v2.1 P11，DECISIONS #49）：右键桌宠时，她头顶浮出一圈气泡选项。
//!
//! - 右键单击：气泡出现后可以慢慢点选（菜单窗口拿到焦点，点别处或 Esc 就收起）。
//! - 按住右键不放：气泡出现，按着把鼠标移到某个气泡上、松开就选中。按住期间后端每 16 毫秒读一次鼠标位置和右键状态
//!   （只在按住的这几秒里读，松开就停），换算成菜单窗口里的坐标发给菜单页面，由页面自己判断指着哪个气泡。
//! - 菜单是一个单独的透明小窗口（不在任务栏、总在最前、不抢焦点地显示），用完藏起来，下次直接用。
//! - 彩蛋：反复选同一个、勿扰来回开关、打开了又不选、按住很久不选、偶尔出现的「？？？」气泡……（台词池 `menu.*`）

use crate::desktop;
use crate::geometry::{self, Rect};
use crate::state::AppState;
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MenuItem {
    pub id: &'static str,
    pub icon: &'static str,
    pub label: String,
    /// 开着的开关（勿扰）
    pub on: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuShow {
    pub items: Vec<MenuItem>,
    /// 放在桌宠下面（气泡往下排）
    pub below: bool,
    /// 按住右键打开的（松开时才决定选不选）
    pub hold: bool,
    /// 当前角色的界面皮肤
    pub theme: crate::character::Theme,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MenuPointer {
    /// 菜单窗口里的逻辑坐标
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MenuRelease {
    pub x: f64,
    pub y: f64,
    /// 按了多久（毫秒）
    pub held_ms: i64,
}

/// 菜单里有什么（纯函数）。`secret`：这次出现神秘气泡
pub fn items(dnd: bool, vision: bool, voice: Option<bool>, secret: bool) -> Vec<MenuItem> {
    let mut v = vec![
        MenuItem {
            id: "chat",
            icon: "💬",
            label: "聊天".into(),
            on: false,
        },
        MenuItem {
            id: "games",
            icon: "🎮",
            label: "一起玩".into(),
            on: false,
        },
        MenuItem {
            id: "remind",
            icon: "⏰",
            label: "提醒".into(),
            on: false,
        },
    ];
    if vision {
        v.push(MenuItem {
            id: "look",
            icon: "👀",
            label: "看一眼".into(),
            on: false,
        });
    }
    v.push(MenuItem {
        id: "dnd",
        icon: "🌙",
        label: if dnd {
            "勿扰中".into()
        } else {
            "勿扰".into()
        },
        on: dnd,
    });
    if let Some(on) = voice {
        v.push(MenuItem {
            id: "voice",
            icon: if on { "🔊" } else { "🔇" },
            label: if on {
                "声音开".into()
            } else {
                "声音关".into()
            },
            on,
        });
    }
    v.push(MenuItem {
        id: "hide",
        icon: "🙈",
        label: "躲起来".into(),
        on: false,
    });
    v.push(MenuItem {
        id: "settings",
        icon: "⚙",
        label: "设置".into(),
        on: false,
    });
    v.push(MenuItem {
        id: "more",
        icon: "⋯",
        label: "更多".into(),
        on: false,
    });
    if secret {
        // 插在中间，像是本来就在那里
        let mid = v.len() / 2;
        v.insert(
            mid,
            MenuItem {
                id: "secret",
                icon: "？",
                label: "？？？".into(),
                on: false,
            },
        );
    }
    v
}

/// 彩蛋的记录
#[derive(Default)]
pub struct MenuState {
    /// 最近选过的（时间秒, id）
    picks: Mutex<Vec<(i64, String)>>,
    /// 最近打开了又没选的时间
    cancels: Mutex<Vec<i64>>,
    /// 菜单是按住右键打开的、正在等松开
    holding: std::sync::atomic::AtomicBool,
    opened_ms: std::sync::atomic::AtomicI64,
}

/// 选完以后的彩蛋（纯函数）：返回台词场景
pub fn pick_egg(recent: &[(i64, String)], id: &str, now: i64) -> Option<&'static str> {
    let same = recent
        .iter()
        .filter(|(t, x)| x == id && now - t <= 90)
        .count();
    if id == "dnd"
        && recent
            .iter()
            .filter(|(t, x)| x == "dnd" && now - t <= 40)
            .count()
            >= 4
    {
        return Some("menu.dnd_spam");
    }
    if id != "dnd" && id != "more" && same >= 3 {
        return Some("menu.repeat");
    }
    None
}

fn menu_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("menu")
}

/// 预先建好（藏着的）菜单窗口，第一次右键时不用等
pub fn ensure(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = menu_window(app) {
        return Ok(w);
    }
    desktop::build_window(app, "menu")
}

fn place(app: &AppHandle, w: &WebviewWindow) -> tauri::Result<bool> {
    let Some(p) = desktop::pet(app) else {
        return Ok(false);
    };
    let scale = p.scale_factor()?;
    w.set_size(LogicalSize::new(
        desktop::MENU_LOGICAL.0,
        desktop::MENU_LOGICAL.1,
    ))?;
    let pp = p.outer_position()?;
    let ps = p.outer_size()?;
    let pr = Rect::new(pp.x, pp.y, ps.width as i32, ps.height as i32);
    let work = desktop::current_work_of(&p)?;
    let size = (
        (desktop::MENU_LOGICAL.0 * scale).round() as i32,
        (desktop::MENU_LOGICAL.1 * scale).round() as i32,
    );
    let (x, y, below) = geometry::menu_position(pr, size, work);
    w.set_position(PhysicalPosition::new(x, y))?;
    Ok(below)
}

/// 桌宠被拖动时菜单跟着走（一般不会发生：开着菜单时拖不了）
pub fn position_if_open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = menu_window(app)
        && desktop::window_visible(&w)
    {
        place(app, &w)?;
    }
    Ok(())
}

/// 右键按下：打开菜单。`hold` = 由按下右键触发（按住期间读鼠标位置，松开再决定）
pub fn open(app: &AppHandle, hold: bool) -> tauri::Result<()> {
    let st = app.state::<AppState>();
    let w = ensure(app)?;
    let below = place(app, &w)?;
    let s = st.settings();
    let secret = crate::lines::seed() % 15 == 0;
    let show = MenuShow {
        items: items(
            s.dnd,
            s.vision.enabled,
            crate::voice::menu_state(&s),
            secret,
        ),
        below,
        hold,
        theme: st
            .current_character()
            .map(|c| c.manifest.theme.sanitized())
            .unwrap_or_default(),
    };
    let _ = app.emit_to("menu", "menu-show", show);
    crate::daily::show_no_activate(&w);
    let generation = st.desktop.next_menu_gen();
    st.radial.holding.store(hold, Ordering::SeqCst);
    st.radial
        .opened_ms
        .store(desktop::now_ms(), Ordering::SeqCst);
    if hold {
        spawn_hold_tracker(app.clone(), w, generation);
    }
    Ok(())
}

/// 按住右键期间：读鼠标位置 → 发给菜单页面；松开时告诉它松在哪
fn spawn_hold_tracker(app: AppHandle, w: WebviewWindow, generation: u64) {
    tauri::async_runtime::spawn(async move {
        let start = desktop::now_ms();
        let mut last: Option<(i32, i32)> = None;
        loop {
            tokio::time::sleep(Duration::from_millis(16)).await;
            let st = app.state::<AppState>();
            if st.desktop.menu_gen() != generation {
                return;
            }
            let cur = desktop::cursor_pos();
            let to_local = |(cx, cy): (i32, i32)| -> Option<(f64, f64)> {
                let pos = w.outer_position().ok()?;
                let scale = w.scale_factor().ok()?;
                Some(((cx - pos.x) as f64 / scale, (cy - pos.y) as f64 / scale))
            };
            if !desktop::right_button_down() || desktop::now_ms() - start > 15_000 {
                st.radial.holding.store(false, Ordering::SeqCst);
                if let Some((x, y)) = cur.and_then(to_local) {
                    let _ = app.emit_to(
                        "menu",
                        "menu-release",
                        MenuRelease {
                            x,
                            y,
                            held_ms: desktop::now_ms() - start,
                        },
                    );
                }
                return;
            }
            if cur != last {
                last = cur;
                if let Some((x, y)) = cur.and_then(to_local) {
                    let _ = app.emit_to("menu", "menu-pointer", MenuPointer { x, y });
                }
            }
        }
    });
}

/// 收起菜单。`picked = false` 是打开了又没选（彩蛋：犹豫不决、按住很久不选）
pub fn close(app: &AppHandle, picked: bool, held_ms: i64) {
    let st = app.state::<AppState>();
    st.desktop.next_menu_gen();
    if let Some(w) = menu_window(app) {
        let _ = desktop::hide_window(&w);
    }
    if picked {
        return;
    }
    let now = crate::memory::now_secs();
    let scene = {
        let mut c = st.radial.cancels.lock().unwrap_or_else(|e| e.into_inner());
        c.retain(|t| now - t <= 60);
        c.push(now);
        if held_ms >= 4000 {
            Some("menu.hold_long")
        } else if c.len() >= 3 {
            c.clear();
            Some("menu.indecisive")
        } else {
            None
        }
    };
    if let Some(scene) = scene {
        crate::companion::say_reaction(app, scene, &[]);
    }
}

/// 让菜单拿到焦点（右键单击后慢慢选：点别处就收起）
pub fn focus(app: &AppHandle) {
    if let Some(w) = menu_window(app) {
        let _ = w.set_focus();
    }
}

/// 选了一项
pub fn pick(app: &AppHandle, id: &str) -> Result<(), String> {
    let st = app.state::<AppState>();
    close(app, true, 0);
    crate::daily::interact(app);
    let now = crate::memory::now_secs();
    let egg = {
        let mut p = st.radial.picks.lock().unwrap_or_else(|e| e.into_inner());
        p.retain(|(t, _)| now - t <= 90);
        p.push((now, id.to_string()));
        let e = pick_egg(&p, id, now);
        if e.is_some() {
            p.retain(|(_, x)| x != id);
        }
        e
    };
    let r: tauri::Result<()> = (|| {
        match id {
            "chat" => desktop::show_chat(app, true)?,
            "games" => {
                desktop::show_chat(app, true)?;
                let _ = app.emit_to("chat", "chat-open-games", None::<String>);
            }
            "remind" => {
                desktop::show_chat(app, true)?;
                let say = st
                    .line("menu.remind", &[])
                    .unwrap_or_else(|| "要我记着什么？直接说「X 点提醒我……」也可以。".into());
                let _ = app.emit_to("chat", "chat-reminder-form", say);
            }
            "look" => crate::vision::look_now(app, "用户点了「看一眼」，让你看一眼他正在看的窗口"),
            "dnd" => {
                let on = !st.settings().dnd;
                st.update_settings(|s| s.dnd = on);
                desktop::broadcast_settings_changed(app);
                if egg.is_none() {
                    crate::companion::say_reaction(
                        app,
                        if on {
                            "intent.dnd_on"
                        } else {
                            "intent.dnd_off"
                        },
                        &[],
                    );
                }
            }
            "voice" => {
                let on = crate::voice::toggle(app);
                crate::companion::say_reaction(
                    app,
                    if on {
                        "menu.voice_on"
                    } else {
                        "menu.voice_off"
                    },
                    &[],
                );
            }
            "hide" => {
                crate::companion::say_reaction(app, "menu.hide", &[]);
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    if let Some(p) = desktop::pet(&app)
                        && desktop::window_visible(&p)
                    {
                        let _ = desktop::toggle_pet_visible(&app);
                        desktop::refresh_tray_menu(&app);
                    }
                });
            }
            "settings" => desktop::open_settings(app)?,
            "more" => desktop::popup_pet_menu(app)?,
            "secret" => crate::companion::say_reaction(app, "menu.secret", &[]),
            _ => {}
        }
        Ok(())
    })();
    if let Some(scene) = egg {
        let label = items(false, true, Some(false), true)
            .into_iter()
            .find(|i| i.id == id)
            .map(|i| i.label)
            .unwrap_or_default();
        crate::companion::say_reaction(app, scene, &[("选项", label)]);
    }
    r.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_items() {
        let v = items(false, false, None, false);
        let ids: Vec<_> = v.iter().map(|i| i.id).collect();
        assert_eq!(
            ids,
            ["chat", "games", "remind", "dnd", "hide", "settings", "more"]
        );
        let v = items(true, true, Some(true), true);
        assert!(v.iter().any(|i| i.id == "look"));
        assert!(v.iter().any(|i| i.id == "secret"));
        assert!(v.iter().find(|i| i.id == "dnd").unwrap().on);
        assert!(v.iter().find(|i| i.id == "voice").unwrap().on);
        assert!(v.len() <= 11);
    }

    #[test]
    fn easter_eggs() {
        let now = 1000;
        let r = |ids: &[&str]| -> Vec<(i64, String)> {
            ids.iter().map(|i| (now - 5, (*i).to_string())).collect()
        };
        assert_eq!(
            pick_egg(&r(&["games", "games", "games"]), "games", now),
            Some("menu.repeat")
        );
        assert_eq!(pick_egg(&r(&["games", "games"]), "games", now), None);
        assert_eq!(
            pick_egg(&r(&["dnd", "dnd", "dnd", "dnd"]), "dnd", now),
            Some("menu.dnd_spam")
        );
        assert_eq!(pick_egg(&r(&["dnd", "dnd", "dnd"]), "dnd", now), None);
        // 很久以前的不算
        let old = vec![(now - 200, "games".to_string()); 3];
        assert_eq!(pick_egg(&old, "games", now), None);
    }
}
