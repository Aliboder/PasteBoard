<script lang="ts">
  import { onMount } from "svelte";
  import { Folder, Image as ImageIcon, Pin, PinOff, X } from "lucide-svelte";
  import type { ItemDto } from "./api";
  import { timeLabel } from "./utils";
  import FileTile from "./FileTile.svelte";
  import ImageThumb from "./ImageThumb.svelte";

  let {
    items,
    kind,
    gridSelected = -1,
    confirmDeleteId = null,
    bindGridEl,
    onSelect,
    onPaste,
    onContext,
    onTogglePin,
    onRemove,
  }: {
    items: ItemDto[];
    kind: "image" | "files";
    gridSelected?: number;
    confirmDeleteId?: number | null;
    bindGridEl: (el: HTMLElement | undefined) => void;
    onSelect: (gi: number) => void;
    onPaste: (id: number) => void;
    onContext: (e: MouseEvent, item: ItemDto) => void;
    onTogglePin: (item: ItemDto) => void;
    onRemove: (item: ItemDto) => void;
  } = $props();

  let gridEl: HTMLElement | undefined = $state();

  /** 网格容器引用同步给父组件（列数计算与滚动跟随用）；卸载时通知父清空，避免残留已销毁的 DOM 引用 */
  $effect(() => {
    bindGridEl(gridEl);
    return () => bindGridEl(undefined);
  });

  /** 列数：读计算样式（图片/文件 Tab 均自适应） */
  function gridCols(): number {
    if (!gridEl) return 1;
    return getComputedStyle(gridEl).gridTemplateColumns.split(" ").filter(Boolean).length || 1;
  }

  /** 把行轨道高度绑定为实际列宽（grid-auto-rows），
   *  防止容器高度不足时 grid 将 auto 行压缩到 min-content 导致卡片上下重叠 */
  function updateCellSize() {
    if (!gridEl) return;
    const cols = gridCols();
    const gap = 8;
    const cell = (gridEl.clientWidth - (cols - 1) * gap) / cols;
    gridEl.style.gridAutoRows = `${Math.max(48, Math.floor(cell))}px`;
  }

  /** 容器宽度变化（窗口缩放/列数变化）时重算卡片尺寸 */
  onMount(() => {
    updateCellSize();
    const observer = new ResizeObserver(updateCellSize);
    if (gridEl) observer.observe(gridEl);
    return () => observer.disconnect();
  });

  /** 横向卡片显示名：取文件名（多文件时加数量） */
  function fileName(item: ItemDto): string {
    const base = item.preview.split(/[\\/]/).pop() ?? item.preview;
    return item.file_count > 1 ? `${base} 等${item.file_count}个` : base;
  }
</script>

<section class="grid-section">
  <div class="section-header">
    <span class="section-title">
      {#if kind === "image"}
        <ImageIcon size={12} />
        图片
      {:else}
        <Folder size={12} />
        文件
      {/if}
    </span>
    <span class="section-count">{items.length} 条</span>
  </div>
  {#if items.length === 0}
    <p class="strip-empty">{kind === "image" ? "暂无图片历史" : "暂无文件历史"}</p>
  {:else}
    <div class="grid" bind:this={gridEl}>
      {#each items as item, gi (item.id)}
        <div
          class="grid-item"
          class:pinned={item.pinned}
          class:selected={gi === gridSelected}
          role="option"
          aria-selected={gi === gridSelected}
          tabindex="-1"
          title={item.kind === "image"
            ? `图片 · ${timeLabel(item.created_at)}`
            : `${item.preview} · ${timeLabel(item.created_at)}`}
          onmouseenter={() => onSelect(gi)}
          onclick={() => onPaste(item.id)}
          oncontextmenu={(e) => onContext(e, item)}
          onkeydown={(e) => {
            if (e.key === "Enter") onPaste(item.id);
          }}
        >
          {#if item.kind === "image"}
            <ImageThumb id={item.id} />
          {:else}
            <!-- 复制的图片文件：图标/缩略图 + 文件名（竖排） -->
            <div class="grid-file">
              <FileTile path={item.preview} name={fileName(item)} />
            </div>
          {/if}
          <span class="grid-time">{timeLabel(item.created_at)}</span>
          <div class="grid-actions">
            <button
              class="icon-btn mini {item.pinned ? 'active' : ''}"
              title={item.pinned ? "取消固定" : "固定"}
              onclick={(e) => {
                e.stopPropagation();
                onTogglePin(item);
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
              class:confirm={confirmDeleteId === item.id}
              title={confirmDeleteId === item.id ? "再点一次确认删除" : "删除"}
              onclick={(e) => {
                e.stopPropagation();
                onRemove(item);
              }}
            >
              {#if confirmDeleteId === item.id}
                <span class="confirm-txt">确认</span>
              {:else}
                <X size={11} />
              {/if}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .grid-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 2px 10px 6px;
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
    opacity: 0.8;
  }
  .strip-empty {
    margin: 0;
    padding: 16px 0;
    text-align: center;
    color: var(--text-dim);
    font-size: 12px;
  }

  .grid {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    gap: 8px;
    align-content: start;
    padding: 2px 0 8px;
  }
  .grid::-webkit-scrollbar {
    width: 6px;
  }
  .grid::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 3px;
  }
  .grid-item {
    position: relative;
    /* 高度由行轨道决定（grid-auto-rows 由 JS 绑定为列宽，见 updateCellSize） */
    aspect-ratio: 1;
    border-radius: 9px;
    overflow: hidden;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color 0.12s, transform 0.12s, box-shadow 0.12s;
  }
  .grid-item:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
  }
  .grid-item.pinned {
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .grid-item.selected {
    box-shadow: inset 0 0 0 2px var(--accent);
    border-color: var(--accent);
  }
  .grid-file {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 4px;
  }
  .grid-time {
    position: absolute;
    right: 3px;
    top: 3px;
    font-size: 8.5px;
    line-height: 1.3;
    color: #fff;
    background: rgba(0, 0, 0, 0.45);
    padding: 2px 4px;
    border-radius: 4px;
    font-family: "Cascadia Mono", Consolas, monospace;
    pointer-events: none;
    transition: opacity 0.12s;
    /* 自适应换行：宽度不足时在"日期 时间"之间的空格处自然断行，不截断 */
    text-align: left;
    white-space: normal;
    max-width: calc(100% - 6px);
  }
  .grid-item:hover .grid-time {
    opacity: 0;
  }
  .grid-actions {
    position: absolute;
    top: 4px;
    right: 4px;
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .grid-item:hover .grid-actions {
    opacity: 1;
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
    color: var(--accent);
  }
  .icon-btn.mini {
    width: 18px;
    height: 18px;
    font-size: 11px;
    background: color-mix(in srgb, var(--bg) 62%, transparent);
    border: 1px solid var(--border);
  }
  .icon-btn.danger:hover {
    background: color-mix(in srgb, var(--danger) 15%, transparent);
    color: var(--danger);
  }
  .icon-btn.confirm {
    background: var(--danger);
    color: #fff;
    width: auto;
    padding: 0 5px;
    border-radius: 6px;
  }
  .confirm-txt {
    font-size: 10px;
    font-weight: 700;
    white-space: nowrap;
  }
</style>
