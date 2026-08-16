<script lang="ts">
  import { Pin, PinOff, X } from "lucide-svelte";
  import type { ItemDto } from "./api";
  import { splitHighlight, timeLabel } from "./utils";

  let {
    item,
    index,
    selected = false,
    flow = false,
    top = null,
    filter = "",
    confirmDeleteId = null,
    onSelect,
    onPaste,
    onContext,
    onTogglePin,
    onRemove,
  }: {
    item: ItemDto;
    index: number;
    selected?: boolean;
    flow?: boolean;
    top?: number | null;
    filter?: string;
    confirmDeleteId?: number | null;
    onSelect: (el: HTMLElement, item: ItemDto) => void;
    onPaste: (id: number) => void;
    onContext: (e: MouseEvent, item: ItemDto) => void;
    onTogglePin: (item: ItemDto) => void;
    onRemove: (item: ItemDto) => void;
  } = $props();
</script>

<div
  class="row {item.kind}"
  class:selected
  class:flow
  class:pinned={item.pinned}
  style={top !== null ? `top: ${top}px` : undefined}
  role="option"
  aria-selected={selected}
  tabindex="-1"
  onmouseenter={(e) => onSelect(e.currentTarget as HTMLElement, item)}
  onclick={() => onPaste(item.id)}
  oncontextmenu={(e) => onContext(e, item)}
  onkeydown={(e) => {
    if (e.key === "Enter") onPaste(item.id);
  }}
>
  <div class="meta">
    <span class="title">
      {#if filter}
        {#each splitHighlight(item.preview, filter) as part, pi (pi)}
          {#if part.m}
            <mark class="hl">{part.t}</mark>
          {:else}
            {part.t}
          {/if}
        {/each}
      {:else}
        {item.preview}
      {/if}
    </span>
    <span class="time">{timeLabel(item.created_at)}</span>
  </div>

  <div class="actions">
    <button
      class="icon-btn {item.pinned ? 'active' : ''}"
      title={item.pinned ? "取消固定" : "固定"}
      onclick={(e) => {
        e.stopPropagation();
        onTogglePin(item);
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
        <X size={13} />
      {/if}
    </button>
  </div>
</div>

<style>
  .row {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 9px;
    border: 1px solid var(--border-strong);
    cursor: pointer;
    transition: background 0.12s, border-color 0.12s;
  }
  /* 全量渲染：正常文档流 */
  .row.flow {
    position: static;
    margin-bottom: 8px;
  }
  .row:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
  .row.selected {
    background: var(--accent-soft);
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .row.pinned {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }
  .row.pinned::before {
    content: "";
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 2px;
    background: var(--accent);
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
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
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

  .hl {
    background: color-mix(in srgb, var(--accent) 28%, transparent);
    color: var(--accent);
    border-radius: 3px;
    padding: 0 1px;
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
  .icon-btn.danger:hover {
    background: color-mix(in srgb, var(--danger) 15%, transparent);
    color: var(--danger);
  }
  .icon-btn.confirm {
    background: var(--danger);
    color: #fff;
    width: auto;
    padding: 0 6px;
    border-radius: 6px;
  }
  .confirm-txt {
    font-size: 10px;
    font-weight: 700;
    white-space: nowrap;
  }
</style>
