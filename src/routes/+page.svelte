<script lang="ts">
  import { onMount } from "svelte";
  import {
    getHistory,
    pinItem,
    deleteItem,
    clearHistory,
    pasteItem,
    getImage,
    getSettings,
    setMaxItems,
    onChange,
    type ItemDto,
    type SettingsDto,
  } from "../lib/api";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let items: ItemDto[] = $state([]);
  let filter = $state("");
  let selected = $state(-1);
  let loading = $state(true);
  let error = $state("");
  let showSettings = $state(false);
  let settings = $state<SettingsDto | null>(null);
  let maxItemsInput = $state("500");
  let settingsMsg = $state("");
  let hoverPreview = $state<{ src: string; top: number } | null>(null);
  const previewCache = new Map<number, string>();
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;

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
  }

  async function saveSettings() {
    const n = parseInt(maxItemsInput, 10);
    if (isNaN(n) || n < 1) {
      settingsMsg = "请输入大于 0 的数字";
      return;
    }
    await setMaxItems(n);
    settingsMsg = "已保存";
  }

  async function reload() {
    loading = true;
    items = await getHistory(filter, 500, 0);
    if (selected >= items.length) selected = -1;
    loading = false;
  }

  function relativeTime(ms: number): string {
    const diff = Date.now() - ms;
    const min = Math.floor(diff / 60000);
    if (min < 1) return "刚刚";
    if (min < 60) return `${min} 分钟前`;
    const hour = Math.floor(min / 60);
    if (hour < 24) return `${hour} 小时前`;
    const day = Math.floor(hour / 24);
    return `${day} 天前`;
  }

  function fileLabel(item: ItemDto): string {
    if (item.file_count <= 1) return item.preview;
    return `${item.preview} 等 ${item.file_count} 个文件`;
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
      await getCurrentWindow().hide();
    } catch (e) {
      error = String(e);
    }
  }

  /** 图片悬停大图预览（350ms 延迟，原图懒加载并缓存） */
  async function showPreview(item: ItemDto, rowEl: HTMLElement) {
    if (item.kind !== "image") return;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = setTimeout(async () => {
      let src = previewCache.get(item.id);
      if (!src) {
        src = (await getImage(item.id)) ?? "";
        if (src) previewCache.set(item.id, src);
      }
      if (!src) return;
      const maxH = 300;
      let top = rowEl.offsetTop - maxH - 8;
      if (top < 8) top = rowEl.offsetTop + rowEl.offsetHeight + 8;
      hoverPreview = { src, top };
    }, 350);
  }

  function hidePreview() {
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverPreview = null;
  }

  /** 全局键盘：↑↓ 选择、Enter 粘贴、Esc 关闭、数字 1~9 快捷粘贴、Delete 删除 */
  function globalKeydown(e: KeyboardEvent) {
    const inInput = document.activeElement?.tagName === "INPUT";
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (items.length) selected = (selected + 1) % items.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length) selected = selected <= 0 ? items.length - 1 : selected - 1;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (selected >= 0 && items[selected]) paste(items[selected].id);
    } else if (e.key === "Escape") {
      getCurrentWindow().hide();
    } else if (e.key === "Delete" && !inInput) {
      if (selected >= 0 && items[selected]) remove(items[selected]);
    } else if (/^[1-9]$/.test(e.key) && !inInput) {
      const idx = parseInt(e.key, 10) - 1;
      if (items[idx]) paste(items[idx].id);
    }
  }

  onMount(() => {
    reload();
    const unlisten = onChange(() => reload());
    return () => {
      unlisten();
    };
  });
</script>

<svelte:head>
  <title>PasteBoard</title>
</svelte:head>

<svelte:window onkeydown={globalKeydown} />

