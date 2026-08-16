<script lang="ts">
  import { onMount } from "svelte";
  import { Image as ImageIcon } from "lucide-svelte";
  import { getThumb } from "./api";

  let { id, alt = "图片" }: { id: number; alt?: string } = $props();

  /** 页面级缓存：同一图片只请求一次（图片条目数有上限，缓存无碍） */
  const thumbCache = new Map<number, string>();

  let src = $state<string | null>(null);

  onMount(async () => {
    let s = thumbCache.get(id);
    if (s === undefined) {
      const fetched = await getThumb(id);
      if (fetched) {
        thumbCache.set(id, fetched);
        s = fetched;
      }
    }
    src = s ?? null;
  });
</script>

{#if src}
  <img src={src} alt={alt} draggable="false" />
{:else}
  <span class="thumb-placeholder">
    <ImageIcon size={16} />
  </span>
{/if}

<style>
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb-placeholder {
    color: var(--text-dim);
    font-size: 11px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
  }
</style>
