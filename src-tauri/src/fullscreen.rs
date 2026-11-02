//! 「有全屏程序在前台」的判断：玩游戏、看全屏视频、演示时桌宠要躲开。
//!
//! 只看系统的 `SHQueryUserNotificationState` 太爱误报：它的「BUSY」对任何铺满屏幕的窗口都成立
//! （截图工具的遮罩、Alt+Tab、任务视图、无标题栏的最大化窗口……），桌宠就会在用户点了什么之后凭空消失。
//! 所以分两种情况：
//! - DirectX 独占全屏、演示模式：系统不会乱报，直接信；
//! - 「BUSY」：再核对前台窗口——不是 DeskMate 自己、不是系统外壳（资源管理器、开始菜单、截图遮罩……）、
//!   没有标题栏、真的盖住了整个显示器。
//!
//! 再加上防抖（`Debounce`）：连续几次都是才算进入全屏，连续几次都不是才算退出。

/// 屏幕上的矩形（和 Win32 的 `RECT` 一样：右 / 下边界不含）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    /// 自己是否完整盖住 `other`
    fn covers(&self, other: &Rect) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }
}

/// 前台窗口的事实（采集和判断分开，判断才好测）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFacts {
    /// 进程名，读不到（权限不够）时为空
    pub exe: String,
    pub class: String,
    /// 是 DeskMate 自己的窗口
    pub own: bool,
    /// 有标题栏（最大化的普通窗口有，全屏的游戏 / 视频没有）
    pub has_caption: bool,
    pub rect: Rect,
    /// 窗口所在显示器的完整范围（含任务栏那一块）
    pub monitor: Rect,
}

/// 系统的全屏状态（`QUERY_USER_NOTIFICATION_STATE` 归并后的样子）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notify {
    Other,
    /// QUNS_BUSY：全屏程序，也可能只是铺满屏幕的窗口
    Busy,
    /// QUNS_RUNNING_D3D_FULL_SCREEN：DirectX 独占全屏
    D3d,
    /// QUNS_PRESENTATION_MODE：演示模式
    Presentation,
}

impl Notify {
    pub fn from_raw(state: i32) -> Self {
        match state {
            2 => Notify::Busy,
            3 => Notify::D3d,
            4 => Notify::Presentation,
            _ => Notify::Other,
        }
    }
}

/// 判定为全屏时，写进日志的信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub exe: String,
    pub reason: &'static str,
}

/// 这些进程的窗口不算「全屏程序」：桌面、任务栏、Alt+Tab、任务视图、开始菜单、搜索、截图遮罩、锁屏
const SHELL_EXES: [&str; 9] = [
    "explorer.exe",
    "searchhost.exe",
    "searchapp.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "screenclippinghost.exe",
    "snippingtool.exe",
    "textinputhost.exe",
    "lockapp.exe",
];
/// 进程名读不到时，靠窗口类名兜底
const SHELL_CLASSES: [&str; 4] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
];

/// 这个前台窗口是不是真的「全屏」：不是自己、不是系统外壳、没有标题栏、盖住了整个显示器
pub fn is_fullscreen_window(f: &WindowFacts) -> bool {
    !f.own
        && !SHELL_EXES.contains(&f.exe.to_ascii_lowercase().as_str())
        && !SHELL_CLASSES.contains(&f.class.as_str())
        && !f.has_caption
        && f.rect.covers(&f.monitor)
}

/// 综合系统状态和前台窗口：有全屏程序就返回是谁、凭什么
pub fn decide(n: Notify, fg: Option<&WindowFacts>) -> Option<Found> {
    let exe = fg.map(|f| f.exe.clone()).unwrap_or_default();
    match n {
        Notify::D3d => Some(Found {
            exe,
            reason: "DirectX 独占全屏",
        }),
        Notify::Presentation => Some(Found {
            exe,
            reason: "演示模式",
        }),
        Notify::Busy => fg.filter(|f| is_fullscreen_window(f)).map(|_| Found {
            exe,
            reason: "铺满屏幕的窗口",
        }),
        Notify::Other => None,
    }
}

/// 防抖：连续 `ENTER` 次都是才进入全屏，连续 `LEAVE` 次都不是才退出。
/// 截图遮罩、Alt+Tab 这种一闪而过的不会让桌宠躲起来，游戏里弹个通知也不会让她跳出来。
#[derive(Debug, Default)]
pub struct Debounce {
    active: bool,
    streak: u8,
}

impl Debounce {
    pub const ENTER: u8 = 3;
    pub const LEAVE: u8 = 2;

    /// 喂一次检测结果，返回现在算不算全屏
    pub fn push(&mut self, found: bool) -> bool {
        if found == self.active {
            self.streak = 0;
        } else {
            self.streak += 1;
            let need = if found { Self::ENTER } else { Self::LEAVE };
            if self.streak >= need {
                self.active = found;
                self.streak = 0;
            }
        }
        self.active
    }
}

#[cfg(windows)]
fn notify_state() -> Notify {
    use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
    let mut state = 0;
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    if hr >= 0 {
        Notify::from_raw(state)
    } else {
        Notify::Other
    }
}

