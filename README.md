# PasteBoard

本地剪贴板管理器（Windows）——小巧、现代、本地优先。

## 功能

- 自动记录剪贴板历史：**文本 / 图片 / 文件**三种类型
- 全局热键 `Ctrl+Shift+V` 唤起弹出窗，回车/点击**一键粘贴回原窗口**（焦点与光标位置精确恢复）
- 实时搜索过滤、星标固定（不随上限清理删除）、单条/批量删除
- 历史上限可配置（默认 500 条），SQLite 本地存储，重启不丢
- 系统托盘常驻，单实例运行

## 使用

| 操作 | 说明 |
|---|---|
| `Ctrl+Shift+V` | 唤起 / 隐藏主窗口 |
| `↑` / `↓` | 选择条目 |
| `Enter` / 点击 | 粘贴到唤起前的窗口并自动关闭 |
| `Esc` | 关闭窗口 |
| 条目右侧 ★ | 固定 / 取消固定 |
| 条目右侧 × | 删除单条 |
| 顶栏 🗑 | 清空全部非固定历史 |
| 顶栏 ⚙ | 设置（历史上限） |

## 开发

```bash
# 环境要求：Rust（MSVC 工具链）、Node.js 18+
npm install          # 安装前端依赖
npm run tauri dev    # 开发模式（热更新）
npm run tauri build  # 打包（NSIS 安装包 + MSI）
```

## 数据位置

| 内容 | 路径 |
|---|---|
| 数据库 / 图片 / 缩略图 | `%APPDATA%\com.aliboder.pasteboard\` |
| 日志 | `%APPDATA%\com.aliboder.pasteboard\pasteboard.log` |

## 技术栈

Rust + Tauri v2 + Svelte(TS) + SQLite(rusqlite) + Windows API（`AddClipboardFormatListener` 事件监听 + 轮询兜底）

## 说明与限制

- 文件类型历史保存的是**路径引用**，原文件被删除/移动后条目会失效（V2 计划支持文件副本）
- 相同内容重复复制会去重并顶到最前
- 密码框等场景请谨慎使用历史功能

## 版本

v0.1.0（MVP）· 2026-08-15
