//! SQLite 存储层：条目与设置读写、固定、上限清理

use crate::models::{Item, ItemKind};
use rusqlite::{Connection, OptionalExtension, params};

pub const DEFAULT_MAX_ITEMS: i64 = 500;

pub struct Db {
    conn: Connection,
}

#[derive(Debug)]
pub enum DbError {
    Sql(rusqlite::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sql(e) => write!(f, "sqlite error: {e}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sql(e)
    }
}

impl Db {
    /// 打开数据库并建表（测试可用 ":memory:"）
    pub fn open(path: &str) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        let db = Db { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS items (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                kind        TEXT NOT NULL CHECK(kind IN ('text','image','files')),
                content     TEXT,
                file_paths  TEXT,
                image_path  TEXT,
                thumb_path  TEXT,
                hash        TEXT NOT NULL UNIQUE,
                pinned      INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_items_created ON items(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_items_pinned ON items(pinned);",
        )?;
        Ok(())
    }

    // ---------- 条目 ----------

    /// 插入新条目；hash 冲突时忽略并返回 None
    pub fn insert_item(&self, item: &Item) -> Result<Option<i64>, DbError> {
        let result = self.conn.execute(
            "INSERT INTO items (kind, content, file_paths, image_path, thumb_path, hash, pinned, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                item.kind.as_str(),
                item.content,
                item.file_paths,
                item.image_path,
                item.thumb_path,
                item.hash,
                item.pinned as i64,
                item.created_at,
            ],
        );
        match result {
            Ok(_) => Ok(Some(self.conn.last_insert_rowid())),
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
                Ok(None) // hash 唯一冲突 → 已存在
            }
            Err(e) => Err(DbError::Sql(e)),
        }
    }

