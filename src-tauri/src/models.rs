//! 剪贴板历史条目数据模型

use serde::Serialize;

/// 条目类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Text,
    Image,
    Files,
}

impl ItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemKind::Text => "text",
            ItemKind::Image => "image",
            ItemKind::Files => "files",
        }
    }
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 剪贴板历史条目
#[derive(Debug, Clone)]
pub struct Item {
    pub id: i64,
    pub kind: ItemKind,
    /// kind=text 时存纯文本
    pub content: Option<String>,
    /// kind=files 时存文件路径 JSON 数组
    pub file_paths: Option<String>,
    /// kind=image 时存原图 PNG 磁盘绝对路径
    pub image_path: Option<String>,
    /// kind=image 时存缩略图磁盘绝对路径
    pub thumb_path: Option<String>,
    /// 去重指纹（sha256 十六进制）
    pub hash: String,
    pub pinned: bool,
    /// Unix 毫秒
    pub created_at: i64,
}

/// 文本预览截断长度（列表显示用，全文保留在 DB）
pub const PREVIEW_MAX_CHARS: usize = 300;

/// 返回给前端的条目视图（序列化用）
#[derive(Debug, Clone, Serialize)]
pub struct ItemDto {
    pub id: i64,
    pub kind: String,
    /// 文本预览（截断）或文件路径展示
    pub preview: String,
    /// 超长文本的全文（仅 preview 被截断时携带，供悬停预览）
    pub full: Option<String>,
    /// 图片缩略图 base64（仅 image 类型）
    pub thumb: Option<String>,
    pub file_count: u32,
    pub pinned: bool,
    pub created_at: i64,
}

impl Item {
    /// 构建前端视图；thumb 由调用方补充（读文件转 base64）
    pub fn to_dto(&self, thumb: Option<String>) -> ItemDto {
        let (preview, full, file_count) = match self.kind {
            ItemKind::Text => {
                let content = self.content.clone().unwrap_or_default();
                let long = content.chars().count() > PREVIEW_MAX_CHARS;
                let preview = if long {
                    content.chars().take(PREVIEW_MAX_CHARS).collect()
                } else {
                    content.clone()
                };
                (preview, if long { Some(content) } else { None }, 0)
            }
            ItemKind::Image => (String::from("图片"), None, 0),
            ItemKind::Files => {
                let paths: Vec<String> = serde_json::from_str(
                    self.file_paths.as_deref().unwrap_or("[]"),
                )
                .unwrap_or_default();
                let count = paths.len() as u32;
                let preview = paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| String::from("文件"));
                (preview, None, count)
            }
        };
        ItemDto {
            id: self.id,
            kind: self.kind.to_string(),
            preview,
            full,
            thumb,
            file_count,
            pinned: self.pinned,
            created_at: self.created_at,
        }
    }
}
