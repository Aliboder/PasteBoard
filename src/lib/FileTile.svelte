<script lang="ts">
  import { onMount } from "svelte";
  import { getFileIcon, getFileThumb } from "./api";
  import { throttled } from "./thumbQueue";
  import { FileText } from "lucide-svelte";

  let { path, name }: { path: string; name: string } = $props();

  /** 可直接预览的图片扩展名 */
  const IMAGE_EXTS = new Set([
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "avif", "tif", "tiff",
  ]);

  /** 按文件路径缓存（缩略图或图标），避免重复请求 */
  const mediaCache = new Map<string, { kind: "thumb" | "icon"; src: string }>();

  let kind = $state<"thumb" | "icon" | "none">("none");
  let src = $state("");
  let rootEl: HTMLElement | undefined = $state();
  let loaded = $state(false);

  async function load() {
    if (loaded) return;
    loaded = true;
    const cached = mediaCache.get(path);
    if (cached) {
      kind = cached.kind;
      src = cached.src;
      return;
    }
    const ext = path.split(".").pop()?.toLowerCase() ?? "";
    if (IMAGE_EXTS.has(ext)) {
      const thumb = await throttled(() => getFileThumb(path));
      if (thumb) {
        kind = "thumb";
        src = thumb;
        mediaCache.set(path, { kind, src });
        return;
      }
    }
    const icon = await throttled(() => getFileIcon(path));
    if (icon) {
      kind = "icon";
      src = icon;
      mediaCache.set(path, { kind, src });
    }
  }

  /** 进入视口才加载（网格卡片多时避免一次性请求风暴） */
  onMount(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          observer.disconnect();
          load();
        }
      },
      { rootMargin: "120px" }
    );
    if (rootEl) observer.observe(rootEl);
    return () => observer.disconnect();
  });
</script>

<div bind:this={rootEl} class="file-tile">
  {#if kind === "thumb"}
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
</div>

<style>
  .file-tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
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