    /// 按 hash 查找条目 id
    pub fn find_by_hash(&self, hash: &str) -> Result<Option<i64>, DbError> {
        let id = self
            .conn
            .query_row(
                "SELECT id FROM items WHERE hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// 把已有条目顶到最前（刷新 created_at）
    pub fn touch_item(&self, id: i64, now_ms: i64) -> Result<(), DbError> {
        self.conn
            .execute(
                "UPDATE items SET created_at = ?1 WHERE id = ?2",
                params![now_ms, id],
            )?;
        Ok(())
    }

    pub fn get_item(&self, id: i64) -> Result<Option<Item>, DbError> {
        let item = self
            .conn
            .query_row(
                "SELECT id, kind, content, file_paths, image_path, thumb_path, hash, pinned, created_at
                 FROM items WHERE id = ?1",
                params![id],
                |row| row_to_item(row),
            )
            .optional()?;
        Ok(item)
    }

    /// 查询历史；keyword 非空时对文本内容/文件路径做 LIKE 过滤
    pub fn list_items(
        &self,
        keyword: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Item>, DbError> {
        let mut stmt;
        let items = if keyword.trim().is_empty() {
            stmt = self.conn.prepare(
                "SELECT id, kind, content, file_paths, image_path, thumb_path, hash, pinned, created_at
                 FROM items ORDER BY pinned DESC, created_at DESC LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt.query_map(params![limit, offset], |row| row_to_item(row))?;
            rows.collect::<Result<Vec<_>, _>>()?
        } else {
            let pattern = format!("%{}%", keyword.trim());
            stmt = self.conn.prepare(
                "SELECT id, kind, content, file_paths, image_path, thumb_path, hash, pinned, created_at
                 FROM items
                 WHERE content LIKE ?1 OR file_paths LIKE ?1
                 ORDER BY pinned DESC, created_at DESC LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt.query_map(params![pattern, limit, offset], |row| row_to_item(row))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        Ok(items)
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<bool, DbError> {
        let n = self.conn.execute(
            "UPDATE items SET pinned = ?1 WHERE id = ?2",
            params![pinned as i64, id],
        )?;
        Ok(n > 0)
    }

    /// 图片落盘后回填路径
    pub fn set_image_paths(
        &self,
        id: i64,
        image_path: Option<String>,
        thumb_path: Option<String>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE items SET image_path = ?1, thumb_path = ?2 WHERE id = ?3",
            params![image_path, thumb_path, id],
        )?;
        Ok(())
    }

    /// 删除单条；返回被删条目的磁盘文件路径（若有），供调用方清理文件
    pub fn delete_item(&self, id: i64) -> Result<Option<Item>, DbError> {
        let item = self.get_item(id)?;
        if item.is_some() {
            self.conn
                .execute("DELETE FROM items WHERE id = ?1", params![id])?;
        }
        Ok(item)
    }

    /// 上限清理：删除最旧的非固定条目，直到不超过 max_items；返回被删条目
    pub fn prune(&self, max_items: i64) -> Result<Vec<Item>, DbError> {
        if max_items < 1 {
            return Ok(vec![]);
        }
        let mut removed = Vec::new();
        loop {
            let count: i64 = self
                .conn
                .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?;
            if count <= max_items {
                break;
            }
            let id: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM items WHERE pinned = 0 ORDER BY created_at ASC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(id) = id else { break };
            if let Some(item) = self.delete_item(id)? {
                removed.push(item);
            }
        }
        Ok(removed)
    }

    pub fn clear_unpinned(&self) -> Result<Vec<Item>, DbError> {
        let items = self.list_items("", 100000, 0)?;
        let mut removed = Vec::new();
        for it in items {
            if !it.pinned {
                if let Some(deleted) = self.delete_item(it.id)? {
                    removed.push(deleted);
                }
            }
        }
        Ok(removed)
    }

    // ---------- 设置 ----------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        let v = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn max_items(&self) -> i64 {
        self.get_setting("max_items")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_ITEMS)
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    let kind: String = row.get(1)?;
    let kind = match kind.as_str() {
        "text" => ItemKind::Text,
        "image" => ItemKind::Image,
        "files" => ItemKind::Files,
        _ => return Err(rusqlite::Error::InvalidColumnType(1, kind, rusqlite::types::Type::Text)),
    };
    Ok(Item {
        id: row.get(0)?,
        kind,
        content: row.get(2)?,
        file_paths: row.get(3)?,
        image_path: row.get(4)?,
        thumb_path: row.get(5)?,
        hash: row.get(6)?,
        pinned: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
    })
}

// ---------- 工具 ----------

/// 当前时间（Unix 毫秒）
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_item(hash: &str, pinned: bool, created_at: i64) -> Item {
        Item {
            id: 0,
            kind: ItemKind::Text,
            content: Some(format!("content-{hash}")),
            file_paths: None,
            image_path: None,
            thumb_path: None,
            hash: hash.to_string(),
            pinned,
            created_at,
        }
    }

    fn open_mem() -> Db {
        Db::open(":memory:").unwrap()
    }

    #[test]
    fn insert_and_get() {
        let db = open_mem();
        let item = test_item("h1", false, 1000);
        let id = db.insert_item(&item).unwrap().unwrap();
        let got = db.get_item(id).unwrap().unwrap();
        assert_eq!(got.hash, "h1");
        assert_eq!(got.kind, ItemKind::Text);
        assert_eq!(got.content.as_deref(), Some("content-h1"));
    }

    #[test]
    fn hash_unique_conflict_returns_none() {
        let db = open_mem();
        db.insert_item(&test_item("dup", false, 1000)).unwrap();
        let second = db.insert_item(&test_item("dup", false, 2000)).unwrap();
        assert!(second.is_none());
        assert_eq!(db.list_items("", 100, 0).unwrap().len(), 1);
    }

    #[test]
    fn touch_moves_to_top() {
        let db = open_mem();
        let a = db.insert_item(&test_item("a", false, 100)).unwrap().unwrap();
        let b = db.insert_item(&test_item("b", false, 200)).unwrap().unwrap();
        db.touch_item(a, 300).unwrap();
        let list = db.list_items("", 100, 0).unwrap();
        assert_eq!(list[0].id, a);
        assert_eq!(list[1].id, b);
    }

    #[test]
    fn list_filters_by_keyword() {
        let db = open_mem();
        db.insert_item(&test_item("a", false, 100)).unwrap();
        let mut files_item = test_item("b", false, 200);
        files_item.kind = ItemKind::Files;
        files_item.file_paths = Some(r#"["C:\\temp\\report.pdf"]"#.to_string());
        db.insert_item(&files_item).unwrap();
        let list = db.list_items("report", 100, 0).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, ItemKind::Files);
    }

    #[test]
    fn pinned_sorted_first() {
        let db = open_mem();
        let a = db.insert_item(&test_item("a", false, 100)).unwrap().unwrap();
        let b = db.insert_item(&test_item("b", false, 200)).unwrap().unwrap();
        db.set_pinned(b, true).unwrap();
        let list = db.list_items("", 100, 0).unwrap();
        assert_eq!(list[0].id, b);
        assert_eq!(list[1].id, a);
    }

    #[test]
    fn prune_keeps_pinned_removes_oldest() {
        let db = open_mem();
        let p = db.insert_item(&test_item("p", true, 100)).unwrap().unwrap();
        let x = db.insert_item(&test_item("x", false, 200)).unwrap().unwrap();
        let y = db.insert_item(&test_item("y", false, 300)).unwrap().unwrap();
        let z = db.insert_item(&test_item("z", false, 400)).unwrap().unwrap();
        let removed = db.prune(2).unwrap();
        // 上限 2：保留固定 p + 最新 z；删掉最旧的两个 x、y
        assert_eq!(removed.len(), 2);
        let ids: Vec<i64> = removed.iter().map(|i| i.id).collect();
        assert!(ids.contains(&x) && ids.contains(&y));
        let remaining = db.list_items("", 100, 0).unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].id, p);
        assert_eq!(remaining[1].id, z);
    }

    #[test]
    fn settings_roundtrip_and_default() {
        let db = open_mem();
        assert_eq!(db.max_items(), DEFAULT_MAX_ITEMS);
        db.set_setting("max_items", "123").unwrap();
        assert_eq!(db.max_items(), 123);
        assert_eq!(db.get_setting("nope").unwrap(), None);
    }
}
