<script lang="ts">
  import { onMount } from "svelte";
  import "../lib/theme.css";
  import {
    getHistory,
    pinItem,
    deleteItem,
    clearHistory,
    clearAllHistory,
    pasteItem,
    getImage,
    getFilePreview,
    getSettings,
    setMaxItems,
    setTheme,
    setHotkey,
    setToggle,
    setAutostart,
    setWindowSize,
    openDataDir,
    getStats,
    resetSettings,
    onChange,
    type ItemDto,
    type ItemKind,
    type SettingsDto,
    type StatsDto,
  } from "../lib/api";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import FileTile from "../lib/FileTile.svelte";
  import {
    Search,
    X,
    Settings,
    Pin,
    PinOff,
    Star,
    Palette,
    Sliders,
    Keyboard,
    History,
    Wrench,
    Folder,
    Image as ImageIcon,
    ClipboardList,
  } from "lucide-svelte";

  /** 可大预览的图片扩展名（files 类型判断用） */
  const PREVIEW_IMAGE_EXTS = new Set([
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "avif", "tif", "tiff",
  ]);

  let items: ItemDto[] = $state([]);
  let filter = $state("");
  let kindFilter = $state<ItemKind | "">("");
  /** 类型筛选 Tab 配置 */
  const kindTabs: { k: ItemKind | ""; label: string }[] = [
    { k: "", label: "全部" },
    { k: "text", label: "文本" },
    { k: "image", label: "图片" },
    { k: "files", label: "文件" },
  ];

  /** 分流：上方横向区 = 图片+文件；下方列表 = 文本 */
  let topItems = $derived(items.filter((i) => i.kind !== "text"));
  let textItems = $derived(items.filter((i) => i.kind === "text"));

  // ---------- 文本列表虚拟滚动 ----------
  /** 行高三档：1 行 / 2 行 / 3 行（与 .row 内 padding + title 行高 + time 一致，留 3~4px 裕量） */
  const ROW_H_SHORT = 56;
  const ROW_H_MID = 76;
  const ROW_H_LONG = 96;
  /** 视口上下各多渲染的行数（缓冲） */
  const VIRTUAL_BUFFER = 4;
  let listEl: HTMLElement | undefined = $state();
  let listScrollTop = $state(0);
  let listViewportH = $state(400);

  /** 估算条目行高：按当前列表宽度估算换行数（换行文本一律按 3 行档） */
  function rowHeightOf(item: ItemDto): number {
    const text = item.preview;
    if (text.includes("\n")) return ROW_H_LONG;
    const cpl = Math.max(16, Math.floor(((listEl?.clientWidth ?? 300) - 48) / 13.5));
    const lines = Math.min(3, Math.max(1, Math.ceil(text.length / cpl)));
    return lines === 1 ? ROW_H_SHORT : lines === 2 ? ROW_H_MID : ROW_H_LONG;
  }

  const rowHeights = $derived(textItems.map(rowHeightOf));
  /** rowOffsets[i] = 第 0..i 行累计高度（行 i 底边） */
  const rowOffsets = $derived.by(() => {
    const arr: number[] = [];
    let acc = 0;
    for (const h of rowHeights) {
      acc += h;
      arr.push(acc);
    }
    return arr;
  });
  const totalHeight = $derived(rowOffsets[rowOffsets.length - 1] ?? 0);

  /** 第一个底边 > offset 的行索引（二分） */
  function firstIndexAfter(offset: number): number {
    const arr = rowOffsets;
    let lo = 0;
    let hi = arr.length;
    while (lo < hi) {
      const m = (lo + hi) >> 1;
      if (arr[m] <= offset) lo = m + 1;
      else hi = m;
    }
    return lo;
  }

  const viewStart = $derived.by(() =>
    Math.max(0, firstIndexAfter(Math.max(0, listScrollTop - VIRTUAL_BUFFER * ROW_H_LONG)))
  );
  const viewEnd = $derived.by(() =>
    Math.min(
      rowOffsets.length,
      firstIndexAfter(listScrollTop + listViewportH + VIRTUAL_BUFFER * ROW_H_LONG)
    )
  );
  const visibleTextItems = $derived.by(() => textItems.slice(viewStart, viewEnd));

  function onListScroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    listScrollTop = el.scrollTop;
    listViewportH = el.clientHeight;
  }

  /** 键盘导航时选中行保持可见 */
  $effect(() => {
    const el = listEl;
    if (!el || selected < 0 || selected >= rowOffsets.length) return;
    const top = selected === 0 ? 0 : rowOffsets[selected - 1];
    const h = rowHeights[selected];
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + h > el.scrollTop + el.clientHeight)
      el.scrollTop = top + h - el.clientHeight;
  });
  let selected = $state(-1);
  let loading = $state(true);
  let error = $state("");
  let errorTimer: ReturnType<typeof setTimeout> | null = null;
  /** 失焦自动隐藏的抑制标志（粘贴流程中焦点切换不应误关窗口） */
  let suppressBlurHide = false;
  /** 本次显示后是否曾获得焦点（防止显示失败导致的瞬时误隐藏） */
  let hasFocusSinceShow = true;
  /** 失焦后挂起的隐藏定时器（延迟期间识别缩放/移动动作则取消） */
  let blurHideTimer: ReturnType<typeof setTimeout> | null = null;

  /** 取消挂起的失焦隐藏 */
  function cancelBlurHide() {
    if (blurHideTimer) {
      clearTimeout(blurHideTimer);
      blurHideTimer = null;
    }
  }

  /** 提取 invoke 错误信息（兼容字符串与对象），并显示为短暂 toast */
  function showError(e: unknown) {
    const msg =
      typeof e === "string"
        ? e
        : (e as { message?: string } | null)?.message ?? "操作失败";
    error = msg;
    if (errorTimer) clearTimeout(errorTimer);
    errorTimer = setTimeout(() => {
      error = "";
    }, 3000);
  }
  let settings = $state<SettingsDto | null>(null);
  let showSettings = $state(false);
  let maxItemsInput = $state("500");
  let themeSel = $state("dark");
  let currentHotkey = $state("Ctrl+Shift+V");
  let hotkeyCapture = $state(false);
  let hotkeyDraft = $state("");
  let settingsMsg = $state("");
  let stats = $state<StatsDto | null>(null);
  let clearMenuOpen = $state(false);
  let captureBoxEl: HTMLElement | undefined = $state();

  /** 进入录制模式时聚焦录制框 */
  $effect(() => {
    if (hotkeyCapture) captureBoxEl?.focus();
  });
  let hoverPreview = $state<{ src: string; top: number; height: number } | null>(null);
  let textPreview = $state<{ text: string; top: number; height: number } | null>(null);
  const previewCache = new Map<number, string>();
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  let textHoverTimer: ReturnType<typeof setTimeout> | null = null;
  let listSectionEl: HTMLElement | undefined = $state();
  let contentEl: HTMLElement | undefined = $state();
  let resizeTimer: ReturnType<typeof setTimeout> | null = null;

  /** 主题：dark / light / system（system 跟随系统深色模式，实时响应变化） */
  let mediaDark: MediaQueryList | null = null;
  let mediaHandler: (() => void) | null = null;

  function applyTheme(theme: string) {
    if (theme === "system") {
      mediaDark = window.matchMedia("(prefers-color-scheme: dark)");
      document.documentElement.setAttribute(
        "data-theme",
        mediaDark.matches ? "dark" : "light"
      );
      if (mediaHandler && mediaDark) mediaDark.removeEventListener("change", mediaHandler);
      mediaHandler = () => {
        if (settings?.theme === "system" && mediaDark) {
          document.documentElement.setAttribute(
            "data-theme",
            mediaDark.matches ? "dark" : "light"
          );
        }
      };
      mediaDark.addEventListener("change", mediaHandler);
    } else {
      if (mediaHandler && mediaDark) {
        mediaDark.removeEventListener("change", mediaHandler);
        mediaHandler = null;
      }
      document.documentElement.setAttribute("data-theme", theme);
    }
  }

  /** 窗口尺寸记忆：resize 结束后（500ms 防抖）保存逻辑尺寸 */
  function watchResize() {
    const win = getCurrentWindow();
    win.onResized(async () => {
      if (listEl) listViewportH = listEl.clientHeight;
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(async () => {
        try {
          const size = await win.innerSize();
          const scale = await win.scaleFactor();
          await setWindowSize(size.width / scale, size.height / scale);
        } catch {
          /* ignore */
        }
      }, 500);
    });
  }

  /** 横向条滚轮：垂直增量转为水平滚动（deltaMode=1 为行模式，乘 24 近似像素） */
  function onStripWheel(e: WheelEvent) {
    e.preventDefault();
    const strip = e.currentTarget as HTMLElement;
    const factor = e.deltaMode === 1 ? 24 : 1;
    strip.scrollLeft += e.deltaY * factor;
  }

  /** 时间显示：今天 → HH:mm，昨天 → 昨天 HH:mm，更早 → MM-DD HH:mm */
  function timeLabel(ms: number): string {
    const d = new Date(ms);
    const now = new Date();
    const hhmm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    if (d.toDateString() === now.toDateString()) return hhmm;
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (d.toDateString() === yesterday.toDateString()) return `昨天 ${hhmm}`;
    return `${d.getMonth() + 1}-${d.getDate()} ${hhmm}`;
  }

  async function openSettings() {
    showSettings = !showSettings;
    settingsMsg = "";
    hotkeyCapture = false;
    if (showSettings) {
      settings = await getSettings();
      maxItemsInput = String(settings.max_items);
      themeSel = settings.theme;
      currentHotkey = settings.hotkey;
      stats = await getStats();
    }
  }

  async function saveSettings() {
    const n = parseInt(maxItemsInput, 10);
    if (isNaN(n) || n < 1) {
      settingsMsg = "请输入大于 0 的数字";
      return;
    }
    await setMaxItems(n);
    await setTheme(themeSel);
    applyTheme(themeSel);
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

  async function refreshStats() {
    stats = await getStats();
  }

  async function doReset() {
    settingsMsg = "";
    try {
      await resetSettings();
      await refreshSettings();
      await openSettings();
      settingsMsg = "已恢复默认设置";
    } catch (e) {
      settingsMsg = String(e);
    }
  }

  /** 刷新设置并应用主题（聚焦时调用） */
  async function refreshSettings() {
    try {
      settings = await getSettings();
      if (settings) applyTheme(settings.theme);
    } catch {
      /* ignore */
    }
  }

  async function reload() {
    loading = true;
    items = await getHistory(filter, kindFilter, 500, 0);
    if (selected >= textItems.length) selected = -1;
    loading = false;
  }

  /** 横向卡片显示名：取文件名（多文件时加数量） */
  function fileName(item: ItemDto): string {
    const base = item.preview.split(/[\\/]/).pop() ?? item.preview;
    return item.file_count > 1 ? `${base} 等${item.file_count}个` : base;
  }

  async function togglePin(item: ItemDto) {
    const ok = await pinItem(item.id, !item.pinned);
    if (ok) await reload();
  }

  async function remove(item: ItemDto) {
    await deleteItem(item.id);
    await reload();
  }

  /** 清空历史（设置面板二级菜单） */
  async function clearUnpinned() {
    clearMenuOpen = false;
    const n = await clearHistory();
    await reload();
    await refreshStats();
    settingsMsg = `已清空 ${n} 条非固定历史`;
  }

  async function clearAllItems() {
    clearMenuOpen = false;
    const n = await clearAllHistory();
    await reload();
    await refreshStats();
    settingsMsg = `已清空全部 ${n} 条历史`;
  }

  async function paste(id: number) {
    error = "";
    // 粘贴会切焦点到目标窗口，期间抑制失焦隐藏（配合"粘贴后保持打开"）
    suppressBlurHide = true;
    setTimeout(() => {
      suppressBlurHide = false;
    }, 600);
    try {
      await pasteItem(id);
      const keepOpen = settings?.keep_open === "on";
      if (!keepOpen) await getCurrentWindow().hide();
    } catch (e) {
      showError(e);
    }
  }

  /** 图片悬停大图预览（350ms 延迟，原图懒加载并缓存）；
   *  image 类型走库内原图；files 类型仅对图片文件（按扩展名）走路径读取；
   *  位置相对内容区（content）计算，横条/网格/文件列表统一复用 */
  async function showPreview(item: ItemDto, rowEl: HTMLElement) {
    if (item.kind !== "image" && item.kind !== "files") return;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = setTimeout(async () => {
      let src = previewCache.get(item.id);
      if (!src) {
        if (item.kind === "image") {
          src = (await getImage(item.id)) ?? "";
        } else {
          // files：第一个文件路径，仅图片扩展名可预览
          const path = item.preview;
          const ext = path.split(".").pop()?.toLowerCase() ?? "";
          if (!PREVIEW_IMAGE_EXTS.has(ext)) return;
          src = (await getFilePreview(path)) ?? "";
        }
        if (src) previewCache.set(item.id, src);
      }
      if (!src) return;
      const MAX_H = 300;
      const FOOTER_RESERVE = 44;
      const rowRect = rowEl.getBoundingClientRect();
      const contentRect = contentEl?.getBoundingClientRect() ?? rowRect;
      const itemBottom = rowRect.bottom - contentRect.top;
      // 优先显示在条目下方
      let top = itemBottom + 8;
      let avail = window.innerHeight - FOOTER_RESERVE - top;
      if (avail < 80) {
        // 下方空间不足：改到条目上方（受顶部边界约束）
        top = Math.max(8, itemBottom - MAX_H - 8);
        avail = window.innerHeight - FOOTER_RESERVE - top;
      }
      const height = Math.min(MAX_H, Math.max(80, avail));
      hoverPreview = { src, top, height };
    }, 350);
  }

  function hidePreview() {
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverPreview = null;
  }

  /** 文本悬停全文预览：350ms 延迟；3 行内能显示完的短文本不弹，避免打扰 */
  function showTextPreview(item: ItemDto, rowEl: HTMLElement) {
    const text = item.full ?? item.preview;
    // 约 3 行 ≈ 80 字符，无换行且更短的内容无需预览
    if (text.length <= 80 && !text.includes("\n")) return;
    if (textHoverTimer) clearTimeout(textHoverTimer);
    textHoverTimer = setTimeout(() => {
      if (!listSectionEl) return;
      const rowRect = rowEl.getBoundingClientRect();
      const secRect = listSectionEl.getBoundingClientRect();
      const MAX_H = 300;
      // 优先显示在行下方
      let top = rowRect.bottom - secRect.top + 8;
      let below = secRect.bottom - 8 - rowRect.bottom;
      let height = Math.min(MAX_H, Math.max(80, below));
      if (below < 80) {
        // 下方空间不足：显示在行上方
        height = Math.min(MAX_H, Math.max(80, rowRect.top - secRect.top - 16));
        top = Math.max(8, rowRect.top - secRect.top - height - 8);
      }
      textPreview = { text, top, height };
    }, 350);
  }

  function hideTextPreview() {
    if (textHoverTimer) clearTimeout(textHoverTimer);
    textPreview = null;
  }

  /** 全局键盘：↑↓ 选择、Enter 粘贴、Esc 关闭、数字 1~9 快捷粘贴、Delete 删除（作用于文本区） */
  function globalKeydown(e: KeyboardEvent) {
    // 快捷键录制期间由录制框独占键盘
    if (hotkeyCapture) return;
    const inInput = document.activeElement?.tagName === "INPUT";
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (textItems.length) selected = (selected + 1) % textItems.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (textItems.length) selected = selected <= 0 ? textItems.length - 1 : selected - 1;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (selected >= 0 && textItems[selected]) paste(textItems[selected].id);
    } else if (e.key === "Escape") {
      getCurrentWindow().hide();
    } else if (e.key === "Delete" && !inInput) {
      if (selected >= 0 && textItems[selected]) remove(textItems[selected]);
    } else if (/^[1-9]$/.test(e.key) && !inInput) {
      const idx = parseInt(e.key, 10) - 1;
      if (textItems[idx]) paste(textItems[idx].id);
    }
  }

  onMount(() => {
    reload();
    // 应用已保存的主题与窗口尺寸
    refreshSettings()
      .then(async () => {
        if (settings && settings.win_w > 0 && settings.win_h > 0) {
          await getCurrentWindow().setSize(new LogicalSize(settings.win_w, settings.win_h));
        }
      })
      .catch(() => applyTheme("dark"));
    watchResize();
    // 失焦行为：点击窗口外部 → 延迟 250ms 隐藏（期间若发生缩放/移动/重新聚焦则取消）；
    // 重新聚焦 → 刷新列表与设置
    getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) {
        hasFocusSinceShow = true;
        cancelBlurHide();
        reload();
        refreshSettings();
      } else if (hasFocusSinceShow && !suppressBlurHide) {
        if (!blurHideTimer) {
          blurHideTimer = setTimeout(() => {
            blurHideTimer = null;
            getCurrentWindow().hide();
          }, 250);
        }
      }
    });
    // 缩放/移动进行中 → 取消挂起的隐藏（用户在拖边缘/标题栏，不是点击外部）
    getCurrentWindow().onResized(() => cancelBlurHide());
    getCurrentWindow().onMoved(() => cancelBlurHide());
    let cleanup: (() => void) | null = null;
    onChange(() => reload()).then((un) => (cleanup = un));
    return () => {
      cleanup?.();
    };
  });
