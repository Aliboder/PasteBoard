# AGENTS.md — PasteBoard 项目协作指南

本文件供 AI 助手（opencode 等）在本仓库工作时遵循。请先阅读全局用户画像（`~/.config/opencode/AGENTS.md`），再结合本文件执行任务。

---

## 1. 项目信息

**PasteBoard**：Windows 本地剪贴板管理器（小巧、现代、本地优先）。

| 项 | 说明 |
|---|---|
| 技术栈 | Rust + Tauri v2 + Svelte 5(TS) + SQLite(rusqlite) + Windows API |
| 平台 | Windows 10/11（仅桌面） |
| 数据目录 | `%APPDATA%\com.aliboder.pasteboard\`（db/images/thumbs/log） |
| 版本 | v0.3.0（GitHub Releases 同步发布） |
| 仓库 | github.com/Aliboder/PasteBoard（私有） |

### 核心功能

- 剪贴板历史：**文本 / 图片 / 文件**三类，事件驱动监听（`AddClipboardFormatListener`）+ 轮询兜底
- 上下分区布局：上方图片/文件横向条（滚轮横滚），下方文本列表（自适应行数，悬停全文预览）
- 文件图标：Shell API 系统图标（`SHGetFileInfo`），图片文件直接显示缩略图
- 粘贴回原窗口：焦点控件 + 选中范围（`EM_GETSEL/SETSEL`）精确恢复
- 点击窗口外部自动隐藏（失焦延迟 250ms + 缩放/移动事件识别）
- 设置：主题（深/浅/跟随系统）、自定义热键（按键录制）、上限、开机自启、打开数据目录、数据统计、恢复默认

### 目录结构

```
src/                      # 前端（SvelteKit SPA）
  routes/+page.svelte     # 主窗口（唯一页面）
  lib/api.ts              # invoke 封装
  lib/FileTile.svelte     # 文件卡片（图标/缩略图）
  lib/theme.css           # 共享主题变量（深/浅）
src-tauri/
  src/lib.rs              # 入口：托盘/热键/窗口定位/插件/状态
  src/commands.rs         # Tauri 命令层（编排）
  src/monitor.rs          # 剪贴板监听 + 入库流水线
  src/clipboard.rs        # 剪贴板读写（文本/图片/文件）
  src/paste.rs            # 粘贴回原窗口（焦点/选区恢复）
  src/file_icons.rs       # Shell 图标 + 图片缩略图/预览
  src/db.rs / store.rs / dedup.rs / models.rs / state.rs
```

---

## 2. 工作流程（最重要）

### 需求处理流程（用户明确要求）

```
用户提出需求
   ↓
AI 复述理解 + 提出方案（含关键决策点）
   ↓
用户确认理解与方案（"可以/没问题"）
   ↓
AI 开始修改
   ↓
AI 自检（编译/类型检查/测试）+ 请用户验收
   ↓
用户验收通过后才算完成
```

**硬性规则**：
1. **先确认后动手**：任何功能/界面/交互变更，必须先复述理解和方案、等待用户确认；用户说"可以/没问题"后再改。大型变更尤其如此（用户常主动要求"你说说你的理解"）。
2. **方案要展开**：提供方案时讲清优缺点、影响范围、预估耗时，给推荐意见；有更简单做法或发现矛盾时直接指出。
3. **小步交付**：每完成一个子步骤汇报一次，等待确认，不要默默做完再统一告知。
4. **完成标准**：自己先验证（命令输出/日志/截图证据），用户亲自验收通过才算完成。
5. **git 操作主动代做**：用户不会用 git，涉及提交/推送/发布要主动协助。常规节奏：每批改动用户验收后，提交并推送；版本发布时同步打 tag + GitHub Release。

### 2.5 GitHub 推送与版本发布（详细流程）

**仓库信息**
- 地址：`github.com/Aliboder/PasteBoard`（**私有**），默认分支 `main`
- 工具：`gh` CLI 已登录（账号 `Aliboder`，token 存于系统凭据 keyring）

**常规推送流程（每批功能/修复后）**
1. 用户验收通过后，主动提交并推送（无需另行吩咐）
2. 提交前先 `git status` / `git diff` 检查，只暂存预期文件
3. 构建产物不提交（已在 .gitignore：`.svelte-kit/`、`build/`、`target/`、`node_modules/`、`src-tauri/gen/`）
4. 提交信息格式：`type: 中文描述`，type 用 `feat` / `fix` / `docs` / `chore` / `refactor`（如 `feat: 设置面板美化`）
5. `git push` 推送 `main`

**版本发布流程（功能批次完成后）**
1. **三处同步升版本号**：`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、`README.md` 版本行
2. 全量自检：`cargo check`、`cargo test`、`npm run check`、`npm run build`
3. 打包：`npm run tauri build` → 产物在 `src-tauri/target/release/bundle/`（nsis/ 与 msi/ 子目录 + 便携 exe）
4. **清理旧版本安装包**（只保留当前版本产物）
5. 提交并推送代码
6. 创建 Release：
   ```bash
   gh release create vX.Y.Z \
     --title "PasteBoard vX.Y.Z" \
     --notes-file release-notes.md \
     <nsis-setup.exe> <msi> <便携 exe>
   ```
   - 说明文档分板块（中文）：`📦 安装包说明` / `✨ 更新内容` / `🔧 修复内容` / `🗺️ 后续规划`
   - 更新内容与修复内容要**分列清晰**，逐条描述实际变更

**注意事项**
- Release 说明必须分区撰写，功能与修复分开，避免混在一起
- 用户不会用 git——以上全部由 AI 代做，只向用户汇报结果与链接（如 `https://github.com/Aliboder/PasteBoard/releases/tag/vX.Y.Z`）

### 常用命令

```bash
npm run tauri dev    # 开发模式（前端热更新，Rust 改动自动重建）
npm run check        # 前端类型检查（svelte-check）——必跑！
cargo check          # Rust 编译检查
cargo test           # Rust 单元测试（12 项）
npm run build        # 前端构建
npm run tauri build  # 打包（NSIS + MSI + 便携 exe）
```

---

## 3. 项目专属注意事项

- **端口**：vite 用 3001/3002（本机 Hyper-V 端口排除段 1068-1467，1420 等不可用）
- **开发实例与安装版冲突**：dev 模式与 release 版同时运行会因单实例机制互相挤掉；切版本前先结束进程（`Stop-Process -Name pasteboard`）
- **dev 日志**：后台启动的 dev 实例输出重定向到 `%TEMP%\opencode\tauri-dev*.log`，排查问题先看它和 `%APPDATA%\com.aliboder.pasteboard\pasteboard.log`
- **前端改动必须跑 `npm run check`**：vite 构建不做类型检查，曾因漏 import 导致运行时错误（`getFilePreview` 未导入，悬停预览静默失败）
- **透明窗口**：`transparent: true` 曾导致白屏问题，已移除；不要再随意开启
- **数据库**：SQLite 单文件，`items`/`settings` 表；图片原图存 `images/{id}.png`，缩略图存 `thumbs/`；**图片原图文件缺失的条目在 `get_history` 中自动过滤隐藏**（数据库保留，恢复文件可找回）
- **测试数据**：本地数据库可能混有开发期测试条目，勿当成 bug
- **交互细节**：主窗口键盘全局监听（`svelte:window`）；设置面板内联；点击条目/Enter 粘贴并关闭（"粘贴后保持打开"可关）；窗口失焦延迟隐藏（250ms，缩放/移动时取消）
