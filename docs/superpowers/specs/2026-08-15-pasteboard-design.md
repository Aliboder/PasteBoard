# PasteBoard 设计文档

> 项目：本地剪贴板管理软件（Windows）
> 技术栈：Rust + Tauri v2 + Svelte（Vite/TS）+ SQLite（rusqlite）+ Windows API（windows crate）
> 设计日期：2026-08-15
> 状态：设计评审中（评审通过后进入实现规划）

---

## 1. 项目概述

### 1.1 目标

一款**小巧、本地优先、界面现代**的 Windows 剪贴板管理器：

- 常驻后台（托盘），自动记录剪贴板历史；
- 支持**文本、图片、任意文件**三类内容；
- 全局热键唤起弹出窗，搜索并一键粘贴回原窗口；
- 单文件体积目标 **≤ 15MB**（Tauri 打包），内存占用低。

### 1.2 非目标（MVP 刻意不做）

云同步、加密、分组、忽略规则、导入导出、脚本/自动化系统、开机自启、局域网共享、富文本（HTML）编辑。

### 1.3 角色定位

参考业界产品（CopyQ / Ditto / jietuba）的共性功能，做"小而精"的本地剪贴板管理器：**监听可靠、粘贴顺手、搜索快、界面现代**。

---

## 2. 技术栈与理由

| 层 | 选型 | 理由 |
|---|---|---|
| 外壳 | Tauri v2 | 复用系统 WebView2，打包体积小（~8-15MB），前后端一体 |
| 后端 | Rust（stable 2024+） | 剪贴板监听/热键/粘贴均为 Windows API 操作，Rust 原生性能与稳定性最佳 |
| Windows API | `windows` crate | 微软官方 crate，覆盖 `AddClipboardFormatListener`、CF_HDROP、SendInput 等 |
| 剪贴板读写 | `arboard` / `tauri-plugin-clipboard-manager` + 自写 CF_HDROP | 文本/图片用它，文件列表用 `windows` crate 自行处理 |
| 数据库 | `rusqlite`（bundled SQLite） | 单文件、零外部依赖、查询快（Ditto 同方案） |
| 哈希 | `sha2` | 去重指纹 |
| 图片 | `image` crate | 解码剪贴板位图、生成缩略图、编码 PNG |
| 前端 | Svelte 5 + Vite + TypeScript | 体积小、模板简单、适合列表类 UI |
| 样式 | 手写 CSS（CSS 变量主题） | 不引 UI 框架，保持小巧与可控的"现代化"外观 |
| 日志 | `log` + 简单文件输出 | 排查问题 |

### 2.1 环境要求（开发机）

- Rust 工具链（rustup）+ VS Build Tools（MSVC 组件）
- Node.js 18+（前端构建）
- Windows 10/11（自带 WebView2 Runtime）

---

## 3. 总体架构

```
┌────────────────────────────────────────────────┐
│  前端 (Svelte + Vite + TS)                      │
│  ├─ 弹出窗：搜索框 + 条目列表 + 固定/删除        │
│  └─ invoke ──→ Tauri 命令 ──→ Rust 后端          │
└───────────────────────┬────────────────────────┘
                        │ Tauri IPC
┌───────────────────────┴────────────────────────┐
│  Rust 后端 (src-tauri/src)                      │
│  ├─ main.rs       入口/托盘/热键/单实例/窗口     │
│  ├─ monitor.rs    剪贴板监听（事件+轮询兜底）     │
│  ├─ clipboard.rs  剪贴板读写（文本/图片/文件）    │
│  ├─ db.rs         SQLite 存储 + 上限清理         │
│  ├─ store.rs      文件落盘（原图/缩略图）        │
│  ├─ dedup.rs      去重 / 忽略自身写入            │
│  ├─ paste.rs      粘贴回上一窗口                │
│  ├─ commands.rs   前端可调用的命令层             │
│  └─ state.rs      全局状态（DB/前台窗口句柄/配置）│
└────────────────────────────────────────────────┘
```

### 3.1 模块职责（单一职责，可独立测试）

| 模块 | 职责 | 依赖 |
|---|---|---|
| `monitor.rs` | 监听剪贴板变化，判定类型，触发保存流程 | clipboard, dedup, db, store |
| `clipboard.rs` | 读写系统剪贴板（文本/图片/文件列表），不关心历史 | windows/arboard |
| `dedup.rs` | 计算内容指纹、去重、忽略自身写入 | clipboard |
| `db.rs` | 增删改查、固定、上限清理 | rusqlite |
| `store.rs` | 图片原图/缩略图写入数据目录 | image |
| `paste.rs` | 把条目内容写回剪贴板并模拟 Ctrl+V | clipboard, state |
| `commands.rs` | 暴露给前端的 invoke 接口，仅做编排 | 其余各模块 |
| `state.rs` | 进程级共享状态（DB 连接、前台窗口句柄、配置） | — |

> 设计原则：`monitor`/`clipboard`/`db` 等模块不依赖 Tauri，可单独写单元测试；只有 `commands.rs`/`main.rs` 接触 Tauri。

---

## 4. 数据设计

