//! Tauri 命令层：前端 invoke 的入口，仅做编排

use crate::models::{Item, ItemDto};
use crate::state::AppState;
use serde::Serialize;
use tauri::{Manager, State};

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub message: String,
}

impl From<rusqlite::Error> for CommandError {
    fn from(e: rusqlite::Error) -> Self {
        CommandError { message: e.to_string() }
    }
}

impl From<crate::db::DbError> for CommandError {
    fn from(e: crate::db::DbError) -> Self {
        CommandError { message: e.to_string() }
    }
}

type CmdResult<T> = Result<T, CommandError>;

/// 历史列表（可搜索、分页）
#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    filter: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> CmdResult<Vec<ItemDto>> {
    let db = state.db.lock().unwrap();
    let items = db.list_items(
        filter.as_deref().unwrap_or(""),
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )?;
    Ok(items.iter().map(|i| to_dto(&state, i)).collect())
}

/// 固定 / 取消固定
#[tauri::command]
pub fn pin_item(state: State<'_, AppState>, id: i64, pinned: bool) -> CmdResult<bool> {
    let db = state.db.lock().unwrap();
    Ok(db.set_pinned(id, pinned)?)
}

/// 删除单条（含磁盘文件）
#[tauri::command]
pub fn delete_item(state: State<'_, AppState>, id: i64) -> CmdResult<bool> {
    let db = state.db.lock().unwrap();
    if let Some(item) = db.delete_item(id)? {
        state.store.remove_files(&item);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 清空全部非固定条目
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> CmdResult<u32> {
    let db = state.db.lock().unwrap();
    let removed = db.clear_unpinned()?;
    let n = removed.len() as u32;
    for item in removed {
        state.store.remove_files(&item);
    }
    Ok(n)
}

/// 粘贴条目到上一窗口（核心动作）
#[tauri::command]
pub fn paste_item(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    crate::paste::paste_item(&state, id).map_err(|m| CommandError { message: m })
}

#[derive(Debug, Serialize)]
pub struct SettingsDto {
    pub max_items: i64,
    pub theme: String,
    /// "on" / "off"：唤起是否跟随鼠标
    pub follow_mouse: String,
    /// "on" / "off"：粘贴后是否保持窗口打开
    pub keep_open: String,
    /// "on" / "off"：窗口置顶
    pub always_on_top: String,
    /// 窗口尺寸（逻辑像素，0 表示未保存）
    pub win_w: f64,
    pub win_h: f64,
    /// 开机自启状态（来自系统，非持久化键）
    pub autostart: bool,
}

fn get_setting_str(db: &crate::db::Db, key: &str, default: &str) -> String {
    db.get_setting(key).ok().flatten().unwrap_or_else(|| default.into())
}

fn set_setting_str(db: &crate::db::Db, key: &str, value: &str) -> Result<(), crate::db::DbError> {
    db.set_setting(key, value)
}

/// 读设置
#[tauri::command]
pub fn get_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<SettingsDto> {
    use tauri_plugin_autostart::ManagerExt;
    let db = state.db.lock().unwrap();
    Ok(SettingsDto {
        max_items: db.max_items(),
        theme: get_setting_str(&db, "theme", "dark"),
        follow_mouse: get_setting_str(&db, "follow_mouse", "on"),
        keep_open: get_setting_str(&db, "keep_open", "off"),
        always_on_top: get_setting_str(&db, "always_on_top", "off"),
        win_w: db
            .get_setting("win_w")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
        win_h: db
            .get_setting("win_h")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
    })
}

/// 写设置
#[tauri::command]
pub fn set_max_items(state: State<'_, AppState>, max_items: i64) -> CmdResult<()> {
    let db = state.db.lock().unwrap();
    db.set_setting("max_items", &max_items.to_string())?;
    Ok(())
}

/// 设置主题（dark / light / system）
#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: String) -> CmdResult<()> {
    if theme != "dark" && theme != "light" && theme != "system" {
        return Err(CommandError {
            message: format!("无效主题: {theme}"),
        });
    }
    let db = state.db.lock().unwrap();
    db.set_setting("theme", &theme)?;
    Ok(())
}

/// 设置开关类选项（follow_mouse / keep_open / always_on_top）
#[tauri::command]
pub fn set_toggle(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> CmdResult<()> {
    if !matches!(key.as_str(), "follow_mouse" | "keep_open" | "always_on_top") {
        return Err(CommandError {
            message: format!("未知设置项: {key}"),
        });
    }
    if value != "on" && value != "off" {
        return Err(CommandError {
            message: "值必须为 on/off".into(),
        });
    }
    // 置顶即时生效
    if key == "always_on_top" {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_always_on_top(value == "on");
        }
    }
    let db = state.db.lock().unwrap();
    set_setting_str(&db, &key, &value)?;
    log::info!("setting {key} -> {value}");
    Ok(())
}

/// 设置开机自启（与托盘勾选联动）
#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> CmdResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let result = if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    };
    result.map_err(|e| CommandError {
        message: format!("设置开机自启失败: {e}"),
    })?;
    // 同步托盘勾选状态
    if let Some(item) = crate::AUTOSTART_ITEM.get() {
        let _ = item.set_checked(enabled);
    }
    log::info!("autostart set to {enabled} (from settings)");
    Ok(())
}

