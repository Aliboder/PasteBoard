//! Tauri 命令层：前端 invoke 的入口，仅做编排

use crate::models::{Item, ItemDto};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

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

#[derive(Debug, Serialize)]
pub struct SettingsDto {
    pub max_items: i64,
}

/// 读设置
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<SettingsDto> {
    let db = state.db.lock().unwrap();
    Ok(SettingsDto {
        max_items: db.max_items(),
    })
}

/// 写设置
#[tauri::command]
pub fn set_max_items(state: State<'_, AppState>, max_items: i64) -> CmdResult<()> {
    let db = state.db.lock().unwrap();
    db.set_setting("max_items", &max_items.to_string())?;
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
