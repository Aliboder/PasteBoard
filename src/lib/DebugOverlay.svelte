<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  let { enabled = false }: { enabled?: boolean } = $props();
  let target = $state<HTMLElement | null>(null);
  let info = $state("");
  let mouseX = $state(0);
  let mouseY = $state(0);

  function onMove(e: MouseEvent) {
    if (!enabled) return;
    mouseX = e.clientX;
    mouseY = e.clientY;
    const el = e.target as HTMLElement;
    if (el === target) return;
    target = el;
    const rect = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    info = [
      `tag: ${el.tagName.toLowerCase()}`,
      el.id ? `id: ${el.id}` : "",
      el.dataset.debugId ? `debug-id: ${el.dataset.debugId}` : "",
      el.className && typeof el.className === "string"
        ? `class: ${el.className.split(/\s+/).filter(Boolean).slice(0, 3).join(" ")}`
        : "",
      `rect: ${Math.round(rect.x)},${Math.round(rect.y)} ${Math.round(rect.width)}×${Math.round(rect.height)}`,
      `display: ${cs.display}  visibility: ${cs.visibility}`,
      el.textContent && el.children.length === 0
        ? `text: "${(el.textContent || "").trim().slice(0, 40)}"`
        : "",
    ]
      .filter(Boolean)
      .join("\n");
  }

  onMount(() => {
    document.addEventListener("mousemove", onMove);
  });

  onDestroy(() => {
    document.removeEventListener("mousemove", onMove);
  });
</script>

{#if enabled}
  <div class="debug-overlay" role="presentation">
    {#if target}
      {@const rect = target.getBoundingClientRect()}
      <div
        class="debug-highlight"
        style="left:{rect.x}px;top:{rect.y}px;width:{rect.width}px;height:{rect.height}px"
      ></div>
      <div
        class="debug-tooltip"
        style="left:{Math.min(mouseX + 12, window.innerWidth - 220)}px;top:{Math.min(mouseY + 12, window.innerHeight - 140)}px"
      >
        <pre>{info}</pre>
      </div>
    {/if}
  </div>
{/if}

<style>
  .debug-overlay {
    position: fixed;
    inset: 0;
    z-index: 99999;
    pointer-events: none;
  }
  .debug-highlight {
    position: fixed;
    border: 2px solid #3b82f6;
    background: rgba(59, 130, 246, 0.08);
    border-radius: 2px;
    pointer-events: none;
    transition: all 0.05s ease-out;
  }
  .debug-tooltip {
    position: fixed;
    background: rgba(15, 15, 15, 0.94);
    border: 1px solid rgba(59, 130, 246, 0.5);
    border-radius: 6px;
    padding: 6px 9px;
    pointer-events: none;
    max-width: 220px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }
  .debug-tooltip pre {
    margin: 0;
    font-family: "Cascadia Code", "Fira Code", Consolas, monospace;
    font-size: 10.5px;
    line-height: 1.55;
    color: #d4d4d4;
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
