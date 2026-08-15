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

/// 可预览图片扩展名（文件条目是否算"图片"的判定口径，与前端一致）
const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "avif", "tif", "tiff",
];

/// 文件条目的首个文件是否为图片（网格/横条展示的就是首个文件）
fn first_file_is_image(item: &Item) -> bool {
    let paths: Vec<String> =
        serde_json::from_str(item.file_paths.as_deref().unwrap_or("[]")).unwrap_or_default();
    let Some(first) = paths.first() else {
        return false;
    };
    first
        .rsplit('.')
        .next()
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 历史列表（可搜索、按类型筛选、分页）；图片原图文件已缺失的条目自动隐藏（数据库保留）；
/// "图片" Tab 额外包含文件条目中的图片文件（复制的图片文件）
#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    filter: Option<String>,
    kind: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> CmdResult<Vec<ItemDto>> {
    let db = state.db.lock().unwrap();
    let mut items = db.list_items(
        filter.as_deref().unwrap_or(""),
        kind.as_deref(),
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )?;
    // 实时检测：图片原图被手动删除后不再显示（恢复文件后自动重新出现）
    items.retain(|it| match it.kind {
        crate::models::ItemKind::Image => it
            .image_path
            .as_deref()
            .map(|p| std::path::Path::new(p).exists())
            .unwrap_or(false),
        // 图片 Tab：文件条目仅保留首个文件为图片的
        crate::models::ItemKind::Files => kind.as_deref() != Some("image") || first_file_is_image(it),
        _ => true,
    });
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

/// 清空全部历史（含固定条目）
#[tauri::command]
pub fn clear_all_history(state: State<'_, AppState>) -> CmdResult<u32> {
    let db = state.db.lock().unwrap();
    let removed = db.clear_all()?;
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

/// 复制条目到剪贴板（不粘贴，右键菜单用）
#[tauri::command]
pub fn copy_item(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let item = {
        let db = state.db.lock().unwrap();
        db.get_item(id)
            .map_err(|e| CommandError { message: e.to_string() })?
            .ok_or(CommandError {
                message: "item not found".into(),
            })?
    };
    crate::paste::write_item_clipboard(&state, &item).map_err(|m| CommandError { message: m })
}

/// 打开文件所在位置（资源管理器定位）
#[tauri::command]
pub fn open_file_location(path: String) -> CmdResult<()> {
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{path}"))
        .spawn()
        .map_err(|e| CommandError {
            message: format!("无法打开所在位置: {e}"),
        })?;
    Ok(())
}

/// 用默认程序打开文件
#[tauri::command]
pub fn open_file(path: String) -> CmdResult<()> {
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| CommandError {
            message: format!("无法打开文件: {e}"),
        })?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct SettingsDto {
    pub max_items: i64,
    pub theme: String,
    /// 当前全局热键（"hotkey" 设置项）
    pub hotkey: String,
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
        hotkey: get_setting_str(&db, "hotkey", "Ctrl+Shift+V"),
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

/// 数据目录路径（设置展示用）
#[tauri::command]
pub fn get_data_dir(state: State<'_, AppState>) -> CmdResult<String> {
    Ok(state.store.root().to_string_lossy().into_owned())
}

/// 打开数据目录（主动清理入口）
#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.store.root();
    std::fs::create_dir_all(dir).map_err(|e| CommandError {
        message: format!("数据目录不可用: {e}"),
    })?;
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .map_err(|e| CommandError {
            message: format!("无法打开资源管理器: {e}"),
        })?;
    Ok(())
}

/// 数据统计（设置面板展示）
#[derive(Debug, Serialize)]
pub struct StatsDto {
    pub total: i64,
    pub text: i64,
    pub image: i64,
    pub files: i64,
    pub db_size: u64,
    pub media_size: u64,
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else if let Ok(meta) = std::fs::metadata(&path) {
                total += meta.len();
            }
        }
    }
    total
}

/// 数据统计：条目数量与磁盘占用
#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> CmdResult<StatsDto> {
    let db = state.db.lock().unwrap();
    let count = |kind: &str| -> i64 {
        db.count_by_kind(kind).unwrap_or(0)
    };
    let total: i64 = db.count_all().unwrap_or(0);
    let db_path = state.store.root().join("pasteboard.db");
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let media_size = dir_size(&state.store.root().join("images"))
        + dir_size(&state.store.root().join("thumbs"));
    Ok(StatsDto {
        total,
        text: count("text"),
        image: count("image"),
        files: count("files"),
        db_size,
        media_size,
    })
}

/// 恢复默认设置（含热键重新注册）
#[tauri::command]
pub fn reset_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let db = state.db.lock().unwrap();
    db.set_setting("max_items", "500")?;
    db.set_setting("theme", "dark")?;
    db.set_setting("follow_mouse", "on")?;
    db.set_setting("keep_open", "off")?;
    db.set_setting("always_on_top", "off")?;
    db.set_setting("hotkey", "Ctrl+Shift+V")?;
    db.set_setting("win_w", "0")?;
    db.set_setting("win_h", "0")?;
    drop(db);
    // 重新注册默认热键并同步窗口状态
    let _ = app.global_shortcut().unregister_all();
    let _ = app.global_shortcut().register("Ctrl+Shift+V");
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_always_on_top(false);
    }
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
    let db = state.db.lock().unwrap();
    // 与当前热键相同则无需重新注册
    if db.get_setting("hotkey").ok().flatten().as_deref() == Some(hotkey.as_str()) {
        return Ok(());
    }
    // 先注册新键：失败时旧热键仍然有效，不会出现"无热键"状态
    app.global_shortcut()
        .register(hotkey.as_str())
        .map_err(|e| CommandError {
            message: format!("快捷键无效或已被其他程序占用：{e}"),
        })?;
    // 成功：注销全部（旧键 + 刚注册的新键）后只注册新键，保证唯一
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut()
        .register(hotkey.as_str())
        .map_err(|e| CommandError {
            message: format!("快捷键无效或已被其他程序占用：{e}"),
        })?;
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