### 4.1 SQLite 表结构

```sql
CREATE TABLE items (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL CHECK(kind IN ('text','image','files')),
    content     TEXT,           -- kind=text: 纯文本内容
    file_paths  TEXT,           -- kind=files: 文件路径 JSON 数组
    image_path  TEXT,           -- kind=image: 原图 PNG 磁盘绝对路径
    thumb_path  TEXT,           -- kind=image: 缩略图磁盘绝对路径
    hash        TEXT NOT NULL UNIQUE,   -- 去重指纹
    pinned      INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL        -- Unix 毫秒
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

- `kind` 三种取值对应三类内容；文本直接存库，图片/文件引磁盘路径。
- 文件内容：MVP 只存**路径引用**（`file_paths` JSON），不复制文件本体。

### 4.2 settings 键（MVP）

| key | 默认值 | 说明 |
|---|---|---|
| `max_items` | `500` | 历史上限（可配） |
| `hotkey` | `Ctrl+Shift+V` | 唤起热键（实现期支持修改） |
| `theme` | `dark` | 主题 |

### 4.3 上限清理策略

- 插入前检查 `COUNT(*)`；超过 `max_items` 时，按 `created_at` 删除最旧的 **非固定** 条目（及其磁盘文件）。
- 固定条目（pinned=1）永不自动删除。
- 清理完成向前端发 `clipboard://pruned` 事件（携带被删 id）。

---

## 5. 剪贴板监听（monitor.rs）——核心

### 5.1 触发方式（双保险）

1. **主方案：事件驱动** —— 创建消息型隐藏窗口，调用 `AddClipboardFormatListener(hwnd)`，在 `WM_CLIPBOARDUPDATE` 消息回调里执行"读取→去重→存储"。由专用线程跑消息循环（`GetMessage/DispatchMessage`）。
2. **兜底方案：轮询** —— 独立线程每 300ms 比对剪贴板"指纹"（当前格式列表 + 内容哈希），发生变化时走同一套保存流程。

> 事件驱动最省资源且响应最快；轮询在个别应用绕过监听消息时兜底。两者共用一个"保存流水线"。

### 5.2 内容类型判定优先级

```
1. 有 CF_HDROP（文件列表）  → 类型 = files
2. 有 CF_DIB / CF_DIBV5     → 类型 = image（解码 → PNG 落盘 → 生成缩略图）
3. 有 CF_UNICODETEXT / HTML → 类型 = text（取纯文本，MVP 不保留 HTML）
4. 其他格式                → 忽略（不记录）
```

### 5.3 去重（dedup.rs）

| 类型 | 指纹算法 |
|---|---|
| text | `sha256(纯文本字节)` |
| image | 原图统一缩放到 32×32 RGBA，`sha256(像素字节)`（与编码格式无关，同图即同指纹） |
| files | `sha256(排序后的路径列表字符串)` |

- 命中重复：**不新增行**，把该行的 `created_at` 刷新到当前（"顶到最前"）。
- `hash` 列 UNIQUE 约束兜底。

### 5.4 忽略自身写入

- 凡是我们自己往剪贴板写内容（粘贴、演示），置原子标志 `self_write=true` 并记录时间戳。
- 监听侧收到变化后：若在标志有效期内（如 300ms），视为自身写入，**跳过存储**并清除标志。
- 避免"粘贴操作产生一条新历史"的循环。

---

## 6. 粘贴回上一窗口（paste.rs）

流程：

1. 热键唤起弹出窗前，用 `GetForegroundWindow()` 记录当前前台窗口句柄 → 存入 `state.prev_foreground`。
2. 用户点击/回车选中条目 → 前端调 `paste_item(id)`：
   - 从 DB 读取内容 → `clipboard.rs` 写入系统剪贴板（并置自身写入标志）；
   - `SetForegroundWindow(prev_foreground)` 还原焦点；
   - `SendInput` 模拟 `Ctrl+V`；
   - 隐藏弹出窗。
3. 边界：原窗口已关闭/最小化 → 仅写剪贴板并保持窗口，提示用户手动粘贴。

> Windows 的 `SetForegroundWindow` 存在权限限制；若触发失败，备用方案为 `AttachThreadInput` 转移输入归属。MVP 先实现标准路径，失败时降级为"只写剪贴板"。

---

## 7. 前端 UI（Svelte）

### 7.1 窗口形态

- 无边框（`decorations:false`）、圆角、半透明遮罩可选（`transparent:true` + CSS `border-radius`）。
- MVP 定位：屏幕中央或记忆上次位置；不跟随光标。

### 7.2 布局

```
┌───────────────────────────────────┐
│ 🔍 搜索…                    ⌨ 设置 │   ← 顶栏（搜索实时过滤）
├───────────────────────────────────┤
│ 📄 固定条目（星标置顶，可多条）     │
│ 📄 最近历史（倒序列表，无限滚动）   │
│   每行：                         │
│   - 文本：2 行预览（截断）         │
│   - 图片：圆角缩略图 + 类型标签     │
│   - 文件：文件图标 + 文件名/路径    │
│   - 右侧 hover 出现：星标 / 删除   │
└───────────────────────────────────┘
```

