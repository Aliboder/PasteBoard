<script lang="ts">
  let version = $state("0.1.0");
</script>

<main class="app">
  <h1>PasteBoard</h1>
  <p>剪贴板管理器骨架已就绪（v{version}）</p>
  <p class="hint">全局热键 Ctrl+Shift+V 可切换本窗口</p>
</main>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
    color: #f0f0f0;
    background: #1e1f24;
    user-select: none;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    letter-spacing: 1px;
  }
  p {
    margin: 0;
    font-size: 14px;
    color: #9a9ba3;
  }
  .hint {
    margin-top: 12px;
    font-size: 12px;
    color: #5f6168;
  }
</style>