</script>

<svelte:head>
  <title>PasteBoard</title>
</svelte:head>

<svelte:window onkeydown={globalKeydown} />

<div class="window">
  <!-- 顶栏：拖动区域（deep = 子树内非交互元素均可拖动）+ 搜索 + 清空 -->
  <header class="topbar" data-tauri-drag-region="deep">
    <div class="searchbox">
      <Search size={14} class="search-icon" />
      <input
        placeholder="搜索剪贴板历史…"
        bind:value={filter}
        oninput={reload}
        spellcheck="false"
      />
      {#if filter}
        <button class="icon-btn" title="清除" onclick={() => (filter = "")}>
          <X size={13} />
        </button>
      {/if}
    </div>
    <button
      class="icon-btn {showSettings ? 'active' : ''}"
      title="设置"
      onclick={() => (showSettings ? (showSettings = false) : openSettings())}
    >
      <Settings size={14} />
    </button>
  </header>

  {#if showSettings}
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
              <button class="danger" onclick={clearAllItems}>清空全部历史（含固定）</button>
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
  {/if}

  <!-- 内容区：类型 Tab + 按类型切换布局 -->
  <div class="content" bind:this={contentEl}>
    <!-- 类型筛选：全部 / 文本 / 图片 / 文件（与搜索叠加） -->
    <div class="kind-tabs">
      {#each kindTabs as tab (tab.k)}
        <button
          class="kind-tab {kindFilter === tab.k ? 'active' : ''}"
          onclick={() => {
            if (kindFilter !== tab.k) {
              kindFilter = tab.k;
              reload();
            }
          }}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    {#if kindFilter === ""}
    <!-- 全部 Tab：上方图片/文件横向条 -->
    <section class="strip-section">
      <div class="section-header">
        <span class="section-title">
          <ImageIcon size={12} />
          图片 / 文件
        </span>
        <span class="section-count">{topItems.length} 条</span>
      </div>
      {#if topItems.length === 0}
        <p class="strip-empty">{filter ? "无匹配结果" : "暂无图片/文件历史"}</p>
      {:else}
        <div class="strip" onwheel={onStripWheel}>
          {#each topItems as item (item.id)}
            <div
              class="strip-item {item.kind}"
              class:pinned={item.pinned}
              role="option"
              aria-selected={false}
              aria-label={item.preview}
              tabindex="-1"
              title={item.kind === "image"
                ? `图片 · ${timeLabel(item.created_at)}`
                : `${item.preview} · ${timeLabel(item.created_at)}`}
              onmouseenter={(e) => showPreview(item, e.currentTarget as HTMLElement)}
              onmouseleave={hidePreview}
              onclick={() => paste(item.id)}
              onkeydown={(e) => {
                if (e.key === "Enter") paste(item.id);
              }}
            >
              {#if item.kind === "image"}
                {#if item.thumb}
                  <img src="data:image/png;base64,{item.thumb}" alt="图片" />
                {:else}
                  <span class="strip-placeholder">
                    <ImageIcon size={16} />
                  </span>
                {/if}
              {:else}
                <FileTile path={item.preview} name={fileName(item)} />
              {/if}
              <div class="strip-actions">
                <button
                  class="icon-btn mini {item.pinned ? 'active' : ''}"
                  title={item.pinned ? "取消固定" : "固定"}
                  onclick={(e) => {
                    e.stopPropagation();
                    togglePin(item);
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
                  title="删除"
                  onclick={(e) => {
                    e.stopPropagation();
                    remove(item);
                  }}
                >
                  <X size={11} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
    {:else if kindFilter === "image"}
    <!-- 图片 Tab：纵向网格，充分利用空间 -->
    <section class="grid-section">
      <div class="section-header">
        <span class="section-title">
          <ImageIcon size={12} />
          图片
        </span>
        <span class="section-count">{topItems.length} 条</span>
      </div>
      {#if topItems.length === 0}
        <p class="strip-empty">暂无图片历史</p>
      {:else}
        <div class="grid">
          {#each topItems as item (item.id)}
            <div
              class="grid-item"
              class:pinned={item.pinned}
              role="option"
              aria-selected={false}
              tabindex="-1"
              title={`图片 · ${timeLabel(item.created_at)}`}
              onmouseenter={(e) => showPreview(item, e.currentTarget as HTMLElement)}
              onmouseleave={hidePreview}
              onclick={() => paste(item.id)}
              onkeydown={(e) => {
                if (e.key === "Enter") paste(item.id);
              }}
            >
              {#if item.thumb}
                <img src="data:image/png;base64,{item.thumb}" alt="图片" draggable="false" />
              {:else}
                <span class="strip-placeholder">
                  <ImageIcon size={16} />
                </span>
              {/if}
              {#if item.pinned}
                <span class="grid-pin-badge">
                  <Star size={11} fill="currentColor" />
                </span>
              {/if}
              <span class="grid-time">{timeLabel(item.created_at)}</span>
              <div class="grid-actions">
                <button
                  class="icon-btn mini {item.pinned ? 'active' : ''}"
                  title={item.pinned ? "取消固定" : "固定"}
                  onclick={(e) => {
                    e.stopPropagation();
                    togglePin(item);
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
                  title="删除"
                  onclick={(e) => {
                    e.stopPropagation();
                    remove(item);
                  }}
                >
                  <X size={11} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
    {:else if kindFilter === "files"}
    <!-- 文件 Tab：纵向列表，图标 + 名称 + 路径 -->
    <section class="file-section">
      <div class="section-header">
        <span class="section-title">
          <Folder size={12} />
          文件
        </span>
        <span class="section-count">{topItems.length} 条</span>
      </div>
      {#if topItems.length === 0}
        <p class="strip-empty">暂无文件历史</p>
      {:else}
        <div class="file-list">
          {#each topItems as item (item.id)}
            <div
              class="file-row"
              class:pinned={item.pinned}
              role="option"
              aria-selected={false}
              tabindex="-1"
              title={`${item.preview} · ${timeLabel(item.created_at)}`}
              onmouseenter={(e) => showPreview(item, e.currentTarget as HTMLElement)}
              onmouseleave={hidePreview}
              onclick={() => paste(item.id)}
              onkeydown={(e) => {
                if (e.key === "Enter") paste(item.id);
              }}
            >
              <FileTile path={item.preview} name={fileName(item)} horizontal />
              <span class="file-time">{timeLabel(item.created_at)}</span>
              <div class="actions">
                <button
                  class="icon-btn {item.pinned ? 'active' : ''}"
                  title={item.pinned ? "取消固定" : "固定"}
                  onclick={(e) => {
                    e.stopPropagation();
                    togglePin(item);
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
                  title="删除"
                  onclick={(e) => {
                    e.stopPropagation();
                    remove(item);
                  }}
                >
                  <X size={13} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
    {/if}

    {#if kindFilter === "" || kindFilter === "text"}
    <!-- 文本历史（全部与文本 Tab 显示） -->
    <section
      class="list-section"
      class:no-top={kindFilter !== ""}
      bind:this={listSectionEl}
    >
      <div class="section-header">
        <span class="section-title">
          <ClipboardList size={12} />
          文本
        </span>
        <span class="section-count">{textItems.length} 条</span>
      </div>
      <main class="list" bind:this={listEl} onscroll={onListScroll}>
        {#if loading}
          <p class="empty">加载中…</p>
        {:else if textItems.length === 0}
          <p class="empty">
            {filter ? "没有匹配的文本" : "暂无文本历史\n复制文字试试"}
          </p>
        {:else}
          <div class="list-inner" style="height: {totalHeight}px">
            {#each visibleTextItems as item, vi (item.id)}
              {@const gi = viewStart + vi}
              {@const top = gi === 0 ? 0 : rowOffsets[gi - 1]}
              <div
                class="row {item.kind}"
                class:selected={gi === selected}
                class:pinned={item.pinned}
                style="top: {top}px"
                role="option"
                aria-selected={gi === selected}
                tabindex="-1"
                onmouseenter={(e) => {
                  selected = gi;
                  showTextPreview(item, e.currentTarget as HTMLElement);
                }}
                onmouseleave={hideTextPreview}
                onclick={() => paste(item.id)}
                onkeydown={(e) => {
                  if (e.key === "Enter") paste(item.id);
                }}
              >
                <div class="meta">
                  <span class="title">
                    {#if item.pinned}
                      <Star size={11} fill="currentColor" class="pin-star" />
                    {/if}
                    {item.preview}
                  </span>
                  <span class="time">{timeLabel(item.created_at)}</span>
                </div>

                <div class="actions">
                  <button
                    class="icon-btn {item.pinned ? 'active' : ''}"
                    title={item.pinned ? "取消固定" : "固定"}
                    onclick={(e) => {
                      e.stopPropagation();
                      togglePin(item);
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
                    title="删除"
                    onclick={(e) => {
                      e.stopPropagation();
                      remove(item);
                    }}
                  >
                    <X size={13} />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </main>

      {#if textPreview}
        <div
          class="text-preview"
          style="top: {textPreview.top}px; height: {textPreview.height}px"
        >
          <pre>{textPreview.text}</pre>
        </div>
      {/if}
    </section>
    {/if}

    {#if hoverPreview}
      <div
        class="img-preview"
        style="top: {hoverPreview.top}px; height: {hoverPreview.height}px"
      >
        <img src="data:image/png;base64,{hoverPreview.src}" alt="预览" />
      </div>
    {/if}
  </div>

  <!-- 错误提示 toast（不遮挡列表，3 秒自动消失） -->
  {#if error}
    <div class="toast">{error}</div>
  {/if}

  <!-- 底部提示（也可拖动窗口） -->
  <footer class="footer" data-tauri-drag-region="deep">
    <span>↑↓ 选择</span>
    <span>Enter 粘贴</span>
    <span>Esc 关闭</span>
    <span class="dot">•</span>
    <span>文本 {textItems.length} · 媒体 {topItems.length}</span>
  </footer>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }

  @keyframes window-in {
    from {
      opacity: 0;
      transform: translateY(-8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .window {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: linear-gradient(180deg, var(--bg-grad), var(--bg) 40%);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    overflow: hidden;
    animation: window-in 160ms cubic-bezier(0.2, 0.8, 0.3, 1);
  }

  /* 顶栏 */
  .topbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px 8px;
  }
  .searchbox {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 9px;
    padding: 6px 10px;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .searchbox:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .search-icon {
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .searchbox input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: 13px;
    font-family: inherit;
  }
  .searchbox input::placeholder {
    color: var(--text-dim);
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
    color: var(--star);
  }
  .icon-btn.danger:hover {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }

  /* 设置面板（内联，分区排版） */
  .settings-panel {
    margin: 0 10px 8px;
    padding: 12px 14px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    max-height: 62%;
    overflow-y: auto;
  }
  .sp-section {
    padding: 8px 0;
  }
  .sp-section + .sp-section {
    border-top: 1px solid var(--border);
  }
  .sp-title {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.4px;
    color: var(--text-dim);
    margin-bottom: 8px;
  }
  .sp-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .sp-label {
    color: var(--text);
    min-width: 40px;
  }
  .sp-unit {
    color: var(--text-dim);
  }
  .sp-stats {
    margin: 8px 0;
    color: var(--text-dim);
    font-size: 11.5px;
    line-height: 1.7;
  }
  .sp-msg {
    color: var(--accent);
    margin-left: auto;
  }
  .sp-hint {
    margin: 2px 0 0;
    padding-top: 8px;
    border-top: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 11px;
    line-height: 1.7;
  }
  .sp-hint code {
    background: var(--bg);
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 10.5px;
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

  /* 按钮 */
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    font-size: 11.5px;
    padding: 4px 11px;
    border-radius: 7px;
    cursor: pointer;
    transition: background 0.12s, border-color 0.12s;
    font-family: inherit;
  }
  .btn:hover {
    background: var(--bg-hover);
    border-color: var(--border-strong);
  }
  .btn.primary {
    background: var(--accent);
    border-color: transparent;
    color: #10121a;
    font-weight: 600;
  }
  .btn.primary:hover {
    filter: brightness(1.08);
  }
  .btn.danger {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 45%, transparent);
  }
  .btn.danger:hover {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }

  /* 开关（自定义 switch） */
  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 5px 0;
    color: var(--text);
    cursor: pointer;
    user-select: none;
  }
  .switch-row input {
    display: none;
  }
  .switch {
    width: 34px;
    height: 18px;
    border-radius: 99px;
    background: var(--border-strong);
    position: relative;
    flex-shrink: 0;
    transition: background 0.15s;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #ffffff;
    transition: transform 0.15s;
  }
  .switch-row input:checked + .switch {
    background: var(--accent);
  }
  .switch-row input:checked + .switch::after {
    transform: translateX(16px);
  }

  /* 清空历史二级菜单 */
  .menu-wrap {
    position: relative;
    margin-top: 4px;
  }
  .caret {
    font-size: 10px;
    opacity: 0.8;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: absolute;
    left: 0;
    top: calc(100% + 4px);
    z-index: 41;
    background: var(--bg-soft);
    border: 1px solid var(--border-strong);
    border-radius: 9px;
    box-shadow: var(--shadow);
    overflow: hidden;
    min-width: 200px;
    padding: 4px;
  }
  .menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 7px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    font-family: inherit;
  }
  .menu button:hover {
    background: var(--bg-hover);
  }
  .menu button.danger {
    color: var(--danger);
  }

  /* 内容区：上横向区 + 下文本区 */
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* 上方：图片/文件横向区（约占 20%） */
  .strip-section {
    height: 20%;
    min-height: 96px;
    display: flex;
    flex-direction: column;
    padding: 2px 10px 6px;
    position: relative;
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
    opacity: 0.75;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 99px;
    padding: 0 6px;
    line-height: 14px;
  }
  .strip {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 8px;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .strip::-webkit-scrollbar {
    height: 4px;
  }
  .strip::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 2px;
  }
  .strip-item {
    position: relative;
    flex-shrink: 0;
    width: 92px;
    height: 100%;
    border-radius: 9px;
    overflow: hidden;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: border-color 0.15s, transform 0.15s, box-shadow 0.15s;
  }
  .strip-item:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
  }
  .strip-item img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .strip-placeholder {
    color: var(--text-dim);
    font-size: 11px;
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
  .strip-actions {
    position: absolute;
    top: 2px;
    right: 2px;
    display: flex;
    gap: 1px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .strip-item:hover .strip-actions {
    opacity: 1;
  }
  .icon-btn.mini {
    width: 18px;
    height: 18px;
    font-size: 11px;
    background: color-mix(in srgb, var(--bg) 62%, transparent);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .strip-empty {
    margin: 0;
    padding: 16px 0;
    text-align: center;
    color: var(--text-dim);
    font-size: 12px;
  }

  /* 图片 Tab：纵向网格 */
  .grid-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 2px 10px 6px;
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
  .grid-item {
    position: relative;
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
  .grid-item img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .grid-item:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
  }
  .grid-item.pinned {
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .grid-time {
    position: absolute;
    right: 4px;
    bottom: 4px;
    font-size: 9.5px;
    color: #fff;
    background: rgba(0, 0, 0, 0.55);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: "Cascadia Mono", Consolas, monospace;
    pointer-events: none;
  }
  .grid-pin-badge {
    position: absolute;
    top: 4px;
    left: 4px;
    color: var(--accent);
    display: flex;
    filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.6));
    pointer-events: none;
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

  /* 文件 Tab：纵向列表 */
  .file-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 2px 10px 6px;
  }
  .file-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 0 8px;
  }
  .file-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    margin-bottom: 3px;
    border-radius: 9px;
    cursor: pointer;
    transition: background 0.12s;
  }
  .file-row:hover {
    background: var(--bg-hover);
  }
  .file-row.pinned {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }
  .file-row.pinned::before {
    content: "";
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 2px;
    background: var(--accent);
  }
  .file-time {
    color: var(--text-dim);
    font-size: 10.5px;
    font-family: "Cascadia Mono", Consolas, monospace;
    flex-shrink: 0;
  }

  /* 下方：文本列表 */
  .list-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
    position: relative;
  }
  .list-section.no-top {
    border-top: none;
  }
  .list-section .section-header {
    padding: 6px 12px 4px;
  }

  /* 列表 */
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 2px 0 8px;
    outline: none;
  }
  .list-inner {
    position: relative;
    padding: 0 10px;
  }
  .list::-webkit-scrollbar {
    width: 6px;
  }
  .list::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 3px;
  }

  /* 类型筛选 Tab */
  .kind-tabs {
    display: flex;
    gap: 4px;
    padding: 0 12px 6px;
    flex-shrink: 0;
  }
  .kind-tab {
    border: 1px solid var(--border);
    background: var(--bg-soft);
    color: var(--text-dim);
    border-radius: 999px;
    padding: 3px 12px;
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: all 0.12s;
  }
  .kind-tab:hover {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .kind-tab.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
    font-weight: 600;
  }

  /* 固定条目视觉：星标 + 主题色左边条 + 微弱底色 */
  .pin-star {
    color: var(--accent);
    margin-right: 4px;
    vertical-align: -1px;
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
  .strip-item.pinned {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .row {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 9px;
    cursor: pointer;
    transition: background 0.12s;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.selected {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 60%, transparent);
  }

  .thumb-wrap {
    width: 56px;
    height: 40px;
    flex-shrink: 0;
    border-radius: 6px;
    overflow: hidden;
    background: var(--bg-soft);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .thumb-wrap img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb-placeholder {
    color: var(--text-dim);
    font-size: 11px;
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
    /* 超过 3 行折叠省略；不足 3 行按实际行数渲染（自适应高度） */
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
  .actions .icon-btn {
    width: 22px;
    height: 22px;
  }

  /* 图片大图预览浮层：高度由 JS 按可用空间计算，图片 contain 适配 */
  .img-preview {
    position: absolute;
    left: 12px;
    right: 12px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
    z-index: 10;
    pointer-events: none;
    display: flex;
  }
  .img-preview img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  /* 文本全文预览浮层 */
  .text-preview {
    position: absolute;
    left: 12px;
    right: 12px;
    background: var(--bg-soft);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
    z-index: 10;
    pointer-events: none;
    overflow: hidden;
  }
  .text-preview pre {
    margin: 0;
    padding: 10px 12px;
    height: 100%;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text);
  }

  .empty {
    text-align: center;
    color: var(--text-dim);
    font-size: 13px;
    padding: 48px 0;
    white-space: pre-line;
    line-height: 1.8;
  }
  .error-msg {
    color: var(--danger);
  }

  .footer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 14px;
    border-top: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 10.5px;
    letter-spacing: 0.3px;
  }
  .footer .dot {
    margin-left: auto;
    opacity: 0.5;
  }

  /* 错误提示 toast */
  .toast {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    bottom: 40px;
    max-width: 85%;
    background: var(--bg-soft);
    border: 1px solid var(--danger);
    color: var(--danger);
    padding: 7px 14px;
    border-radius: 8px;
    font-size: 12px;
    box-shadow: var(--shadow);
    z-index: 30;
    pointer-events: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    animation: window-in 120ms ease-out;
  }
</style>
