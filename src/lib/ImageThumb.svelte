<script lang="ts">
  import { onMount } from "svelte";
  import { Image as ImageIcon } from "lucide-svelte";
  import { getThumb } from "./api";
  import { throttled } from "./thumbQueue";

  let { id, alt = "图片" }: { id: number; alt?: string } = $props();

  /** 页面级缓存：同一图片只请求一次（图片条目数有上限，缓存无碍） */
  const thumbCache = new Map<number, string>();

  let src = $state<string | null>(null);
  let rootEl: HTMLElement | undefined = $state();
  let loaded = $state(false);

  async function load() {
    if (loaded) return;
    loaded = true;
    let s = thumbCache.get(id);
    if (s === undefined) {
      const fetched = await throttled(() => getThumb(id));
      if (fetched) {
        thumbCache.set(id, fetched);
        s = fetched;
      }
    }
    src = s ?? null;
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

<div bind:this={rootEl} class="thumb-wrap">
  {#if src}
    <img src="data:image/png;base64,{src}" alt={alt} draggable="false" />
  {:else}
    <span class="thumb-placeholder">
      <ImageIcon size={16} />
    </span>
  {/if}
</div>

<style>
  .thumb-wrap {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
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
