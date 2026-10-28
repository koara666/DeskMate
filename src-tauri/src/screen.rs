//! 截屏（v2 P7 / v2.1 P9，DECISIONS #45、#47）：只在内存里处理，**不写硬盘、不进记忆**，发给视觉模型后立刻丢掉。
//! 只截要看的那一个窗口（前台窗口；前台是 DeskMate 自己时取它后面那个），不截整个屏幕、旁边的窗口和任务栏。

/// 发给模型的图的最长边：用户按快捷键时清楚一点，主动看时省一点
pub const MAX_SIDE_HOTKEY: u32 = 1280;
pub const MAX_SIDE_AUTO: u32 = 1024;
/// 文字识别用的图的最长边（太大识别慢，太小认不出小字）
pub const MAX_SIDE_OCR: u32 = 2048;
pub const JPEG_QUALITY: u8 = 72;

/// 一张截图（BGRA，每像素 4 字节，从上到下）
pub struct Shot {
    pub bgra: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// 要看的窗口
#[derive(Debug, Clone)]
pub struct Target {
    pub hwnd: isize,
    pub exe: String,
    pub path: String,
    pub title: String,
}

/// 缩放（2×2 平均，足够看清画面大意）。`out_bgra`：输出 BGRA（给文字识别）还是 RGB（给编码）
pub fn downscale(src: &[u8], w: u32, h: u32, max_side: u32, out_bgra: bool) -> (Vec<u8>, u32, u32) {
    let scale = (w.max(h) as f64 / max_side as f64).max(1.0);
    let nw = ((w as f64 / scale).round() as u32).max(1);
    let nh = ((h as f64 / scale).round() as u32).max(1);
    let ch = if out_bgra { 4 } else { 3 };
    let mut out = Vec::with_capacity((nw * nh * ch) as usize);
    let px = |x: u32, y: u32| -> [u32; 3] {
        let i = ((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize;
        [src[i] as u32, src[i + 1] as u32, src[i + 2] as u32]
    };
    for y in 0..nh {
        for x in 0..nw {
            let sx = (x as f64 * scale) as u32;
            let sy = (y as f64 * scale) as u32;
            let (a, b, c, d) = (
                px(sx, sy),
                px(sx + 1, sy),
                px(sx, sy + 1),
                px(sx + 1, sy + 1),
            );
            let avg = |k: usize| ((a[k] + b[k] + c[k] + d[k]) / 4) as u8;
            if out_bgra {
                out.extend_from_slice(&[avg(0), avg(1), avg(2), 255]);
            } else {
                // BGRA → RGB
                out.extend_from_slice(&[avg(2), avg(1), avg(0)]);
            }
        }
    }
    (out, nw, nh)
}

/// 在 RGB 图上涂黑一块（坐标会被裁到图内）
pub fn fill_rgb(rgb: &mut [u8], w: u32, h: u32, x: f32, y: f32, rw: f32, rh: f32) {
    let x0 = x.floor().max(0.0) as u32;
    let y0 = y.floor().max(0.0) as u32;
    let x1 = ((x + rw).ceil().max(0.0) as u32).min(w);
    let y1 = ((y + rh).ceil().max(0.0) as u32).min(h);
    for yy in y0..y1 {
        for xx in x0..x1 {
            let i = ((yy * w + xx) * 3) as usize;
            rgb[i..i + 3].copy_from_slice(&[24, 24, 28]);
        }
    }
}

pub fn encode_jpeg(rgb: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut buf, JPEG_QUALITY);
    enc.encode(
        rgb,
        u16::try_from(w).map_err(|_| "图太大")?,
        u16::try_from(h).map_err(|_| "图太大")?,
        jpeg_encoder::ColorType::Rgb,
    )
    .map_err(|e| e.to_string())?;
    Ok(buf)
}

#[cfg(windows)]
fn window_info(
    hwnd: windows_sys::Win32::Foundation::HWND,
) -> Option<(u32, String, String, String)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextW, GetWindowThreadProcessId};
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let mut tb = [0u16; 512];
        let n = GetWindowTextW(hwnd, tb.as_mut_ptr(), tb.len() as i32);
        let title = String::from_utf16_lossy(&tb[..n.max(0) as usize]);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return Some((pid, String::new(), String::new(), title));
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        let path = if ok != 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        };
        let exe = path.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        Some((pid, exe, path, title))
    }
}

/// 窗口能不能当「要看的窗口」：可见、没最小化、没被系统藏起来、够大、不是桌面和任务栏
#[cfg(windows)]
fn usable(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetClassNameW, GetWindowLongPtrW, IsIconic, IsWindowVisible, WS_EX_TOOLWINDOW,
    };
    unsafe {
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return false;
        }
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, &mut cloaked as *mut u32 as _, 4);
        if cloaked != 0 {
            return false;
        }
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0 {
            return false;
        }
        let mut cb = [0u16; 64];
        let n = GetClassNameW(hwnd, cb.as_mut_ptr(), cb.len() as i32);
        let class = String::from_utf16_lossy(&cb[..n.max(0) as usize]);
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return false;
        }
        window_rect(hwnd).is_some_and(|(_, _, w, h)| w >= 120 && h >= 90)
    }
}

/// 窗口在屏幕上的位置（不含看不见的阴影边框），裁到它所在的显示器以内
#[cfg(windows)]
fn window_rect(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<(i32, i32, u32, u32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut r as *mut RECT as _,
            std::mem::size_of::<RECT>() as u32,
        ) != 0
            && GetWindowRect(hwnd, &mut r) == 0
        {
            return None;
        }
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return None;
        }
        let m = mi.rcMonitor;
        let (x0, y0) = (r.left.max(m.left), r.top.max(m.top));
        let (x1, y1) = (r.right.min(m.right), r.bottom.min(m.bottom));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some((x0, y0, (x1 - x0) as u32, (y1 - y0) as u32))
    }
}

