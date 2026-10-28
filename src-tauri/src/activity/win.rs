//! Windows：读前台窗口的进程名、程序路径和标题；监听「前台窗口切换」系统事件（不轮询）。
//! 标题只交给 `classify` 在本地判断，不保存、不写日志。

use std::sync::OnceLock;

/// 前台窗口：（进程名，程序完整路径，窗口标题）
pub struct Foreground {
    pub exe: String,
    pub path: String,
    pub title: String,
    pub pid: u32,
}

static SIGNAL: OnceLock<tokio::sync::mpsc::UnboundedSender<()>> = OnceLock::new();

#[cfg(windows)]
pub fn foreground() -> Option<Foreground> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut title_buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
        let title = String::from_utf16_lossy(&title_buf[..n.max(0) as usize]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let exe = path.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        Some(Foreground {
            exe,
            path,
            title,
            pid,
        })
    }
}

#[cfg(not(windows))]
pub fn foreground() -> Option<Foreground> {
    None
}

#[cfg(windows)]
unsafe extern "system" fn on_foreground(
    _hook: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    _hwnd: windows_sys::Win32::Foundation::HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if let Some(tx) = SIGNAL.get() {
        let _ = tx.send(());
    }
}

/// 在主线程上注册前台窗口切换的系统事件（回调只发一个信号，真正的处理在后台）
pub fn install(app: &tauri::AppHandle) -> tokio::sync::mpsc::UnboundedReceiver<()> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = SIGNAL.set(tx);
    #[cfg(windows)]
    {
        let r = app.run_on_main_thread(|| {
            use windows_sys::Win32::UI::Accessibility::SetWinEventHook;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
            };
            let h = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    std::ptr::null_mut(),
                    Some(on_foreground),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            if h.is_null() {
                log::warn!("注册前台窗口事件失败，改为每 30 秒看一次");
            }
        });
        if let Err(e) = r {
            log::warn!("注册前台窗口事件失败：{e}");
        }
    }
    #[cfg(not(windows))]
    let _ = app;
    rx
}

/// 本机冒烟测试（默认忽略）：读一次前台窗口并分类，只打印分类和程序名，不打印窗口标题。
/// `cargo test --manifest-path src-tauri/Cargo.toml real_foreground_smoke -- --ignored --nocapture`
#[cfg(test)]
mod smoke {
    #[test]
    #[ignore]
    fn real_foreground_smoke() {
        let fg = super::foreground().expect("读不到前台窗口");
        let s = super::super::classify::classify(&fg.exe, &fg.path, &fg.title, &[]);
        println!(
            "进程 {}，分类 {:?}，程序名 {:?}，隐私 {}",
            s.exe, s.category, s.label, s.private
        );
        assert!(!s.exe.is_empty());
    }
}
