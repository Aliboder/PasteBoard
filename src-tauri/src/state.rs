//! 进程级共享状态（不依赖 Tauri，便于测试）

use crate::db::Db;
use crate::store::FileStore;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize};
use std::sync::Mutex;

pub struct AppState {
    /// rusqlite::Connection 非 Sync，用 Mutex 串行化访问
    pub db: Mutex<Db>,
    pub store: FileStore,
    /// 自身写入剪贴板标志（监听侧据此忽略）
    pub self_write: AtomicBool,
    /// 唤起弹出窗前的前台窗口句柄（HWND，0 表示无）
    pub prev_foreground: AtomicIsize,
}

impl AppState {
    pub fn new(data_dir: PathBuf, db_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let store = FileStore::new(data_dir)?;
        let db = Db::open(db_path.to_str().unwrap_or(":memory:"))?;
        Ok(Self {
            db: Mutex::new(db),
            store,
            self_write: AtomicBool::new(false),
            prev_foreground: AtomicIsize::new(0),
        })
    }
}
