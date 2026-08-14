//! 剪贴板监听：事件驱动（AddClipboardFormatListener）+ 轮询兜底

use crate::clipboard;
use crate::db::{now_ms, DbError};
use crate::dedup;
use crate::models::{Item, ItemDto, ItemKind};
use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM, HMODULE};
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassExW,
    TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
    WM_CLIPBOARDUPDATE,
};
use windows::core::{PCWSTR, PWSTR};

/// 监听器线程持有的应用句柄
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// 轮询间隔（毫秒）
const POLL_INTERVAL_MS: u64 = 500;
/// 自身写入守卫窗口（毫秒）
const SELF_WRITE_GUARD_MS: i64 = 300;
/// 缩略图最长边
const THUMB_MAX_SIZE: u32 = 256;

pub fn start(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
    std::thread::spawn(listener_thread);
    std::thread::spawn(poll_thread);
}

// ---------- 事件驱动监听（消息窗口 + WM_CLIPBOARDUPDATE） ----------

unsafe extern "system" fn listener_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        process_clipboard_change();
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn listener_thread() {
    unsafe {
        let class_name = widestr("PasteBoardClipboardListener");
        let hmodule: HMODULE = GetModuleHandleW(None).expect("GetModuleHandleW failed");
        let hinstance = HINSTANCE(hmodule.0);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(listener_proc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            log::error!("RegisterClassExW failed, err={}", std::io::Error::last_os_error());
            return;
        }
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                log::error!("CreateWindowExW failed: {e}");
                return;
            }
        };
        if AddClipboardFormatListener(hwnd).is_err() {
            log::error!("AddClipboardFormatListener failed");
            return;
        }
        log::info!("clipboard listener ready, hwnd={:?}", hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

// ---------- 轮询兜底 ----------

fn poll_thread() {
    let mut last_signature: Option<String> = None;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        let signature = clipboard_signature();
        if signature.is_some() && signature != last_signature {
            process_clipboard_change();
        }
        last_signature = signature;
    }
}

/// 剪贴板内容签名：格式名列表 + 文本内容（轮询用）
fn clipboard_signature() -> Option<String> {
    let mut parts = clipboard::format_names();
    parts.sort();
    let mut sig = parts.join("|");
    if let Some(text) = clipboard::read_text() {
        sig.push_str("::");
        sig.push_str(&dedup::hash_text(&text));
    }
    if sig.is_empty() {
        None
    } else {
        Some(sig)
    }
}

// ---------- 保存流水线（事件与轮询共用） ----------

fn process_clipboard_change() {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let state = app.state::<AppState>();

    // 自身写入守卫：我们写入剪贴板后 300ms 内的变化跳过
    if state.self_write.swap(false, Ordering::SeqCst) {
        if now_ms() - state.last_self_write_ms.load(Ordering::SeqCst) < SELF_WRITE_GUARD_MS {
            log::debug!("skip self write");
            return;
        }
    }

    match save_from_clipboard(&state, app) {
        Ok(Some((item, changed))) => {
            if let Some(dto) = item_dto(&state, &item) {
                let payload = serde_json::json!({ "id": item.id, "kind": item.kind.to_string(), "changed": changed, "item": dto });
                let _ = app.emit("clipboard://changed", payload);
            }
        }
        Ok(None) => {}
        Err(e) => log::warn!("failed to save clipboard: {e}"),
    }
}

/// 读取剪贴板并入库；返回 (条目, 是否新增)。重复内容仅刷新时间（changed=false）
fn save_from_clipboard(
    state: &AppState,
    app: &AppHandle,
) -> Result<Option<(Item, bool)>, DbError> {
    // 1. 类型判定：文件 > 图片 > 文本
    let (kind, content, file_paths, image_data, hash) = if let Some(files) = clipboard::read_files()
    {
        let json = serde_json::to_string(&files).unwrap_or_else(|_| "[]".into());
        (
            ItemKind::Files,
            None,
            Some(json),
            None,
            dedup::hash_files(&files),
        )
    } else if let Some((rgba, w, h)) = clipboard::read_image_rgba() {
        let Some(hash) = dedup::hash_image_rgba(&rgba, w, h) else {
            return Ok(None);
        };
        (ItemKind::Image, None, None, Some((rgba, w, h)), hash)
    } else if let Some(text) = clipboard::read_text() {
        if text.trim().is_empty() {
            return Ok(None);
        }
        let text_hash = dedup::hash_text(&text);
        (ItemKind::Text, Some(text), None, None, text_hash)
    } else {
        return Ok(None);
    };

    let db = state.db.lock().unwrap();
    let now = now_ms();

    // 2. 去重：命中则顶到最前
    if let Some(existing_id) = db.find_by_hash(&hash)? {
        db.touch_item(existing_id, now)?;
        let item = db.get_item(existing_id)?.ok_or_else(|| {
            DbError::Sql(rusqlite::Error::QueryReturnedNoRows)
        })?;
        log::debug!("dedup: touch item {}", existing_id);
        return Ok(Some((item, false)));
    }

    // 3. 新增
    let mut item = Item {
        id: 0,
        kind: kind.clone(),
        content,
        file_paths,
        image_path: None,
        thumb_path: None,
        hash,
        pinned: false,
        created_at: now,
    };
    let Some(id) = db.insert_item(&item)? else {
        return Ok(None);
    };
    item.id = id;
    log::info!("saved clipboard item id={id} kind={}", item.kind);

    // 4. 图片落盘（原图 + 缩略图）
    if let Some((rgba, w, h)) = image_data {
        match clipboard::rgba_to_png(&rgba, w, h) {
            Ok(png) => {
                if let (Ok(img_path), Ok(thumb_png)) = (
                    state.store.save_image(id, &png),
                    crate::store::FileStore::make_thumb_png(&rgba, w, h, THUMB_MAX_SIZE)
                        .map(|tp| state.store.save_thumb(id, &tp))
                        .unwrap_or(Ok(std::path::PathBuf::new())),
                ) {
                    let _ = db.set_image_paths(
                        id,
                        Some(img_path.to_string_lossy().into_owned()),
                        Some(thumb_png.to_string_lossy().into_owned()),
                    );
                    item.image_path = Some(img_path.to_string_lossy().into_owned());
                    item.thumb_path = Some(thumb_png.to_string_lossy().into_owned());
                } else {
                    log::warn!("failed to save image files for item {id}");
                }
            }
            Err(e) => log::warn!("failed to encode png: {e}"),
        }
    }

    // 5. 上限清理
    let removed = db.prune(db.max_items())?;
    if !removed.is_empty() {
        let ids: Vec<i64> = removed.iter().map(|r| r.id).collect();
        let _ = app.emit("clipboard://pruned", serde_json::json!(ids));
        for r in removed {
            state.store.remove_files(&r);
        }
    }

    Ok(Some((item, true)))
}

/// 组装前端视图；图片缩略图读取后转 base64
fn item_dto(_state: &AppState, item: &Item) -> Option<ItemDto> {
    let thumb = if item.kind == ItemKind::Image {
        item.thumb_path.as_ref().and_then(|p| {
            std::fs::read(p)
                .ok()
                .map(|b| base64_encode(&b))
        })
    } else {
        None
    };
    Some(item.to_dto(thumb))
}

pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        out.push(TABLE[(b[0] >> 2) as usize] as char);
        out.push(TABLE[((b[0] & 0x03) << 4 | b[1] >> 4) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((b[1] & 0x0F) << 2 | b[2] >> 6) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(b[2] & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn widestr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