/// 保存窗口尺寸（用户调整后记忆）
#[tauri::command]
pub fn set_window_size(state: State<'_, AppState>, w: f64, h: f64) -> CmdResult<()> {
    if !(w > 0.0) || !(h > 0.0) {
        return Err(CommandError {
            message: "无效尺寸".into(),
        });
    }
    let db = state.db.lock().unwrap();
    db.set_setting("win_w", &w.to_string())?;
    db.set_setting("win_h", &h.to_string())?;
    Ok(())
}

/// 设置全局热键（立即生效并持久化）
#[tauri::command]
pub fn set_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    hotkey: String,
) -> CmdResult<()> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut()
        .register(hotkey.as_str())
        .map_err(|e| CommandError {
            message: format!("快捷键无效或已被其他程序占用：{e}"),
        })?;
    let db = state.db.lock().unwrap();
    db.set_setting("hotkey", &hotkey)?;
    log::info!("hotkey changed to {hotkey}");
    Ok(())
}

/// 图片缩略图 base64（按 id 读取）
#[tauri::command]
pub fn get_thumb(state: State<'_, AppState>, id: i64) -> CmdResult<Option<String>> {
    let db = state.db.lock().unwrap();
    let Some(item) = db.get_item(id)? else {
        return Ok(None);
    };
    let Some(path) = item.thumb_path else {
        return Ok(None);
    };
    Ok(std::fs::read(&path).ok().map(|b| crate::monitor::base64_encode(&b)))
}

/// 图片原图 base64（大图预览用，按 id 读取）
#[tauri::command]
pub fn get_image(state: State<'_, AppState>, id: i64) -> CmdResult<Option<String>> {
    let db = state.db.lock().unwrap();
    let Some(item) = db.get_item(id)? else {
        return Ok(None);
    };
    let Some(path) = item.image_path else {
        return Ok(None);
    };
    Ok(std::fs::read(&path).ok().map(|b| crate::monitor::base64_encode(&b)))
}

/// 文件类型图标（Shell API，按文件路径）
#[tauri::command]
pub fn get_file_icon(path: String) -> CmdResult<Option<String>> {
    Ok(crate::file_icons::file_icon_png(&path))
}

/// 图片文件缩略图（按文件路径）
#[tauri::command]
pub fn get_file_thumb(path: String) -> CmdResult<Option<String>> {
    Ok(crate::file_icons::file_thumb_png(&path))
}

/// 图片文件大预览（按文件路径，最长边 1024）
#[tauri::command]
pub fn get_file_preview(path: String) -> CmdResult<Option<String>> {
    Ok(crate::file_icons::file_preview_png(&path))
}

/// 组装前端视图（缩略图 base64 内联，MVP 简单方案）
fn to_dto(_state: &AppState, item: &Item) -> ItemDto {
    let thumb = if item.kind == crate::models::ItemKind::Image {
        item.thumb_path.as_ref().and_then(|p| {
            std::fs::read(p)
                .ok()
                .map(|b| crate::monitor::base64_encode(&b))
        })
    } else {
        None
    };
    item.to_dto(thumb)
}
