//! 截屏（v2 P7，DECISIONS #45）：只在内存里处理，**不写硬盘、不进记忆**，发给视觉模型后立刻丢掉。
//! 截鼠标所在的那块屏幕，缩到最长边 1024 像素以内，编码成 PNG。

/// 缩小后的最长边
pub const MAX_SIDE: u32 = 1024;

/// BGRA（每像素 4 字节，从上到下）缩放成 RGB（最近邻 + 2×2 平均，足够看清画面大意）
pub fn downscale_bgra(src: &[u8], w: u32, h: u32, max_side: u32) -> (Vec<u8>, u32, u32) {
    let scale = (w.max(h) as f64 / max_side as f64).max(1.0);
    let nw = ((w as f64 / scale).round() as u32).max(1);
    let nh = ((h as f64 / scale).round() as u32).max(1);
    let mut out = Vec::with_capacity((nw * nh * 3) as usize);
    let px = |x: u32, y: u32| -> [u32; 3] {
        let i = ((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize;
        [src[i + 2] as u32, src[i + 1] as u32, src[i] as u32]
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
            for k in 0..3 {
                out.push(((a[k] + b[k] + c[k] + d[k]) / 4) as u8);
            }
        }
    }
    (out, nw, nh)
}

pub fn encode_png(rgb: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(rgb).map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

/// 截鼠标所在的屏幕，返回 PNG
#[cfg(windows)]
pub fn capture_png() -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
        CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits,
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC,
        SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return Err("找不到屏幕".into());
        }
        let r = mi.rcMonitor;
        let (w, h) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
        if w == 0 || h == 0 || w > 16384 || h > 16384 {
            return Err("屏幕大小不对".into());
        }
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
            r.left,
            r.top,
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
        let (rgb, nw, nh) = downscale_bgra(&buf, w, h, MAX_SIDE);
        drop(buf);
        encode_png(&rgb, nw, nh)
    }
}

#[cfg(not(windows))]
pub fn capture_png() -> Result<Vec<u8>, String> {
    Err("这个系统上还不支持".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机冒烟（默认忽略）：截一次屏，只检查格式和大小，不保存、不发送
    #[test]
    #[ignore]
    fn real_capture_smoke() {
        let png = capture_png().expect("截屏失败");
        assert!(png.starts_with(b"\x89PNG"));
        println!("PNG {} KB", png.len() / 1024);
        assert!(png.len() < 3 * 1024 * 1024);
    }

    #[test]
    fn downscale_and_encode() {
        // 2000×1000 的纯红图（BGRA）
        let (w, h) = (2000u32, 1000u32);
        let mut src = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            src.extend_from_slice(&[0, 0, 255, 255]);
        }
        let (rgb, nw, nh) = downscale_bgra(&src, w, h, 1024);
        assert_eq!((nw, nh), (1024, 512));
        assert_eq!(&rgb[..3], &[255, 0, 0]);
        let png = encode_png(&rgb, nw, nh).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        // 小图不放大
        let (_, w2, h2) = downscale_bgra(&src[..(100 * 50 * 4) as usize], 100, 50, 1024);
        assert_eq!((w2, h2), (100, 50));
    }
}
