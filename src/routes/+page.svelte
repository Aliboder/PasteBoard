<script lang="ts">
  import { onMount } from "svelte";
  import "../lib/theme.css";
  import {
    getHistory,
    pinItem,
    deleteItem,
    pasteItem,
    copyItem,
    openFileLocation,
    openFile,
    getSettings,
    setWindowSize,
    onChange,
    type ItemDto,
    type ItemKind,
    type SettingsDto,
  } from "../lib/api";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import FileTile from "../lib/FileTile.svelte";
  import TextRow from "../lib/TextRow.svelte";
  import GridPanel from "../lib/GridPanel.svelte";
  import SettingsPanel from "../lib/SettingsPanel.svelte";
  import ImageThumb from "../lib/ImageThumb.svelte";
  import { splitHighlight, timeLabel } from "../lib/utils";
  import {
    Search,
    X,
    Settings,
    Pin,
    PinOff,
    ExternalLink,
    Folder,
    Image as ImageIcon,
    ClipboardList,
  } from "lucide-svelte";

  let items: ItemDto[] = $state([]);
  let filter = $state("");
  let kindFilter = $state<ItemKind | "">("");
  /** 类型筛选 Tab 配置 */
  const kindTabs: { k: ItemKind | ""; label: string }[] = [
    { k: "", label: "全部" },
    { k: "pinned", label: "固定" },
    { k: "text", label: "文本" },
    { k: "image", label: "图片" },
    { k: "files", label: "文件" },
  ];

  /** 分流：上方横向区 = 图片+文件；下方列表 = 文本 */
  let topItems = $derived(items.filter((i) => i.kind !== "text"));
  let textItems = $derived(items.filter((i) => i.kind === "text"));

  // ---------- 文本列表虚拟滚动 ----------
  /** 行高三档：1 行 / 2 行 / 3 行（内容高 + 1px 描边 + 6px 行距） */
  const ROW_H_SHORT = 62;
  const ROW_H_MID = 82;
  const ROW_H_LONG = 102;
  /** 视口上下各多渲染的行数（缓冲） */
  const VIRTUAL_BUFFER = 4;
  let listEl: HTMLElement | undefined = $state();
  let listScrollTop = $state(0);
  let listViewportH = $state(400);

  /** 与 .title 一致的字体度量上下文（canvas 测量，惰性创建） */
  let measureCtx: CanvasRenderingContext2D | null = null;
  /** 字符宽度缓存（测量结果按字符复用） */
  const charWidthCache = new Map<string, number>();
  const TITLE_FONT = "13px system-ui, 'Segoe UI', 'Microsoft YaHei', sans-serif";

  function charWidth(ch: string): number {
    let w = charWidthCache.get(ch);
    if (w === undefined) {
      if (!measureCtx) measureCtx = document.createElement("canvas").getContext("2d");
      w = measureCtx?.measureText(ch).width ?? 13.5;
      charWidthCache.set(ch, w);
    }
    return w;
  }

  /** 标题区实际可用宽度：列表宽 - 两侧 padding - 行内 padding - 操作按钮区 - 间隙 */
  function titleMaxWidth(): number {
    const w = listEl?.clientWidth ?? 300;
    return Math.max(80, w - 106);
  }

  /** 按真实字体度量逐字符模拟换行（\n 强制换行，封顶 3 行 = line-clamp 上限） */
  function measureLines(text: string, maxWidth: number): number {
    let w = 0;
    let lines = 1;
    for (const ch of text) {
      if (ch === "\n") {
        lines += 1;
        w = 0;
      } else {
        const cw = charWidth(ch);
        w += cw;
        if (w > maxWidth) {
          lines += 1;
          w = cw; // 当前字符成为新行首字符
        }
      }
      if (lines >= 3) return 3;
    }
    return Math.min(3, lines);
  }

  /** 条目行高：按真实字体度量估算行数，三档对应 */
  function rowHeightOf(item: ItemDto): number {
    const lines = measureLines(item.preview, titleMaxWidth());
    return lines === 1 ? ROW_H_SHORT : lines === 2 ? ROW_H_MID : ROW_H_LONG;
  }

  /** 全量渲染阈值：条目数超过时启用虚拟滚动（正常量级走浏览器原生布局，精确无跳变） */
  const VIRTUAL_THRESHOLD = 600;
  const USE_VIRTUAL = $derived(textItems.length > VIRTUAL_THRESHOLD);

  const rowHeights = $derived(USE_VIRTUAL ? textItems.map(rowHeightOf) : []);
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
    if (!el || selected < 0) return;
    if (USE_VIRTUAL) {
      if (selected >= rowOffsets.length) return;
      const top = selected === 0 ? 0 : rowOffsets[selected - 1];
      const h = rowHeights[selected];
      if (top < el.scrollTop) el.scrollTop = top;
      else if (top + h > el.scrollTop + el.clientHeight)
        el.scrollTop = top + h - el.clientHeight;
    } else {
      // 全量渲染：滚动容器内定位选中行
      const row = el.querySelectorAll(".row")[selected] as HTMLElement | undefined;
      row?.scrollIntoView({ block: "nearest" });
    }
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
  /** 二次确认：待确认删除的固定条目 id（3 秒未再点复位） */
  let confirmDeleteId = $state<number | null>(null);
  let confirmTimer: ReturnType<typeof setTimeout> | null = null;
  /** 网格（图片/文件 Tab）键盘导航选中索引 */
  let gridSelected = $state(-1);
  /** 网格容器引用（当前 Tab 只有一个网格） */
  let gridEl: HTMLElement | undefined = $state();
  /** 右键菜单 */
  let ctxMenu = $state<{ x: number; y: number; item: ItemDto } | null>(null);

  let listSectionEl: HTMLElement | undefined = $state();
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

  async function openSettings() {
    showSettings = !showSettings;
    if (showSettings) {
      settings = await getSettings();
    }
  }

  /** 设置面板操作后：刷新列表、设置与主题（清空/恢复默认后调用） */
  async function onSettingsChanged() {
    await reload();
    settings = await getSettings();
    if (settings) applyTheme(settings.theme);
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

  /**
   * 刷新列表；silent 时不清空列表（保留旧内容渲染，数据到位就地替换），
   * 用于操作后刷新——避免"加载中"占位导致滚动位置丢失（全部页点固定按钮滚动条回顶问题）
   */
  async function reload(silent = false) {
    if (!silent) loading = true;
    try {
      items = await getHistory(filter, kindFilter, 500, 0);
    } catch (e) {
      // 数据库异常等场景：提示而非卡死"加载中"
      showError(e);
      items = [];
    }
    if (selected >= textItems.length) selected = -1;
    if (gridSelected >= topItems.length) gridSelected = -1;
    loading = false;
  }

  /** 网格键盘导航：选中卡片滚动进视野 */
  $effect(() => {
    if (gridSelected < 0 || !gridEl) return;
    const card = gridEl.querySelectorAll(".grid-item")[gridSelected] as HTMLElement | undefined;
    card?.scrollIntoView({ block: "nearest" });
  });

  /** 横向卡片显示名：取文件名（多文件时加数量） */
  function fileName(item: ItemDto): string {
    const base = item.preview.split(/[\\/]/).pop() ?? item.preview;
    return item.file_count > 1 ? `${base} 等${item.file_count}个` : base;
  }

  async function togglePin(item: ItemDto) {
    const ok = await pinItem(item.id, !item.pinned);
    if (ok) await reload(true);
  }

  /** 删除：固定条目需二次确认（3 秒内再点一次），非固定一键删除 */
  async function remove(item: ItemDto) {
    if (item.pinned && confirmDeleteId !== item.id) {
      confirmDeleteId = item.id;
      if (confirmTimer) clearTimeout(confirmTimer);
      confirmTimer = setTimeout(() => (confirmDeleteId = null), 3000);
      return;
    }
    if (confirmTimer) clearTimeout(confirmTimer);
    confirmDeleteId = null;
    await deleteItem(item.id);
    await reload(true);
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

  /** 网格列数：读计算样式，图片/文件 Tab 均自适应列数 */
  function gridCols(): number {
    if (!gridEl) return 1;
    return getComputedStyle(gridEl).gridTemplateColumns.split(" ").filter(Boolean).length || 1;
  }

  /** 网格键盘导航（图片/文件 Tab）：方向键移动、Enter 粘贴、Delete 删除 */
  function gridKeydown(e: KeyboardEvent, inInput: boolean) {
    const list = topItems;
    if (!list.length) return;
    const cols = gridCols();
    if (e.key === "ArrowRight") {
      e.preventDefault();
      gridSelected = (gridSelected + 1) % list.length;
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      gridSelected = gridSelected <= 0 ? list.length - 1 : gridSelected - 1;
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      gridSelected = Math.min(list.length - 1, (gridSelected < 0 ? 0 : gridSelected) + cols);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      gridSelected = Math.max(0, (gridSelected < 0 ? cols : gridSelected) - cols);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (gridSelected >= 0 && list[gridSelected]) paste(list[gridSelected].id);
      else if (list.length) paste(list[0].id);
    } else if (e.key === "Delete" && !inInput) {
      if (gridSelected >= 0 && list[gridSelected]) remove(list[gridSelected]);
    }
  }

  /** 全局键盘：↑↓ 选择、Enter 粘贴、Esc 关闭、数字 1~9 快捷粘贴、Delete 删除 */
  function globalKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (ctxMenu) {
        ctxMenu = null;
        return;
      }
      getCurrentWindow().hide();
      return;
    }
    const inInput = document.activeElement?.tagName === "INPUT";
    // 图片/文件 Tab：网格导航
    if (kindFilter === "image" || kindFilter === "files") {
      gridKeydown(e, inInput);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (textItems.length) selected = (selected + 1) % textItems.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (textItems.length) selected = selected <= 0 ? textItems.length - 1 : selected - 1;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (selected >= 0 && textItems[selected]) paste(textItems[selected].id);
      else if (filter && textItems.length) paste(textItems[0].id);
    } else if (e.key === "Delete" && !inInput) {
      if (selected >= 0 && textItems[selected]) remove(textItems[selected]);
    } else if (/^[1-9]$/.test(e.key) && !inInput) {
      const idx = parseInt(e.key, 10) - 1;
      if (textItems[idx]) paste(textItems[idx].id);
    }
  }

  /** 右键菜单：打开（位置钳制在窗口内） */
  function openCtxMenu(e: MouseEvent, item: ItemDto) {
    e.preventDefault();
    ctxMenu = {
      x: Math.min(e.clientX, window.innerWidth - 150),
      y: Math.min(e.clientY, window.innerHeight - 150),
      item,
    };
  }

  function closeCtxMenu() {
    ctxMenu = null;
  }

  /** 右键菜单：复制条目到剪贴板（不粘贴） */
  async function ctxCopy(item: ItemDto) {
    closeCtxMenu();
    try {
      await copyItem(item.id);
      error = "已复制到剪贴板";
      if (errorTimer) clearTimeout(errorTimer);
      errorTimer = setTimeout(() => (error = ""), 2000);
    } catch (e) {
      showError(e);
    }
  }

  /** 右键菜单：删除（固定条目同样二次确认） */
  function ctxDelete(item: ItemDto) {
    closeCtxMenu();
    if (item.pinned && confirmDeleteId !== item.id) {
      showError("固定条目需二次确认：请再点一次卡片上的删除按钮");
    }
    remove(item);
  }

  /** 右键菜单：打开文件所在位置 */
  async function ctxOpenLocation(item: ItemDto) {
    closeCtxMenu();
    try {
      await openFileLocation(item.preview);
    } catch (e) {
      showError(e);
    }
  }

  /** 右键菜单：打开文件 */
  async function ctxOpenFile(item: ItemDto) {
    closeCtxMenu();
    try {
      await openFile(item.preview);
    } catch (e) {
      showError(e);
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
        // silent：不闪"加载中"，避免滚动位置丢失
        reload(true);
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
    onChange(() => reload(true)).then((un) => (cleanup = un));
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
        oninput={() => reload()}
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
    <SettingsPanel
      {settings}
      onApplyTheme={applyTheme}
      onCleared={onSettingsChanged}
    />
  {/if}

  <!-- 内容区：类型 Tab + 按类型切换布局 -->
  <div class="content">
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

    {#if kindFilter === "" || kindFilter === "pinned"}
    <!-- 全部/固定 Tab：上方图片/文件横向条 -->
    <section class="strip-section">
      <div class="section-header">
        <span class="section-title">
          <ImageIcon size={12} />
          图片 / 文件
        </span>
        <span class="section-count">{topItems.length} 条</span>
      </div>
      {#if topItems.length === 0}
        <p class="strip-empty">
          {kindFilter === "pinned"
            ? "暂无固定图片/文件"
            : filter
              ? "无匹配结果"
              : "暂无图片/文件历史"}
        </p>
      {:else}
        <div class="strip" onwheel={onStripWheel}>
          {#each topItems as item (item.id)}
            <div
              class="strip-item {item.kind}{item.pinned ? ' pinned' : ''}"
              role="option"
              aria-selected={false}
              aria-label={item.preview}
              tabindex="-1"
              title={item.kind === "image"
                ? `图片 · ${timeLabel(item.created_at)}`
                : `${item.preview} · ${timeLabel(item.created_at)}`}
              onclick={() => paste(item.id)}
              oncontextmenu={(e) => openCtxMenu(e, item)}
              onkeydown={(e) => {
                if (e.key === "Enter") paste(item.id);
              }}
            >
              {#if item.kind === "image"}
                <ImageThumb id={item.id} />
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
                  class:confirm={confirmDeleteId === item.id}
                  title={confirmDeleteId === item.id ? "再点一次确认删除" : "删除"}
                  onclick={(e) => {
                    e.stopPropagation();
                    remove(item);
                  }}
                >
                  {#if confirmDeleteId === item.id}
                    <span class="confirm-txt">确认</span>
                  {:else}
                    <X size={11} />
                  {/if}
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
        {:else if kindFilter === "image"}
    <GridPanel
      items={topItems}
      kind="image"
      gridSelected={gridSelected}
      confirmDeleteId={confirmDeleteId}
      bindGridEl={(el) => (gridEl = el)}
      onSelect={(gi) => (gridSelected = gi)}
      onPaste={(id) => paste(id)}
      onContext={openCtxMenu}
      onTogglePin={togglePin}
      onRemove={remove}
    />
        {:else if kindFilter === "files"}
    <GridPanel
      items={topItems}
      kind="files"
      gridSelected={gridSelected}
      confirmDeleteId={confirmDeleteId}
      bindGridEl={(el) => (gridEl = el)}
      onSelect={(gi) => (gridSelected = gi)}
      onPaste={(id) => paste(id)}
      onContext={openCtxMenu}
      onTogglePin={togglePin}
      onRemove={remove}
    />
    {/if}

    {#if kindFilter === "" || kindFilter === "text" || kindFilter === "pinned"}
    <!-- 文本历史（全部/文本/固定 Tab 显示） -->
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
            {kindFilter === "pinned"
              ? "暂无固定内容\n悬停条目点击图钉即可固定"
              : filter
                ? "没有匹配的文本"
                : "暂无文本历史\n复制文字试试"}
          </p>
          {#if kindFilter === "pinned"}
            <button
              class="empty-btn"
              onclick={() => {
                filter = "";
                kindFilter = "";
                reload();
              }}
            >
              去全部页固定
            </button>
          {/if}
        {:else}
          {#if USE_VIRTUAL}
            <div class="list-inner virtual" style="height: {totalHeight}px">
              {#each visibleTextItems as item, vi (item.id)}
                {@const gi = viewStart + vi}
                {@const top = gi === 0 ? 0 : rowOffsets[gi - 1]}
                <TextRow
                  {item}
                  index={gi}
                  selected={gi === selected}
                  {top}
                  {filter}
                  confirmDeleteId={confirmDeleteId}
                  onSelect={() => (selected = gi)}
                  onPaste={(id) => paste(id)}
                  onContext={openCtxMenu}
                  onTogglePin={togglePin}
                  onRemove={remove}
                />
              {/each}
            </div>
          {:else}
            <div class="list-inner flow">
              {#each textItems as item, i (item.id)}
                <TextRow
                  {item}
                  index={i}
                  selected={i === selected}
                  flow
                  {filter}
                  confirmDeleteId={confirmDeleteId}
                  onSelect={() => (selected = i)}
                  onPaste={(id) => paste(id)}
                  onContext={openCtxMenu}
                  onTogglePin={togglePin}
                  onRemove={remove}
                />
              {/each}
            </div>
          {/if}
        {/if}
      </main>
    </section>
    {/if}
  </div>

  <!-- 错误提示 toast（不遮挡列表，3 秒自动消失） -->
  {#if error}
    <div class="toast">{error}</div>
  {/if}

  <!-- 条目右键菜单 -->
  {#if ctxMenu}
    <div
      class="ctx-backdrop"
      role="presentation"
      onclick={closeCtxMenu}
      oncontextmenu={(e) => e.preventDefault()}
      onkeydown={() => {}}
    ></div>
    <div class="ctx-menu" style="left: {ctxMenu.x}px; top: {ctxMenu.y}px">
      <button onclick={() => ctxCopy(ctxMenu!.item)}>
        <ClipboardList size={12} />
        复制
      </button>
      <button onclick={() => {
        const item = ctxMenu!.item;
        closeCtxMenu();
        togglePin(item);
      }}>
        <Pin size={12} />
        {ctxMenu!.item.pinned ? "取消固定" : "固定"}
      </button>
      {#if ctxMenu!.item.kind === "files"}
        <button onclick={() => ctxOpenLocation(ctxMenu!.item)}>
          <Folder size={12} />
          打开所在位置
        </button>
        <button onclick={() => ctxOpenFile(ctxMenu!.item)}>
          <ExternalLink size={12} />
          打开文件
        </button>
      {/if}
      <button class="danger" onclick={() => ctxDelete(ctxMenu!.item)}>
        <X size={12} />
        删除
      </button>
    </div>
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
  /* 二次确认态：红色实心按钮 */
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
  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .icon-btn.active {
    color: var(--accent);
  }
  .icon-btn.danger:hover {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
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

  /* 固定条目视觉：横条卡片描边 */
  .strip-item.pinned {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  /* 条目右键菜单 */
  .ctx-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .ctx-menu {
    position: fixed;
    z-index: 91;
    min-width: 132px;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    padding: 4px;
    display: flex;
    flex-direction: column;
  }
  .ctx-menu button {
    display: flex;
    align-items: center;
    gap: 7px;
    text-align: left;
    padding: 7px 10px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.1s;
  }
  .ctx-menu button:hover {
    background: var(--bg-hover);
  }
  .ctx-menu button.danger {
    color: var(--danger);
  }

  .empty {
    text-align: center;
    color: var(--text-dim);
    font-size: 13px;
    padding: 48px 0 12px;
    white-space: pre-line;
    line-height: 1.8;
  }
  .empty-btn {
    display: block;
    margin: 4px auto 0;
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    font-size: 12px;
    padding: 6px 18px;
    border-radius: 8px;
    cursor: pointer;
    transition: background 0.12s;
  }
  .empty-btn:hover {
    background: var(--accent-soft);
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
