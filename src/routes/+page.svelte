<script lang="ts">
  import { onMount } from "svelte";
  import {
    getHistory,
    pinItem,
    deleteItem,
    clearHistory,
    pasteItem,
    getImage,
    getFilePreview,
    getSettings,
    setMaxItems,
    setTheme,
    setHotkey,
    setToggle,
    setAutostart,
    setWindowSize,
    onChange,
    type ItemDto,
    type SettingsDto,
  } from "../lib/api";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import FileTile from "../lib/FileTile.svelte";
  import {
    Search,
    X,
    Trash2,
    Settings,
    Pin,
    PinOff,
    Image as ImageIcon,
    ClipboardList,
  } from "lucide-svelte";

  /** 可大预览的图片扩展名（files 类型判断用） */
  const PREVIEW_IMAGE_EXTS = new Set([
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "avif", "tif", "tiff",
  ]);

  let items: ItemDto[] = $state([]);
  let filter = $state("");
  let selected = $state(-1);
  let loading = $state(true);
  let error = $state("");
  let showSettings = $state(false);
  let settings = $state<SettingsDto | null>(null);
  let maxItemsInput = $state("500");
  let themeSel = $state("dark");
  let hotkeyInput = $state("Ctrl+Shift+V");
  let settingsMsg = $state("");
  let hoverPreview = $state<{ src: string; top: number; height: number } | null>(null);
  let textPreview = $state<{ text: string; top: number; height: number } | null>(null);
  const previewCache = new Map<number, string>();
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  let textHoverTimer: ReturnType<typeof setTimeout> | null = null;
  let listSectionEl: HTMLElement | undefined = $state();
  let resizeTimer: ReturnType<typeof setTimeout> | null = null;

  /** 主题：dark / light / system（system 跟随系统深色模式，实时响应变化） */
  let mediaDark: MediaQueryList | null = null;
  let mediaHandler: (() => void) | null = null;

  function applyTheme(theme: string) {
    if (theme === "system") {
      mediaDark = window.matchMedia("(prefers-color-scheme: dark)");
      document.documentElement.setAttribute(
        "data-theme",
        mediaDark.matches ? "dark" : "light"
      );
      if (mediaHandler && mediaDark) mediaDark.removeEventListener("change", mediaHandler);
      mediaHandler = () => {
        if (themeSel === "system" && mediaDark) {
          document.documentElement.setAttribute(
            "data-theme",
            mediaDark.matches ? "dark" : "light"
          );
        }
      };
      mediaDark.addEventListener("change", mediaHandler);
    } else {
      if (mediaHandler && mediaDark) {
        mediaDark.removeEventListener("change", mediaHandler);
        mediaHandler = null;
      }
      document.documentElement.setAttribute("data-theme", theme);
    }
  }

  /** 窗口尺寸记忆：resize 结束后（500ms 防抖）保存逻辑尺寸 */
  function watchResize() {
    const win = getCurrentWindow();
    win.onResized(async () => {
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(async () => {
        try {
          const size = await win.innerSize();
          const scale = await win.scaleFactor();
          await setWindowSize(size.width / scale, size.height / scale);
        } catch {
          /* ignore */
        }
      }, 500);
    });
  }

  /** 横向条滚轮：垂直增量转为水平滚动（deltaMode=1 为行模式，乘 24 近似像素） */
  function onStripWheel(e: WheelEvent) {
    e.preventDefault();
    const strip = e.currentTarget as HTMLElement;
    const factor = e.deltaMode === 1 ? 24 : 1;
    strip.scrollLeft += e.deltaY * factor;
  }

  /** 时间显示：今天 → HH:mm，昨天 → 昨天 HH:mm，更早 → MM-DD HH:mm */
  function timeLabel(ms: number): string {
    const d = new Date(ms);
    const now = new Date();
    const hhmm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    if (d.toDateString() === now.toDateString()) return hhmm;
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (d.toDateString() === yesterday.toDateString()) return `昨天 ${hhmm}`;
    return `${d.getMonth() + 1}-${d.getDate()} ${hhmm}`;
  }

  async function openSettings() {
    showSettings = true;
    settingsMsg = "";
    settings = await getSettings();
    maxItemsInput = String(settings.max_items);
    themeSel = settings.theme;
  }

  async function saveSettings() {
    const n = parseInt(maxItemsInput, 10);
    if (isNaN(n) || n < 1) {
      settingsMsg = "请输入大于 0 的数字";
      return;
    }
    await setMaxItems(n);
    await setTheme(themeSel);
    applyTheme(themeSel);
    settingsMsg = "已保存";
  }

  async function saveHotkey() {
    settingsMsg = "";
    try {
      await setHotkey(hotkeyInput.trim());
      settingsMsg = `热键已生效：${hotkeyInput.trim()}`;
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  async function toggleSetting(key: string, enabled: boolean) {
    settingsMsg = "";
    try {
      await setToggle(key, enabled ? "on" : "off");
      if (settings) settings[key as "follow_mouse" | "keep_open" | "always_on_top"] = enabled ? "on" : "off";
      settingsMsg = "已保存";
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  async function toggleAutostart(enabled: boolean) {
    settingsMsg = "";
    try {
      await setAutostart(enabled);
      if (settings) settings.autostart = enabled;
      settingsMsg = enabled ? "已开启开机自启" : "已关闭开机自启";
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  /** 分流：上方横向区 = 图片+文件；下方列表 = 文本 */
  let topItems = $derived(items.filter((i) => i.kind !== "text"));
  let textItems = $derived(items.filter((i) => i.kind === "text"));

  async function reload() {
    loading = true;
    items = await getHistory(filter, 500, 0);
    if (selected >= textItems.length) selected = -1;
    loading = false;
  }

  /** 横向卡片显示名：取文件名（多文件时加数量） */
  function fileName(item: ItemDto): string {
    const base = item.preview.split(/[\\/]/).pop() ?? item.preview;
    return item.file_count > 1 ? `${base} 等${item.file_count}个` : base;
  }

  async function togglePin(item: ItemDto) {
    const ok = await pinItem(item.id, !item.pinned);
    if (ok) await reload();
  }

  async function remove(item: ItemDto) {
    await deleteItem(item.id);
    await reload();
  }

  async function clearAll() {
    await clearHistory();
    await reload();
  }

  async function paste(id: number) {
    error = "";
    try {
      await pasteItem(id);
      const keepOpen = settings?.keep_open === "on";
      if (!keepOpen) await getCurrentWindow().hide();
    } catch (e) {
      error = String(e);
    }
  }

  /** 图片悬停大图预览（350ms 延迟，原图懒加载并缓存）；
   *  image 类型走库内原图；files 类型仅对图片文件（按扩展名）走路径读取；
   *  位置固定在图片条下方，高度随窗口剩余空间自适应，不遮挡图片区 */
  async function showPreview(item: ItemDto, rowEl: HTMLElement) {
    if (item.kind !== "image" && item.kind !== "files") return;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = setTimeout(async () => {
      let src = previewCache.get(item.id);
      if (!src) {
        if (item.kind === "image") {
          src = (await getImage(item.id)) ?? "";
        } else {
          // files：第一个文件路径，仅图片扩展名可预览
          const path = item.preview;
          const ext = path.split(".").pop()?.toLowerCase() ?? "";
          if (!PREVIEW_IMAGE_EXTS.has(ext)) return;
          src = (await getFilePreview(path)) ?? "";
        }
        if (src) previewCache.set(item.id, src);
      }
      if (!src) return;
      const MAX_H = 300;
      const FOOTER_RESERVE = 44;
      const itemBottom = rowEl.offsetTop + rowEl.offsetHeight;
      // 优先显示在条目（图片条）下方
      let top = itemBottom + 8;
      let avail = window.innerHeight - FOOTER_RESERVE - top;
      if (avail < 80) {
        // 下方空间不足：改到条目上方（受顶部边界约束）
        top = Math.max(8, itemBottom - MAX_H - 8);
        avail = window.innerHeight - FOOTER_RESERVE - top;
      }
      const height = Math.min(MAX_H, Math.max(80, avail));
      hoverPreview = { src, top, height };
    }, 350);
  }

  function hidePreview() {
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverPreview = null;
  }

  /** 文本悬停全文预览：350ms 延迟；3 行内能显示完的短文本不弹，避免打扰 */
  function showTextPreview(item: ItemDto, rowEl: HTMLElement) {
    const text = item.preview;
    // 约 3 行 ≈ 80 字符，无换行且更短的内容无需预览
    if (text.length <= 80 && !text.includes("\n")) return;
    if (textHoverTimer) clearTimeout(textHoverTimer);
    textHoverTimer = setTimeout(() => {
      if (!listSectionEl) return;
      const rowRect = rowEl.getBoundingClientRect();
      const secRect = listSectionEl.getBoundingClientRect();
      const MAX_H = 300;
      // 优先显示在行下方
      let top = rowRect.bottom - secRect.top + 8;
      let below = secRect.bottom - 8 - rowRect.bottom;
      let height = Math.min(MAX_H, Math.max(80, below));
      if (below < 80) {
        // 下方空间不足：显示在行上方
        height = Math.min(MAX_H, Math.max(80, rowRect.top - secRect.top - 16));
        top = Math.max(8, rowRect.top - secRect.top - height - 8);
      }
      textPreview = { text, top, height };
    }, 350);
  }

  function hideTextPreview() {
    if (textHoverTimer) clearTimeout(textHoverTimer);
    textPreview = null;
  }

  /** 全局键盘：↑↓ 选择、Enter 粘贴、Esc 关闭、数字 1~9 快捷粘贴、Delete 删除（作用于文本区） */
  function globalKeydown(e: KeyboardEvent) {
    const inInput = document.activeElement?.tagName === "INPUT";
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (textItems.length) selected = (selected + 1) % textItems.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (textItems.length) selected = selected <= 0 ? textItems.length - 1 : selected - 1;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (selected >= 0 && textItems[selected]) paste(textItems[selected].id);
    } else if (e.key === "Escape") {
      getCurrentWindow().hide();
    } else if (e.key === "Delete" && !inInput) {
      if (selected >= 0 && textItems[selected]) remove(textItems[selected]);
    } else if (/^[1-9]$/.test(e.key) && !inInput) {
      const idx = parseInt(e.key, 10) - 1;
      if (textItems[idx]) paste(textItems[idx].id);
    }
  }

  onMount(() => {
    reload();
    // 应用已保存的主题与窗口尺寸
    getSettings()
      .then(async (s) => {
        settings = s;
        themeSel = s.theme;
        applyTheme(s.theme);
        if (s.win_w > 0 && s.win_h > 0) {
          await getCurrentWindow().setSize(new LogicalSize(s.win_w, s.win_h));
        }
      })
      .catch(() => applyTheme("dark"));
    watchResize();
    let cleanup: (() => void) | null = null;
    onChange(() => reload()).then((un) => (cleanup = un));
    return () => cleanup?.();
  });
</script>

<svelte:head>
  <title>PasteBoard</title>
</svelte:head>

<svelte:window onkeydown={globalKeydown} />

<div class="window">
  <!-- 顶栏：拖动区域（deep = 子树内非交互元素均可拖动）+ 搜索 + 清空 -->
  <header class="topbar" data-tauri-drag-region="deep">
    <div class="searchbox">
      <Search size={14} class="search-icon" />
      <input
        placeholder="搜索剪贴板历史…"
        bind:value={filter}
        oninput={reload}
        spellcheck="false"
      />
      {#if filter}
        <button class="icon-btn" title="清除" onclick={() => (filter = "")}>
          <X size={13} />
        </button>
      {/if}
    </div>
    <button class="icon-btn clear-btn" title="清空历史" onclick={clearAll}>
      <Trash2 size={14} />
    </button>
    <button
      class="icon-btn {showSettings ? 'active' : ''}"
      title="设置"
      onclick={() => (showSettings ? (showSettings = false) : openSettings())}
    >
      <Settings size={14} />
    </button>
  </header>

  {#if showSettings}
    <section class="settings-panel">
      <div class="settings-row">
        <label>
          历史上限
          <input
            type="number"
            min="1"
            max="100000"
            bind:value={maxItemsInput}
            onkeydown={(e) => {
              if (e.key === "Enter") saveSettings();
            }}
          />条
        </label>
        <label>
          主题
          <select bind:value={themeSel}>
            <option value="dark">深色</option>
            <option value="light">浅色</option>
            <option value="system">跟随系统</option>
          </select>
        </label>
        <button class="save-btn" onclick={saveSettings}>保存</button>
      </div>

      <div class="settings-row">
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings?.follow_mouse === "on"}
            onchange={(e) => toggleSetting("follow_mouse", (e.currentTarget as HTMLInputElement).checked)}
          />
          唤起跟随鼠标
        </label>
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings?.keep_open === "on"}
            onchange={(e) => toggleSetting("keep_open", (e.currentTarget as HTMLInputElement).checked)}
          />
          粘贴后保持打开
        </label>
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings?.always_on_top === "on"}
            onchange={(e) => toggleSetting("always_on_top", (e.currentTarget as HTMLInputElement).checked)}
          />
          窗口置顶
        </label>
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings?.autostart ?? false}
            onchange={(e) => toggleAutostart((e.currentTarget as HTMLInputElement).checked)}
          />
          开机自启
        </label>
      </div>

      <div class="settings-row">
        <label>
          全局快捷键
          <input
            class="hotkey-input"
            placeholder="如 Ctrl+Shift+V"
            bind:value={hotkeyInput}
            onkeydown={(e) => {
              if (e.key === "Enter") saveHotkey();
            }}
          />
        </label>
        <button class="save-btn" onclick={saveHotkey}>应用</button>
        <span class="settings-msg">{settingsMsg}</span>
      </div>

      <p class="settings-hint">
        快捷键格式参考：<code>Ctrl+Shift+V</code>、<code>Alt+Q</code>、<code>CmdOrCtrl+Shift+C</code><br />
        窗口可拖动边缘调整大小，尺寸会自动记忆<br />
        数据目录：<code>%APPDATA%\com.aliboder.pasteboard</code>
      </p>
    </section>
  {/if}

  <!-- 内容区：上方媒体横向区（20%）+ 下方文本列表 -->
  <div class="content">
    <!-- 上方：图片 / 文件，横向排布 -->
    <section class="strip-section">
      <div class="section-header">
        <span class="section-title">
          <ImageIcon size={12} />
          图片 / 文件
        </span>
        <span class="section-count">{topItems.length} 条</span>
      </div>
      {#if topItems.length === 0}
        <p class="strip-empty">{filter ? "无匹配结果" : "暂无图片/文件历史"}</p>
      {:else}
        <div class="strip" onwheel={onStripWheel}>
          {#each topItems as item (item.id)}
            <div
              class="strip-item {item.kind}"
              role="option"
              aria-label={item.preview}
              tabindex="-1"
              title={item.kind === "image"
                ? `图片 · ${timeLabel(item.created_at)}`
                : `${item.preview} · ${timeLabel(item.created_at)}`}
              onmouseenter={(e) => showPreview(item, e.currentTarget as HTMLElement)}
              onmouseleave={hidePreview}
              onclick={() => paste(item.id)}
              onkeydown={(e) => {
                if (e.key === "Enter") paste(item.id);
              }}
            >
              {#if item.kind === "image"}
                {#if item.thumb}
                  <img src="data:image/png;base64,{item.thumb}" alt="图片" />
                {:else}
                  <span class="strip-placeholder">
                    <ImageIcon size={16} />
                  </span>
                {/if}
              {:else}
                <FileTile path={item.preview} name={fileName(item)} />
              {/if}
              <div class="strip-actions">
                <button
                  class="icon-btn mini {item.pinned ? 'active' : ''}"
                  title={item.pinned ? "取消固定" : "固定"}
                  onclick={(e) => {
                    e.stopPropagation();
                    togglePin(item);
                  }}
                >
                  {#if item.pinned}
                    <Pin size={11} fill="currentColor" />
                  {:else}
                    <PinOff size={11} />
                  {/if}
                </button>
                <button
                  class="icon-btn mini danger"
                  title="删除"
                  onclick={(e) => {
                    e.stopPropagation();
                    remove(item);
                  }}
                >
                  <X size={11} />
                </button>
              </div>
            </div>
          {/each}
        </div>
        {#if hoverPreview}
          <div
            class="img-preview"
            style="top: {hoverPreview.top}px; height: {hoverPreview.height}px"
          >
            <img src="data:image/png;base64,{hoverPreview.src}" alt="预览" />
          </div>
        {/if}
      {/if}
    </section>

    <!-- 下方：文本历史（垂直列表，交互不变） -->
    <section class="list-section" bind:this={listSectionEl}>
      <div class="section-header">
        <span class="section-title">
          <ClipboardList size={12} />
          文本
        </span>
        <span class="section-count">{textItems.length} 条</span>
      </div>
      <main class="list">
        {#if error}
          <p class="empty error-msg">{error}</p>
        {:else if loading}
          <p class="empty">加载中…</p>
        {:else if textItems.length === 0}
          <p class="empty">
            {filter ? "没有匹配的文本" : "暂无文本历史\n复制文字试试"}
          </p>
        {:else}
          {#each textItems as item, i (item.id)}
            <div
              class="row {item.kind}"
              class:selected={i === selected}
              role="option"
              aria-selected={i === selected}
              tabindex="-1"
              onmouseenter={(e) => {
                selected = i;
                showTextPreview(item, e.currentTarget as HTMLElement);
              }}
              onmouseleave={hideTextPreview}
              onclick={() => paste(item.id)}
              onkeydown={(e) => {
                if (e.key === "Enter") paste(item.id);
              }}
            >
              <div class="meta">
                <span class="title">
                  {item.preview}
                </span>
                <span class="time">{timeLabel(item.created_at)}</span>
              </div>

              <div class="actions">
                <button
                  class="icon-btn {item.pinned ? 'active' : ''}"
                  title={item.pinned ? "取消固定" : "固定"}
                  onclick={(e) => {
                    e.stopPropagation();
                    togglePin(item);
                  }}
                >
                  {#if item.pinned}
                    <Pin size={13} fill="currentColor" />
                  {:else}
                    <PinOff size={13} />
                  {/if}
                </button>
                <button
                  class="icon-btn danger"
                  title="删除"
                  onclick={(e) => {
                    e.stopPropagation();
                    remove(item);
                  }}
                >
                  <X size={13} />
                </button>
              </div>
            </div>
          {/each}
        {/if}
      </main>

      {#if textPreview}
        <div
          class="text-preview"
          style="top: {textPreview.top}px; height: {textPreview.height}px"
        >
          <pre>{textPreview.text}</pre>
        </div>
      {/if}
    </section>
  </div>

  <!-- 底部提示（也可拖动窗口） -->
  <footer class="footer" data-tauri-drag-region="deep">
    <span>↑↓ 选择</span>
    <span>Enter 粘贴</span>
    <span>Esc 关闭</span>
    <span class="dot">•</span>
    <span>文本 {textItems.length} · 媒体 {topItems.length}</span>
  </footer>
</div>

<style>
  :global(:root) {
    color: var(--text);
    --bg: #131418;
    --bg-grad: #1a1c23;
    --bg-soft: #1e2129;
    --bg-hover: #272b36;
    --border: #2b2f3a;
    --border-strong: #3d4250;
    --text: #e9ebf2;
    --text-dim: #8b91a3;
    --accent: #4da3ff;
    --accent-soft: rgba(77, 163, 255, 0.13);
    --danger: #f07178;
    --star: #ffcf5c;
    --shadow: 0 16px 48px rgba(0, 0, 0, 0.55);
    --radius: 14px;
    font-family: "Segoe UI Variable Text", "Segoe UI", "PingFang SC",
      "Microsoft YaHei UI", "Microsoft YaHei", sans-serif;
  }

  :global(:root[data-theme="light"]) {
    --bg: #f2f3f7;
    --bg-grad: #fafbfd;
    --bg-soft: #ffffff;
    --bg-hover: #e7e9ef;
    --border: #d8dbe3;
    --border-strong: #c3c7d1;
    --text: #20242c;
    --text-dim: #6a7080;
    --accent: #2f6fd8;
    --accent-soft: rgba(47, 111, 216, 0.11);
    --danger: #d64550;
    --star: #b8860b;
    --shadow: 0 16px 48px rgba(30, 35, 50, 0.26);
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }

  @keyframes window-in {
    from {
      opacity: 0;
      transform: translateY(-8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: linear-gradient(180deg, var(--bg-grad), var(--bg) 40%);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    overflow: hidden;
    animation: window-in 160ms cubic-bezier(0.2, 0.8, 0.3, 1);
  }

  /* 顶栏 */
  .topbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px 8px;
  }
  .searchbox {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 9px;
    padding: 6px 10px;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .searchbox:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .search-icon {
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .searchbox input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13px;
    font-family: inherit;
  }
  .searchbox input::placeholder {
    color: var(--text-dim);
  }

  .icon-btn {
    border: none;
    background: transparent;
    color: var(--text-dim);
    width: 27px;
    height: 27px;
    border-radius: 7px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition: background 0.12s, color 0.12s;
  }
  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon-btn.active {
    color: var(--star);
  }
  .icon-btn.danger:hover {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }

  /* 设置面板 */
  .settings-panel {
    margin: 0 10px 8px;
    padding: 12px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .settings-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .settings-panel label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .settings-panel label.toggle {
    cursor: pointer;
    user-select: none;
    color: var(--text);
  }
  .settings-panel input[type="checkbox"] {
    accent-color: var(--accent);
    cursor: pointer;
  }
  .settings-panel input[type="number"],
  .settings-panel input.hotkey-input {
    width: 80px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 4px 8px;
    font-size: 12px;
    outline: none;
  }
  .settings-panel input.hotkey-input {
    width: 140px;
    font-family: Consolas, monospace;
  }
  .settings-panel input[type="number"]:focus,
  .settings-panel input.hotkey-input:focus {
    border-color: var(--accent);
  }
  .settings-panel select {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 3px 6px;
    font-size: 12px;
    outline: none;
  }
  .hotkey-row {
    margin-left: auto;
  }
  .settings-panel .settings-msg {
    color: var(--accent);
    margin-left: auto;
  }
  .save-btn {
    border: none;
    background: var(--accent);
    color: #10121a;
    font-size: 12px;
    font-weight: 600;
    padding: 5px 14px;
    border-radius: 6px;
    cursor: pointer;
  }
  .settings-hint {
    flex-basis: 100%;
    margin: 4px 0 0;
    line-height: 1.7;
  }
  .settings-hint code {
    background: var(--bg);
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 11px;
  }

  /* 内容区：上横向区 + 下文本区 */
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* 上方：图片/文件横向区（约占 20%） */
  .strip-section {
    height: 20%;
    min-height: 96px;
    display: flex;
    flex-direction: column;
    padding: 2px 10px 6px;
    position: relative;
  }
  .section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 2px 4px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .section-title {
    display: flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
    letter-spacing: 0.4px;
  }
  .section-count {
    font-size: 10px;
    opacity: 0.75;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 99px;
    padding: 0 6px;
    line-height: 14px;
  }
  .strip {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 8px;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .strip::-webkit-scrollbar {
    height: 4px;
  }
  .strip::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 2px;
  }
  .strip-item {
    position: relative;
    flex-shrink: 0;
    width: 92px;
    height: 100%;
    border-radius: 9px;
    overflow: hidden;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: border-color 0.15s, transform 0.15s, box-shadow 0.15s;
  }
  .strip-item:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
  }
  .strip-item img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .strip-placeholder {
    color: var(--text-dim);
    font-size: 11px;
  }
  .file-icon {
    color: var(--text-dim);
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  .file-name {
    max-width: 84px;
    padding: 2px 4px 0;
    font-size: 10px;
    color: var(--text);
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .strip-actions {
    position: absolute;
    top: 2px;
    right: 2px;
    display: flex;
    gap: 1px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .strip-item:hover .strip-actions {
    opacity: 1;
  }
  .icon-btn.mini {
    width: 18px;
    height: 18px;
    font-size: 11px;
    background: color-mix(in srgb, var(--bg) 62%, transparent);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .strip-empty {
    margin: 0;
    padding: 16px 0;
    text-align: center;
    color: var(--text-dim);
    font-size: 12px;
  }

  /* 下方：文本列表 */
  .list-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
    position: relative;
  }
  .list-section .section-header {
    padding: 6px 12px 4px;
  }

  /* 列表 */
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 2px 10px 8px;
    outline: none;
  }
  .list::-webkit-scrollbar {
    width: 6px;
  }
  .list::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 3px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    margin-bottom: 3px;
    border-radius: 9px;
    cursor: pointer;
    transition: background 0.12s;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.selected {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 60%, transparent);
  }

  .thumb-wrap {
    width: 56px;
    height: 40px;
    flex-shrink: 0;
    border-radius: 6px;
    overflow: hidden;
    background: var(--bg-soft);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .thumb-wrap img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb-placeholder {
    color: var(--text-dim);
    font-size: 11px;
  }

  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    color: var(--text);
    font-size: 13px;
    line-height: 1.5;
    /* 固定 3 行高度，保证条目整齐；超出折叠省略 */
    height: 58.5px;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
  }
  .time {
    color: var(--text-dim);
    font-size: 10.5px;
    font-family: "Cascadia Mono", Consolas, monospace;
    letter-spacing: 0.2px;
    opacity: 0.85;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .row:hover .actions,
  .row.selected .actions {
    opacity: 1;
  }
  .actions .icon-btn {
    width: 22px;
    height: 22px;
  }

  /* 图片大图预览浮层：高度由 JS 按可用空间计算，图片 contain 适配 */
  .img-preview {
    position: absolute;
    left: 12px;
    right: 12px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
    z-index: 10;
    pointer-events: none;
    display: flex;
  }
  .img-preview img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  /* 文本全文预览浮层 */
  .text-preview {
    position: absolute;
    left: 12px;
    right: 12px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
    z-index: 10;
    pointer-events: none;
    overflow: hidden;
  }
  .text-preview pre {
    margin: 0;
    padding: 10px 12px;
    height: 100%;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text);
  }

  .empty {
    text-align: center;
    color: var(--text-dim);
    font-size: 13px;
    padding: 48px 0;
    white-space: pre-line;
    line-height: 1.8;
  }
  .error-msg {
    color: var(--danger);
  }

  .footer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 14px;
    border-top: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 10.5px;
    letter-spacing: 0.3px;
  }
  .footer .dot {
    margin-left: auto;
    opacity: 0.5;
  }
</style>
