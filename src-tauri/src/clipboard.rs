//! 剪贴板读写封装（仅 Windows）：文本 / 图片 / 文件列表

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};

/// 串行化本应用内所有剪贴板访问（OpenClipboard 同时只能有一个持有者）
static CLIP_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn open_clipboard() -> bool {
    unsafe { OpenClipboard(None).is_ok() }
}

fn close_clipboard() {
    unsafe {
        let _ = CloseClipboard();
    }
}

/// 当前剪贴板文本（若存在 CF_UNICODETEXT）
pub fn read_text() -> Option<String> {
    let _guard = CLIP_MUTEX.lock().ok()?;
    if !open_clipboard() {
        return None;
    }
    let result = (|| {
        if !unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32) }.is_ok() {
            return None;
        }
        unsafe {
            let h = HGLOBAL(GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?.0);
            let ptr = GlobalLock(h) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let size = GlobalSize(h);
            let len = size / 2;
            let slice = std::slice::from_raw_parts(ptr, len as usize);
            let s = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(h);
            let trimmed = s.trim_end_matches('\0').to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
    })();
    close_clipboard();
    result
}

/// 当前剪贴板文件列表（若存在 CF_HDROP）
pub fn read_files() -> Option<Vec<String>> {
    let _guard = CLIP_MUTEX.lock().ok()?;
    if !open_clipboard() {
        return None;
    }
    let result = (|| {
        if !unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32) }.is_ok() {
            return None;
        }
        unsafe {
            let h = HGLOBAL(GetClipboardData(CF_HDROP.0 as u32).ok()?.0);
            let base = GlobalLock(h) as *const u8;
            if base.is_null() {
                return None;
            }
            // DROPFILES：pFiles(u32 偏移) + pt(8) + fNC(u32) + fWide(u32)
            let pfiles = *(base as *const u32) as usize;
            let fwide = *(base.add(16) as *const u32) != 0;
            let list_ptr = base.add(pfiles);
            let mut out = Vec::new();
            let mut cur = list_ptr;
            loop {
                let start = cur;
                let mut len = 0usize;
                if fwide {
                    while *((start as *const u16).add(len)) != 0 {
                        len += 1;
                    }
                } else {
                    while *start.add(len) != 0 {
                        len += 1;
                    }
                }
                if len == 0 {
                    break;
                }
                let s = if fwide {
                    String::from_utf16_lossy(std::slice::from_raw_parts(
                        start as *const u16,
                        len,
                    ))
                } else {
                    String::from_utf8_lossy(std::slice::from_raw_parts(start, len)).to_string()
                };
                out.push(s);
                cur = if fwide {
                    start.add((len + 1) * 2)
                } else {
                    start.add(len + 1)
                };
            }
            let _ = GlobalUnlock(h);
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
    })();
    close_clipboard();
    result
}

/// 当前剪贴板图片（若存在 CF_DIB / CF_DIBV5），返回 RGBA 像素 + 宽高
pub fn read_image_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let _guard = CLIP_MUTEX.lock().ok()?;
    if !open_clipboard() {
        return None;
    }
    let result = (|| {
        unsafe {
            let format = if IsClipboardFormatAvailable(CF_DIBV5.0 as u32).is_ok() {
                CF_DIBV5.0 as u32
            } else if IsClipboardFormatAvailable(CF_DIB.0 as u32).is_ok() {
                CF_DIB.0 as u32
            } else {
                return None;
            };
            let h = HGLOBAL(GetClipboardData(format).ok()?.0);
            let ptr = GlobalLock(h) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let size = GlobalSize(h);
            let result = decode_dib(ptr, size);
            let _ = GlobalUnlock(h);
            result
        }
    })();
    close_clipboard();
    result
}

/// 解析 DIB 内存块 → RGBA
/// 布局：BITMAPINFOHEADER(biSize 起) + 像素数据（默认自下而上）
unsafe fn decode_dib(ptr: *const u8, size: usize) -> Option<(Vec<u8>, u32, u32)> {
    let header_size = *(ptr as *const u32) as usize;
    if header_size < 40 || size <= header_size {
        return None;
    }
    let width = *(ptr.add(4) as *const i32);
    let height_raw = *(ptr.add(8) as *const i32);
    let bit_count = *(ptr.add(14) as *const u16);
    let compression = *(ptr.add(16) as *const u32);

    // 仅支持 BI_RGB(0)；BI_BITFIELDS(3) 的 32bpp 按常见 BGRA 掩码处理
    if compression != 0 && compression != 3 {
        log::debug!("unsupported DIB compression {compression}");
        return None;
    }
    if width <= 0 || height_raw == 0 {
        return None;
    }

    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs();
    let bpp = match bit_count {
        24 | 32 => bit_count,
        other => {
            log::debug!("unsupported DIB bit depth {other}");
            return None;
        }
    };

    let row_stride = ((width as usize * bpp as usize + 31) / 32) * 4;
    let pixels_off = header_size;
    if pixels_off + row_stride * height as usize > size {
        return None;
    }

    let mut rgba = Vec::with_capacity((width as usize * height as usize) * 4);
    let data = ptr.add(pixels_off);
    let bytes_per_px = (bpp / 8) as usize;

    for y in 0..height as usize {
        // 自下而上：内存第一行是图像最后一行
        let src_y = if top_down { y } else { (height as usize - 1) - y };
        let row = data.add(src_y * row_stride);
        for x in 0..width as usize {
            let px = row.add(x * bytes_per_px);
            let (r, g, b) = if bytes_per_px >= 4 {
                // BGRA → RGBA
                (*px.add(2), *px.add(1), *px.add(0))
            } else {
                // BGR → RGB
                (*px.add(2), *px.add(1), *px.add(0))
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Some((rgba, width as u32, height as u32))
}

/// 把 RGBA 编码为 PNG 字节
pub fn rgba_to_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or("invalid rgba buffer")?;
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// 写入文本到剪贴板（成功返回 true）
pub fn write_text(text: &str) -> bool {
    let _guard = match CLIP_MUTEX.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    if !open_clipboard() {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = wide.len() * 2;
        let h = match GlobalAlloc(GMEM_MOVEABLE, bytes) {
            Ok(h) => h,
            Err(_) => {
                close_clipboard();
                return false;
            }
        };
        let ptr = GlobalLock(h);
        if ptr.is_null() {
            let _ = GlobalFree(Some(h));
            close_clipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
        let _ = GlobalUnlock(h);
        // 交给系统管理；失败时释放
        if SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(h.0))).is_err() {
            let _ = GlobalFree(Some(h));
        }
    }
    close_clipboard();
    true
}

/// 当前剪贴板格式列表（调试用）
#[allow(dead_code)]
pub fn format_names() -> Vec<String> {
    let mut out = Vec::new();
    let _guard = CLIP_MUTEX.lock();
    if !open_clipboard() {
        return out;
    }
    unsafe {
        let mut cur: u32 = 0;
        loop {
            cur = EnumClipboardFormats(cur);
            if cur == 0 {
                break;
            }
            let mut buf = [0u16; 256];
            let n = windows::Win32::System::DataExchange::GetClipboardFormatNameW(cur, &mut buf);
            if n > 0 {
                out.push(String::from_utf16_lossy(&buf[..n as usize]));
            } else {
                out.push(format!("0x{cur:X}"));
            }
        }
    }
    close_clipboard();
    out
}