/// 要看的窗口：前台窗口；前台是 DeskMate 自己（比如正开着设置）时，取 Z 顺序里它后面第一个能看的窗口
#[cfg(windows)]
pub fn target_window() -> Option<Target> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_HWNDNEXT, GetForegroundWindow, GetTopWindow, GetWindow,
    };
    let me = std::process::id();
    unsafe {
        let fg = GetForegroundWindow();
        let pick = |h| {
            let (pid, exe, path, title) = window_info(h)?;
            (pid != me && usable(h)).then_some(Target {
                hwnd: h as isize,
                exe,
                path,
                title,
            })
        };
        if !fg.is_null()
            && let Some(t) = pick(fg)
        {
            return Some(t);
        }
        // 前台是自己（或者是桌面）：按 Z 顺序往下找
        if !fg.is_null() && window_info(fg).is_some_and(|(pid, ..)| pid != me) {
            return None;
        }
        let mut h = GetTopWindow(std::ptr::null_mut());
        let mut n = 0;
        while !h.is_null() && n < 400 {
            if let Some(t) = pick(h) {
                return Some(t);
            }
            h = GetWindow(h, GW_HWNDNEXT);
            n += 1;
        }
        None
    }
}

/// Z 顺序里第一个能看的别的程序的窗口（本机冒烟测试用：前台是桌面时也能试一次）
#[cfg(windows)]
pub fn first_usable() -> Option<Target> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GW_HWNDNEXT, GetTopWindow, GetWindow};
    let me = std::process::id();
    unsafe {
        let mut h = GetTopWindow(std::ptr::null_mut());
        let mut n = 0;
        while !h.is_null() && n < 400 {
            if let Some((pid, exe, path, title)) = window_info(h)
                && pid != me
                && usable(h)
            {
                return Some(Target {
                    hwnd: h as isize,
                    exe,
                    path,
                    title,
                });
            }
            h = GetWindow(h, GW_HWNDNEXT);
            n += 1;
        }
        None
    }
}

#[cfg(not(windows))]
pub fn first_usable() -> Option<Target> {
    None
}

#[cfg(not(windows))]
pub fn target_window() -> Option<Target> {
    None
}

/// 截这个窗口在屏幕上的那一块
#[cfg(windows)]
pub fn capture(t: &Target) -> Result<Shot, String> {
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
        CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
        SRCCOPY, SelectObject,
    };
    let (x, y, w, h) = window_rect(t.hwnd as _).ok_or("找不到窗口的位置")?;
    if w > 16384 || h > 16384 {
        return Err("窗口大小不对".into());
    }
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w as i32, h as i32);
        let old = SelectObject(mem, bmp);
        let ok = BitBlt(
            mem,
            0,
            0,
            w as i32,
            h as i32,
            screen,
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        );
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            // 负数 = 从上到下
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            mem,
            bmp,
            0,
            h,
            buf.as_mut_ptr() as _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
        if ok == 0 || lines == 0 {
            return Err("截屏失败".into());
        }
        Ok(Shot { bgra: buf, w, h })
    }
}

#[cfg(not(windows))]
pub fn capture(_t: &Target) -> Result<Shot, String> {
    Err("这个系统上还不支持".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机冒烟（默认忽略）：截一次要看的窗口，只打印程序名、大小和用时，不保存、不发送、不打印标题
    #[test]
    #[ignore]
    fn real_capture_smoke() {
        let t0 = std::time::Instant::now();
        let t = target_window().or_else(first_usable).expect("没找到窗口");
        let shot = capture(&t).expect("截屏失败");
        let (rgb, w, h) = downscale(&shot.bgra, shot.w, shot.h, MAX_SIDE_HOTKEY, false);
        let jpg = encode_jpeg(&rgb, w, h).unwrap();
        println!(
            "{}：{}×{} → {}×{}，JPEG {} KB，{:?}",
            t.exe,
            shot.w,
            shot.h,
            w,
            h,
            jpg.len() / 1024,
            t0.elapsed()
        );
        assert!(jpg.starts_with(&[0xFF, 0xD8]));
    }

    fn red(w: u32, h: u32) -> Vec<u8> {
        let mut src = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            src.extend_from_slice(&[0, 0, 255, 255]);
        }
        src
    }

    #[test]
    fn downscale_mask_and_encode() {
        let (w, h) = (2000u32, 1000u32);
        let src = red(w, h);
        let (mut rgb, nw, nh) = downscale(&src, w, h, 1024, false);
        assert_eq!((nw, nh), (1024, 512));
        assert_eq!(&rgb[..3], &[255, 0, 0]);
        fill_rgb(&mut rgb, nw, nh, -5.0, -5.0, 10.0, 10.0);
        assert_eq!(&rgb[..3], &[24, 24, 28], "涂黑");
        let i = ((6 * nw + 6) * 3) as usize;
        assert_eq!(&rgb[i..i + 3], &[255, 0, 0], "范围外不动");
        fill_rgb(&mut rgb, nw, nh, 2000.0, 2000.0, 10.0, 10.0);
        let jpg = encode_jpeg(&rgb, nw, nh).unwrap();
        assert!(jpg.starts_with(&[0xFF, 0xD8]));
        // BGRA 输出给文字识别
        let (bgra, _, _) = downscale(&src, w, h, 512, true);
        assert_eq!(&bgra[..4], &[0, 0, 255, 255]);
        // 小图不放大
        let (_, w2, h2) = downscale(&src[..(100 * 50 * 4) as usize], 100, 50, 1024, false);
        assert_eq!((w2, h2), (100, 50));
    }
}
