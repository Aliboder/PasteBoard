<script lang="ts">
  import { onMount } from "svelte";
  import { getFileIcon, getFileThumb } from "./api";
  import { FileText } from "lucide-svelte";

  let { path, name, horizontal = false }: { path: string; name: string; horizontal?: boolean } =
    $props();

  /** 可直接预览的图片扩展名 */
  const IMAGE_EXTS = new Set([
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "avif", "tif", "tiff",
  ]);

  /** 按文件路径缓存（缩略图或图标），避免重复请求 */
  const mediaCache = new Map<string, { kind: "thumb" | "icon"; src: string }>();

  let kind = $state<"thumb" | "icon" | "none">("none");
  let src = $state("");

  onMount(async () => {
    const cached = mediaCache.get(path);
    if (cached) {
      kind = cached.kind;
      src = cached.src;
      return;
    }
    const ext = path.split(".").pop()?.toLowerCase() ?? "";
    if (IMAGE_EXTS.has(ext)) {
      const thumb = await getFileThumb(path);
      if (thumb) {
        kind = "thumb";
        src = thumb;
        mediaCache.set(path, { kind, src });
        return;
      }
    }
    const icon = await getFileIcon(path);
    if (icon) {
      kind = "icon";
      src = icon;
      mediaCache.set(path, { kind, src });
    }
  });
</script>

{#if horizontal}
  {#if kind === "thumb"}
    <img class="h-thumb" src="data:image/png;base64,{src}" alt={name} draggable="false" />
  {:else if kind === "icon"}
    <img class="h-icon" src="data:image/png;base64,{src}" alt={name} draggable="false" />
  {:else}
    <span class="h-fallback">
      <FileText size={20} />
    </span>
  {/if}
  <span class="h-name">{name}</span>
{:else if kind === "thumb"}
  <img class="tile-thumb" src="data:image/png;base64,{src}" alt={name} draggable="false" />
{:else if kind === "icon"}
  <img class="tile-icon" src="data:image/png;base64,{src}" alt={name} draggable="false" />
  <span class="file-name">{name}</span>
{:else}
  <span class="file-icon">
    <FileText size={18} />
  </span>
  <span class="file-name">{name}</span>
{/if}

<style>
  .h-thumb {
    width: 40px;
    height: 40px;
    border-radius: 6px;
    object-fit: cover;
    flex-shrink: 0;
  }
  .h-icon {
    width: 32px;
    height: 32px;
    flex-shrink: 0;
  }
  .h-fallback {
    width: 32px;
    height: 32px;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .h-name {
    flex: 1;
    min-width: 0;
    font-size: 12.5px;
    color: var(--text);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tile-thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .tile-icon {
    width: 32px;
    height: 32px;
    margin-bottom: 2px;
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
</style>
