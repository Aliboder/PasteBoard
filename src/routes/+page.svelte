<script lang="ts">
  import { onMount } from "svelte";
  import {
    getHistory,
    pinItem,
    deleteItem,
    clearHistory,
    onChange,
    type ItemDto,
  } from "../lib/api";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let items: ItemDto[] = $state([]);
  let filter = $state("");
  let selected = $state(-1);
  let loading = $state(true);

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

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (items.length) selected = (selected + 1) % items.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length) selected = selected <= 0 ? items.length - 1 : selected - 1;
    } else if (e.key === "Escape") {
      getCurrentWindow().hide();
    }
    // Enter → 粘贴（阶段 5 接入）
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
  </header>

  <!-- 列表 -->
  <main class="list" onkeydown={onKeydown} tabindex="-1">
    {#if loading}
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
          onmouseenter={() => (selected = i)}
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
              <span class="time">{relativeTime(item.created_at)}</span>
            </div>
          {:else}
            <div class="meta">
              <span class="title">
                {item.kind === "files" ? fileLabel(item) : item.preview}
              </span>
              <span class="time">
                {relativeTime(item.created_at)}
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

  /* 列表 */
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 10px 8px;
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

  .empty {
    text-align: center;
    color: var(--text-dim);
    font-size: 13px;
    padding: 48px 0;
    white-space: pre-line;
    line-height: 1.8;
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