<div class="window">
  <!-- 顶栏：拖动区域 + 搜索 + 清空 -->
  <header class="topbar" data-tauri-drag-region>
    <div class="searchbox">
      <span class="search-icon">⌕</span>
      <input
        placeholder="搜索剪贴板历史…"
        bind:value={filter}
        oninput={reload}
        spellcheck="false"
      />
      {#if filter}
        <button class="icon-btn" title="清除" onclick={() => (filter = "")}>×</button>
      {/if}
    </div>
    <button class="icon-btn clear-btn" title="清空历史" onclick={clearAll}>🗑</button>
    <button
      class="icon-btn {showSettings ? 'active' : ''}"
      title="设置"
      onclick={() => (showSettings ? (showSettings = false) : openSettings())}
    >⚙</button>
  </header>

  {#if showSettings}
    <section class="settings-panel">
      <label>
        历史上限（条）
        <input
          type="number"
          min="1"
          max="100000"
          bind:value={maxItemsInput}
          onkeydown={(e) => {
            if (e.key === "Enter") saveSettings();
          }}
        />
      </label>
      <button class="save-btn" onclick={saveSettings}>保存</button>
      <span class="settings-msg">{settingsMsg}</span>
      <p class="settings-hint">
        全局快捷键：<code>Ctrl+Shift+V</code>（暂不可自定义）<br />
        数据目录：<code>%APPDATA%\com.aliboder.pasteboard</code>
      </p>
    </section>
  {/if}

  <!-- 列表 -->
  <main class="list">
    {#if error}
      <p class="empty error-msg">{error}</p>
    {:else if loading}
      <p class="empty">加载中…</p>
    {:else if items.length === 0}
      <p class="empty">
        {filter ? "没有匹配的结果" : "暂无剪贴板历史\n复制任意内容试试"}
      </p>
    {:else}
      {#each items as item, i (item.id)}
        <div
          class="row {item.kind}"
          class:selected={i === selected}
          role="option"
          aria-selected={i === selected}
          tabindex="-1"
          onmouseenter={(e) => {
            selected = i;
            showPreview(item, e.currentTarget as HTMLElement);
          }}
          onmouseleave={hidePreview}
          onclick={() => paste(item.id)}
          onkeydown={(e) => {
            if (e.key === "Enter") paste(item.id);
          }}
        >
          {#if item.kind === "image"}
            <div class="thumb-wrap">
              {#if item.thumb}
                <img src="data:image/png;base64,{item.thumb}" alt="缩略图" />
              {:else}
                <span class="thumb-placeholder">图片</span>
              {/if}
            </div>
            <div class="meta">
              <span class="title">图片</span>
              <span class="time">{timeLabel(item.created_at)}</span>
            </div>
          {:else}
            <div class="meta">
              <span class="title">
                {item.kind === "files" ? fileLabel(item) : item.preview}
              </span>
              <span class="time">
                {timeLabel(item.created_at)}
                {#if item.kind === "files" && item.file_count > 1}
                  · {item.file_count} 个文件
                {/if}
              </span>
            </div>
          {/if}

          <div class="actions">
            <button
              class="icon-btn {item.pinned ? 'active' : ''}"
              title={item.pinned ? "取消固定" : "固定"}
              onclick={() => togglePin(item)}
            >★</button>
            <button class="icon-btn danger" title="删除" onclick={() => remove(item)}>×</button>
          </div>
        </div>
      {/each}

      {#if hoverPreview}
        <div class="img-preview" style="top: {hoverPreview.top}px">
          <img src="data:image/png;base64,{hoverPreview.src}" alt="预览" />
        </div>
      {/if}
    {/if}
  </main>

  <!-- 底部提示 -->
  <footer class="footer">
    <span>↑↓ 选择</span>
    <span>Enter 粘贴</span>
    <span>Esc 关闭</span>
    <span class="dot">•</span>
    <span>共 {items.length} 条</span>
  </footer>
</div>

<style>
  :global(:root) {
    --bg: #1c1d22;
    --bg-soft: #26272e;
    --bg-hover: #2e2f38;
    --border: #33343d;
    --text: #e8e8ee;
    --text-dim: #9a9ba6;
    --accent: #6ea8fe;
    --danger: #e06c75;
    --radius: 12px;
    font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif;
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }

  .window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
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
    border-radius: 8px;
    padding: 6px 10px;
  }
  .searchbox:focus-within {
    border-color: var(--accent);
  }
  .search-icon {
    color: var(--text-dim);
    font-size: 15px;
  }
  .searchbox input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13px;
  }
  .searchbox input::placeholder {
    color: var(--text-dim);
  }

  .icon-btn {
    border: none;
    background: transparent;
    color: var(--text-dim);
    font-size: 14px;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon-btn.active {
    color: #ffd166;
  }
  .icon-btn.danger:hover {
    color: var(--danger);
  }

  /* 设置面板 */
  .settings-panel {
    margin: 0 10px 8px;
    padding: 12px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 12px;
    color: var(--text-dim);
  }
  .settings-panel label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .settings-panel input[type="number"] {
    width: 80px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 4px 8px;
    font-size: 12px;
    outline: none;
  }
  .settings-panel input[type="number"]:focus {
    border-color: var(--accent);
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
  .settings-msg {
    color: var(--accent);
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

  /* 列表 */
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 10px 8px;
    outline: none;
    position: relative;
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
    margin-bottom: 4px;
    border-radius: 8px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.selected {
    background: var(--bg-hover);
    box-shadow: inset 0 0 0 1px var(--accent);
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
    gap: 2px;
  }
  .title {
    color: var(--text);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .time {
    color: var(--text-dim);
    font-size: 11px;
  }

  .actions {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .row:hover .actions,
  .row.selected .actions {
    opacity: 1;
  }

  /* 图片大图预览浮层 */
  .img-preview {
    position: absolute;
    left: 12px;
    right: 12px;
    max-height: 300px;
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
    max-height: 300px;
    object-fit: contain;
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
    font-size: 11px;
  }
  .footer .dot {
    margin-left: auto;
  }
</style>
