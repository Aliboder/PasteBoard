<script lang="ts">
  import { onMount } from "svelte";
  import { getImage, getFilePreview } from "../../lib/api";
  import { getCurrentWindow, LogicalSize, currentMonitor } from "@tauri-apps/api/window";

  let src = $state<string | null>(null);
  let failed = $state(false);

  /** 解析父窗口传入的参数：kind=image → 按 id 读原图；kind=file → 按路径读文件预览 */
  onMount(async () => {
    const params = new URLSearchParams(window.location.search);
    const win = getCurrentWindow();
    try {
      const b64 =
        params.get("kind") === "file"
          ? await getFilePreview(params.get("path") ?? "")
          : await getImage(Number(params.get("id")));
      if (b64) {
        src = `data:image/png;base64,${b64}`;
        await fitWindow();
      } else {
        failed = true;
      }
    } catch {
      failed = true;
    }
    // 图片与窗口尺寸就绪后再显示，避免先闪默认尺寸窗口
    await win.show();
    await win.setFocus();
  });

  /** 窗口尺寸自适应图片：按宽高比等比缩放（窗口比例 = 图片比例，无黑边），不小于最小窗口、不超过最大上限 */
  async function fitWindow() {
    const win = getCurrentWindow();
    const monitor = await currentMonitor();
    // 屏幕可用逻辑尺寸（monitor.size 为物理像素，换算到逻辑）
    const screen =
      monitor && monitor.size.width > 0
        ? monitor.size.toLogical(monitor.scaleFactor)
        : { width: 1920, height: 1080 };
    // 解码图片真实宽高
    const dims = await new Promise<{ w: number; h: number }>((resolve) => {
      const img = new Image();
      img.onload = () => resolve({ w: img.naturalWidth, h: img.naturalHeight });
      img.onerror = () => resolve({ w: 0, h: 0 });
      img.src = src ?? "";
    });
    if (dims.w === 0 || dims.h === 0) return;
    const MIN_W = 480;
    const MIN_H = 360;
    const MAX_W = 1280;
    const MAX_H = 800;
    const maxW = Math.floor(screen.width * 0.9);
    const maxH = Math.floor(screen.height * 0.9);
    // 等比缩放系数：同时受最大上限与屏幕上限约束；小图再按最小窗口放大
    const scaleDown = Math.min(MAX_W / dims.w, MAX_H / dims.h, maxW / dims.w, maxH / dims.h);
    let scale = Math.min(scaleDown, 1);
    if (dims.w * scale < MIN_W || dims.h * scale < MIN_H) {
      scale = Math.max(MIN_W / dims.w, MIN_H / dims.h);
    }
    await win.setSize(new LogicalSize(Math.round(dims.w * scale), Math.round(dims.h * scale)));
    await win.center();
  }

  function close() {
    getCurrentWindow().close();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div
  class="viewer"
  role="presentation"
  onclick={close}
  oncontextmenu={(e) => {
    e.preventDefault();
    close();
  }}
>
  {#if failed}
    <p class="msg">大图加载失败</p>
  {:else if src}
    <img src={src} alt="大图" draggable="false" />
  {:else}
    <p class="msg">加载中…</p>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: #101010;
    overflow: hidden;
  }
  .viewer {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #101010;
    cursor: zoom-out;
  }
  img {
    max-width: 100vw;
    max-height: 100vh;
    object-fit: contain;
  }
  .msg {
    color: #8a8a8a;
    font-size: 14px;
    letter-spacing: 0.5px;
  }
</style>