- 交互：`↑/↓` 选择，`Enter` 粘贴并关闭，`Esc` 关闭。
- 点击条目 = 粘贴并关闭；星标 = 固定/取消固定；垃圾桶 = 删除。
- 空态提示："暂无剪贴板历史"。

### 7.3 主题

- 深色主题为主（CSS 变量集中管理，便于后续扩展浅色）。
- 风格：圆角卡片、hover 高亮、简洁留白，参考现代工具类软件的观感。

### 7.4 图片缩略图加载

- MVP：`paste_item`/`get_history` 返回小尺寸 base64（~200px JPEG/PNG）直接内联显示，避免大图撑爆内存。
- 数据量大后可升级为 Tauri 自定义协议 `pasteboard://thumb/{id}` 按需加载。

---

## 8. 前后端通信协议

### 8.1 Tauri 命令（前端 `invoke` 调用）

| 命令 | 入参 | 返回 | 说明 |
|---|---|---|---|
| `get_history` | `filter?: string, limit?: number, offset?: number` | `Vec<ItemDTO>` | 历史列表（可按关键词过滤） |
| `pin_item` | `id, pinned` | `()` | 固定/取消 |
| `delete_item` | `id` | `()` | 删除单条 |
| `clear_history` | — | `()` | 清空全部非固定条目 |
| `paste_item` | `id` | `() -> PasteResult` | 粘贴并关闭（核心动作） |
| `get_settings` | — | `Settings` | 读配置 |
| `set_settings` | `Settings` | `()` | 写配置（max_items/hotkey 等） |

`ItemDTO`：
```rust
{ id: u64, kind: "text"|"image"|"files", preview: String,   // 文本预览或文件路径
  thumb?: String,          // 图片缩略图 base64
  file_count?: u32, pinned: bool, created_at: u64 }
```

### 8.2 事件（Rust → 前端）

| 事件 | 载荷 | 用途 |
|---|---|---|
| `clipboard://changed` | `ItemDTO` | 新条目入库后刷新列表 |
| `clipboard://pruned` | `Vec<id>` | 上限清理后移除已删条目 |

---

## 9. 错误处理与稳定性

- 剪贴板读取失败（应用占用/格式异常）→ 记录日志，跳过该次，不崩溃。
- DB 写失败 → 日志 + 继续运行（剪贴板仍可用，只是不入历史）。
- 全局异常：Rust `panic` 由主线程守卫捕获写日志；前端 UI 异常不影响后端监听。
- 单实例：`tauri-plugin-single-instance`，防止重复监听。
- 日志：`log` + 写入 `%APPDATA%/pasteboard/logs/`，设大小上限滚动。

---

## 10. 测试策略

| 层 | 内容 |
|---|---|
| Rust 单元测试 | db（增删/固定/上限清理）、dedup（指纹稳定与去重）、file_paths JSON 解析、settings 读写 |
| 前端 | MVP 阶段不做框架级测试，仅保证 `npm run build` 通过 |
| 手动验收清单 | 见下文 10.1 |

### 10.1 手动验收清单

1. 复制文本 → 历史出现 → 重启应用后仍在（持久化）。
2. 复制截图/网页图片 → 缩略图正常显示。
3. 资源管理器复制多个文件 → 历史出现文件条目。
4. 同一内容连续复制 → 只保留一条并顶到最前（去重）。
5. 固定 1 条后复制满上限 → 固定条目不被删。
6. 热键唤起 → 搜索 → Enter 粘贴回原窗口且弹出窗自动关闭。
7. 托盘菜单可退出；重复启动只弹一个实例。

---

## 11. 构建与交付

- `tauri build` → NSIS 安装包 + 便携 exe；验证体积（目标 ≤ 15MB）。
- 开发期 `npm run tauri dev` 热更新前端，Rust 改动需重启。
- 产物命名 `PasteBoard`，应用数据目录 `%APPDATA%/pasteboard/`。

---

## 12. 里程碑

| 里程碑 | 内容 | 依赖 |
|---|---|---|
| M1 骨架 | 环境 + Tauri 模板 + 插件 + 托盘/热键空跑通 | — |
| M2 数据层 | db + store + dedup 单测通过 | M1 |
| M3 监听 | monitor + clipboard 事件驱动跑通 | M2 |
| M4 前端 | 弹出窗 + 列表 + 搜索 + 固定/删除 | M3 |
| M5 粘贴 | paste 回原窗口闭环 | M4 |
| M6 打磨交付 | 配置/主题/打包/验收清单全过 | M5 |

---

## 13. 待确认点（实现期敲定，不影响当前设计评审）

1. 文件历史 MVP 只存**路径引用**（原文件被删则条目失效）——已默认，如需文件副本列入 V2。
2. 热键默认 `Ctrl+Shift+V`（与 PowerToys 冲突则改用 `Ctrl+Alt+V`，启动时检测）。
3. 弹出窗位置：MVP 取屏幕中央，后续可做记忆位置/跟随鼠标。

---

*设计评审完成后，将拆分为实现计划（按里程碑逐轮交付）。*
