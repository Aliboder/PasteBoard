<script lang="ts">
  import {
    Keyboard,
    History,
    Palette,
    Sliders,
    Wrench,
  } from "lucide-svelte";
  import {
    getSettings,
    setMaxItems,
    setTheme,
    setHotkey,
    setToggle,
    setAutostart,
    openDataDir,
    getStats,
    resetSettings,
    clearHistory,
    clearAllHistory,
    type SettingsDto,
    type StatsDto,
  } from "./api";

  let {
    settings,
    onApplyTheme,
    onCleared,
  }: {
    settings: SettingsDto | null;
    onApplyTheme: (theme: string) => void;
    onCleared: () => void;
  } = $props();

  let maxItemsInput = $state("500");
  let themeSel = $state("dark");
  let currentHotkey = $state("Ctrl+Shift+V");
  let hotkeyCapture = $state(false);
  let hotkeyDraft = $state("");
  let settingsMsg = $state("");
  let stats = $state<StatsDto | null>(null);
  let clearMenuOpen = $state(false);
  let confirmClearAll = $state(false);
  let captureBoxEl: HTMLElement | undefined = $state();

  /** 打开面板时由父组件触发：同步设置与统计 */
  $effect(() => {
    if (settings) {
      maxItemsInput = String(settings.max_items);
      themeSel = settings.theme;
      currentHotkey = settings.hotkey;
    }
  });
  $effect(() => {
    getStats().then((s) => (stats = s));
  });

  /** 进入录制模式时聚焦录制框 */
  $effect(() => {
    if (hotkeyCapture) captureBoxEl?.focus();
  });

  async function saveSettings() {
    const n = parseInt(maxItemsInput, 10);
    if (isNaN(n) || n < 1) {
      settingsMsg = "请输入大于 0 的数字";
      return;
    }
    await setMaxItems(n);
    await setTheme(themeSel);
    onApplyTheme(themeSel);
    settingsMsg = "已保存";
  }

  /** 应用新热键（录制后自动调用） */
  async function applyHotkey(combo: string) {
    settingsMsg = "";
    try {
      await setHotkey(combo);
      currentHotkey = combo;
      hotkeyCapture = false;
      settingsMsg = `热键已生效：${combo}`;
    } catch (e) {
      settingsMsg = typeof e === "string" ? e : (e as { message?: string })?.message ?? "快捷键设置失败";
    }
  }

  /** 把 e.code 映射为可识别的键名（不支持的键返回 null） */
  function mapKey(code: string): string | null {
    if (/^Key[A-Z]$/.test(code)) return code.slice(3);
    if (/^Digit[0-9]$/.test(code)) return code.slice(5);
    if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
    const map: Record<string, string> = {
      Space: "Space",
      Enter: "Enter",
      Tab: "Tab",
      Backspace: "Backspace",
      Delete: "Delete",
      Home: "Home",
      End: "End",
      PageUp: "PageUp",
      PageDown: "PageDown",
      Insert: "Insert",
      ArrowUp: "Up",
      ArrowDown: "Down",
      ArrowLeft: "Left",
      ArrowRight: "Right",
    };
    return map[code] ?? null;
  }

  /** 按键录制：捕获组合键并自动应用 */
  function onHotkeyKeydown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      hotkeyCapture = false;
      return;
    }
    if (e.repeat) return;
    const mods: string[] = [];
    if (e.ctrlKey) mods.push("Ctrl");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (e.metaKey) mods.push("Super");
    const key = mapKey(e.code);
    if (!key) return; // 忽略不可映射键
    if (mods.length === 0) {
      settingsMsg = "快捷键需至少包含一个修饰键（Ctrl / Alt / Shift / Win）";
      return;
    }
    hotkeyDraft = [...mods, key].join("+");
    applyHotkey(hotkeyDraft);
  }

  async function toggleSetting(key: string, enabled: boolean) {
    settingsMsg = "";
    try {
      await setToggle(key, enabled ? "on" : "off");
      if (settings) settings[key as "follow_mouse" | "keep_open" | "always_on_top"] = enabled ? "on" : "off";
      settingsMsg = "已保存";
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  async function toggleAutostart(enabled: boolean) {
    settingsMsg = "";
    try {
      await setAutostart(enabled);
      if (settings) settings.autostart = enabled;
      settingsMsg = enabled ? "已开启开机自启" : "已关闭开机自启";
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  async function openDataDirectory() {
    settingsMsg = "";
    try {
      await openDataDir();
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  function fmtSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
  }

  async function doReset() {
    settingsMsg = "";
    try {
      await resetSettings();
      onCleared();
      settingsMsg = "已恢复默认设置";
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  /** 清空历史（二级菜单） */
  async function clearUnpinned() {
    clearMenuOpen = false;
    const n = await clearHistory();
    onCleared();
    settingsMsg = `已清空 ${n} 条非固定历史`;
  }

  /** 清空全部（含固定）：需二次确认 */
  async function clearAllItems() {
    if (!confirmClearAll) {
      confirmClearAll = true;
      settingsMsg = "⚠️ 再点一次确认清空全部（含固定）";
      setTimeout(() => (confirmClearAll = false), 3000);
      return;
    }
    confirmClearAll = false;
    clearMenuOpen = false;
    const n = await clearAllHistory();
    onCleared();
    settingsMsg = `已清空全部 ${n} 条历史`;
  }
</script>

<section class="settings-panel">
  <!-- 外观 -->
  <div class="sp-section">
    <div class="sp-title">
      <Palette size={12} />
      外观
    </div>
    <div class="sp-row">
      <span class="sp-label">主题</span>
      <select bind:value={themeSel}>
        <option value="dark">深色</option>
        <option value="light">浅色</option>
        <option value="system">跟随系统</option>
      </select>
      <button class="btn small primary" onclick={saveSettings}>保存</button>
    </div>
  </div>

  <!-- 行为 -->
  <div class="sp-section">
    <div class="sp-title">
      <Sliders size={12} />
      行为
    </div>
    <label class="switch-row">
      <span>唤起跟随鼠标</span>
      <input
        type="checkbox"
        checked={settings?.follow_mouse === "on"}
        onchange={(e) => toggleSetting("follow_mouse", (e.currentTarget as HTMLInputElement).checked)}
      />
      <span class="switch"></span>
    </label>
    <label class="switch-row">
      <span>粘贴后保持打开</span>
      <input
        type="checkbox"
        checked={settings?.keep_open === "on"}
        onchange={(e) => toggleSetting("keep_open", (e.currentTarget as HTMLInputElement).checked)}
      />
      <span class="switch"></span>
    </label>
    <label class="switch-row">
      <span>主窗口置顶</span>
      <input
        type="checkbox"
        checked={settings?.always_on_top === "on"}
        onchange={(e) => toggleSetting("always_on_top", (e.currentTarget as HTMLInputElement).checked)}
      />
      <span class="switch"></span>
    </label>
    <label class="switch-row">
      <span>开机自启</span>
      <input
        type="checkbox"
        checked={settings?.autostart ?? false}
        onchange={(e) => toggleAutostart((e.currentTarget as HTMLInputElement).checked)}
      />
      <span class="switch"></span>
    </label>
  </div>

  <!-- 快捷键 -->
  <div class="sp-section">
    <div class="sp-title">
      <Keyboard size={12} />
      全局快捷键
    </div>
    {#if !hotkeyCapture}
      <div class="sp-row">
        <kbd class="hotkey-chip">{currentHotkey}</kbd>
        <button class="btn small" onclick={() => (hotkeyCapture = true)}>修改</button>
        <span class="sp-msg">{settingsMsg}</span>
      </div>
    {:else}
      <div
        class="capture-box"
        role="button"
        tabindex="0"
        bind:this={captureBoxEl}
        onkeydown={onHotkeyKeydown}
        onblur={() => {
          if (hotkeyCapture) hotkeyCapture = false;
        }}
      >
        <span class="capture-hint">请按下新的快捷键组合（需含 Ctrl/Alt/Shift/Win，Esc 取消）</span>
        <strong class="capture-value">{hotkeyDraft || "…"}</strong>
        <span class="capture-msg">{settingsMsg}</span>
      </div>
    {/if}
  </div>

  <!-- 历史 -->
  <div class="sp-section">
    <div class="sp-title">
      <History size={12} />
      历史
    </div>
    <div class="sp-row">
      <span class="sp-label">上限</span>
      <input
        class="num-input"
        type="number"
        min="1"
        max="100000"
        bind:value={maxItemsInput}
        onkeydown={(e) => {
          if (e.key === "Enter") saveSettings();
        }}
      />
      <span class="sp-unit">条</span>
      <button class="btn small primary" onclick={saveSettings}>保存</button>
    </div>
    {#if stats}
      <p class="sp-stats">
        共 {stats.total} 条（文本 {stats.text} · 图片 {stats.image} · 文件 {stats.files}）
        <br />
        数据库 {fmtSize(stats.db_size)} · 图片文件 {fmtSize(stats.media_size)}
      </p>
    {/if}
    <div class="menu-wrap">
      <button class="btn small danger" onclick={() => (clearMenuOpen = !clearMenuOpen)}>
        清空历史
        <span class="caret">▾</span>
      </button>
      {#if clearMenuOpen}
        <div
          class="menu-backdrop"
          role="presentation"
          onclick={() => (clearMenuOpen = false)}
          onkeydown={() => {}}
        ></div>
        <div class="menu">
          <button onclick={clearUnpinned}>清空非固定历史（保留固定）</button>
          <button
            class="danger"
            class:confirm={confirmClearAll}
            onclick={clearAllItems}
          >
            {confirmClearAll ? "⚠️ 确认清空全部！" : "清空全部历史（含固定）"}
          </button>
        </div>
      {/if}
    </div>
  </div>

  <!-- 数据与维护 -->
  <div class="sp-section">
    <div class="sp-title">
      <Wrench size={12} />
      数据与维护
    </div>
    <div class="sp-row">
      <button class="btn small" onclick={openDataDirectory}>打开数据目录</button>
      <button class="btn small danger" onclick={doReset}>恢复默认设置</button>
    </div>
  </div>

  <p class="sp-hint">
    快捷键格式：<code>Ctrl+Shift+V</code>、<code>Alt+Q</code> 等<br />
    数据目录：<code>%APPDATA%\com.aliboder.pasteboard</code>（删除图片文件后条目自动隐藏）
  </p>
</section>

<style>
  .settings-panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 14px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
    max-height: 58%;
    overflow-y: auto;
  }
  .sp-section {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .sp-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--text-dim);
    text-transform: uppercase;
  }
  .sp-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sp-label {
    font-size: 12px;
    color: var(--text);
    flex-shrink: 0;
  }
  .sp-msg {
    font-size: 11px;
    color: var(--accent);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sp-stats {
    margin: 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.7;
  }
  .sp-unit {
    font-size: 12px;
    color: var(--text-dim);
  }
  .sp-hint {
    margin: 2px 0 0;
    font-size: 10.5px;
    color: var(--text-dim);
    line-height: 1.8;
  }
  .sp-hint code {
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
    font-size: 10px;
  }

  .settings-panel select,
  .settings-panel input[type="number"] {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 7px;
    color: var(--text);
    padding: 5px 9px;
    font-size: 12px;
    outline: none;
    font-family: inherit;
  }
  .settings-panel select:focus,
  .settings-panel input[type="number"]:focus {
    border-color: var(--accent);
  }
  .num-input {
    width: 64px;
  }

  .btn {
    border: 1px solid var(--border);
    background: var(--bg-soft);
    color: var(--text);
    border-radius: 7px;
    padding: 5px 12px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition: all 0.12s;
  }
  .btn:hover {
    border-color: var(--border-strong);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
    font-weight: 600;
  }
  .btn.small {
    padding: 3px 10px;
    font-size: 11.5px;
  }
  .btn.danger {
    color: var(--danger);
  }

  /* 快捷键显示与录制 */
  .hotkey-chip {
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 4px 10px;
    font-family: Consolas, monospace;
    font-size: 12px;
    color: var(--accent);
  }
  .capture-box {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg);
    border: 1px dashed var(--accent);
    border-radius: 8px;
    padding: 10px 12px;
    outline: none;
  }
  .capture-box:focus {
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .capture-hint {
    color: var(--text-dim);
    font-size: 11px;
  }
  .capture-value {
    font-family: Consolas, monospace;
    font-size: 15px;
    color: var(--accent);
  }
  .capture-msg {
    color: var(--danger);
    font-size: 11px;
  }

  /* 开关行 */
  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    padding: 3px 0;
  }
  .switch-row input {
    display: none;
  }
  .switch {
    width: 32px;
    height: 18px;
    border-radius: 999px;
    background: var(--border);
    position: relative;
    transition: background 0.15s;
    flex-shrink: 0;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.15s;
  }
  .switch-row input:checked + .switch {
    background: var(--accent);
  }
  .switch-row input:checked + .switch::after {
    transform: translateX(14px);
  }

  /* 清空历史二级菜单 */
  .menu-wrap {
    position: relative;
  }
  .caret {
    font-size: 9px;
    margin-left: 4px;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
  }
  .menu {
    position: absolute;
    bottom: calc(100% + 4px);
    left: 0;
    z-index: 21;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    padding: 4px;
    min-width: 170px;
  }
  .menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 7px 10px;
    border: none;
    background: none;
    border-radius: 6px;
    color: var(--text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--bg-hover);
  }
  .menu button.danger {
    color: var(--danger);
  }
  .menu button.danger.confirm {
    background: var(--danger);
    color: #fff;
    font-weight: 700;
  }
</style>