#[cfg(windows)]
fn foreground_facts() -> Option<WindowFacts> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, WS_CAPTION,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let (pid, exe, _, _) = crate::screen::window_info(hwnd)?;
        let mut cb = [0u16; 64];
        let n = GetClassNameW(hwnd, cb.as_mut_ptr(), cb.len() as i32);
        let class = String::from_utf16_lossy(&cb[..n.max(0) as usize]);
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 {
            return None;
        }
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi) == 0 {
            return None;
        }
        let m = mi.rcMonitor;
        Some(WindowFacts {
            exe,
            class,
            own: pid == std::process::id(),
            has_caption: GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION,
            rect: Rect {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            },
            monitor: Rect {
                left: m.left,
                top: m.top,
                right: m.right,
                bottom: m.bottom,
            },
        })
    }
}

/// 现在有没有全屏程序在前台（单次检测，没有防抖）
#[cfg(windows)]
pub fn detect() -> Option<Found> {
    let n = notify_state();
    if n == Notify::Other {
        return None;
    }
    decide(n, foreground_facts().as_ref())
}

#[cfg(not(windows))]
pub fn detect() -> Option<Found> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    fn win(exe: &str, rect: Rect, has_caption: bool) -> WindowFacts {
        WindowFacts {
            exe: exe.into(),
            class: "SomeClass".into(),
            own: false,
            has_caption,
            rect,
            monitor: MON,
        }
    }

    #[test]
    fn exclusive_fullscreen_and_presentation_are_trusted() {
        let f = decide(Notify::D3d, None).expect("独占全屏直接信");
        assert_eq!(f.reason, "DirectX 独占全屏");
        assert!(decide(Notify::Presentation, None).is_some());
        assert!(decide(Notify::Other, Some(&win("game.exe", MON, false))).is_none());
    }

    #[test]
    fn busy_needs_a_window_that_really_covers_the_monitor() {
        // 无边框铺满：游戏 / 全屏视频
        let borderless = win("game.exe", MON, false);
        let f = decide(Notify::Busy, Some(&borderless)).expect("铺满且没标题栏");
        assert_eq!(f.exe, "game.exe");
        // 比显示器还大一点（独占全屏有时会）也算
        let bigger = win(
            "game.exe",
            Rect {
                left: -1,
                top: -1,
                right: 1921,
                bottom: 1081,
            },
            false,
        );
        assert!(decide(Notify::Busy, Some(&bigger)).is_some());
        // 最大化的普通窗口：有标题栏
        assert!(decide(Notify::Busy, Some(&win("chrome.exe", MON, true))).is_none());
        // 没盖住任务栏那一块
        let work_area = Rect {
            bottom: 1040,
            ..MON
        };
        assert!(decide(Notify::Busy, Some(&win("game.exe", work_area, false))).is_none());
        // 读不到前台窗口
        assert!(decide(Notify::Busy, None).is_none());
    }

    #[test]
    fn shell_and_own_windows_never_count() {
        // Alt+Tab、任务视图、桌面都属于资源管理器
        assert!(decide(Notify::Busy, Some(&win("explorer.exe", MON, false))).is_none());
        assert!(decide(Notify::Busy, Some(&win("Explorer.EXE", MON, false))).is_none());
        assert!(
            decide(
                Notify::Busy,
                Some(&win("ScreenClippingHost.exe", MON, false))
            )
            .is_none()
        );
        // 进程名读不到：靠类名
        let mut desktop = win("", MON, false);
        desktop.class = "WorkerW".into();
        assert!(decide(Notify::Busy, Some(&desktop)).is_none());
        // DeskMate 自己的窗口
        let mut own = win("deskmate.exe", MON, false);
        own.own = true;
        assert!(decide(Notify::Busy, Some(&own)).is_none());
        // 进程名读不到（带反作弊的游戏）但几何上是全屏：照样算
        assert!(decide(Notify::Busy, Some(&win("", MON, false))).is_some());
    }

    #[test]
    fn debounce_ignores_blips_in_both_directions() {
        let mut d = Debounce::default();
        // 截图遮罩一闪而过：连续两次不够
        assert!(!d.push(true));
        assert!(!d.push(true));
        assert!(!d.push(false));
        // 连续三次才算进入
        assert!(!d.push(true));
        assert!(!d.push(true));
        assert!(d.push(true));
        // 游戏里弹个通知（一次不是全屏）不会跳出来
        assert!(d.push(false));
        assert!(d.push(true));
        // 连续两次不是才算退出
        assert!(d.push(false));
        assert!(!d.push(false));
        assert!(!d.push(false));
    }

    /// 本机冒烟测试（默认忽略）：打印系统现在的全屏状态和前台窗口的事实，只打印进程名和类名，不打印标题。
    /// `cargo test --manifest-path src-tauri/Cargo.toml real_fullscreen_smoke -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_fullscreen_smoke() {
        let n = notify_state();
        let f = foreground_facts();
        println!("state {n:?}; foreground {f:?}");
        println!("verdict {:?}", decide(n, f.as_ref()));
    }
}
