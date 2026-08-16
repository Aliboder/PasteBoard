import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ItemKind = "text" | "image" | "files" | "pinned";

export interface ItemDto {
  id: number;
  kind: ItemKind;
  preview: string;
  full: string | null;
  thumb: string | null;
  file_count: number;
  pinned: boolean;
  created_at: number;
}

export interface SettingsDto {
  max_items: number;
  theme: string;
  hotkey: string;
  follow_mouse: string;
  keep_open: string;
  always_on_top: string;
  win_w: number;
  win_h: number;
  autostart: boolean;
}

export async function getHistory(
  filter = "",
  kind: ItemKind | "" = "",
  limit = 200,
  offset = 0
): Promise<ItemDto[]> {
  return invoke("get_history", { filter, kind: kind || null, limit, offset });
}

export async function pinItem(id: number, pinned: boolean): Promise<boolean> {
  return invoke("pin_item", { id, pinned });
}

export async function deleteItem(id: number): Promise<boolean> {
  return invoke("delete_item", { id });
}

export async function clearHistory(): Promise<number> {
  return invoke("clear_history");
}

export async function clearAllHistory(): Promise<number> {
  return invoke("clear_all_history");
}

export async function pasteItem(id: number): Promise<void> {
  return invoke("paste_item", { id });
}

export async function copyItem(id: number): Promise<void> {
  return invoke("copy_item", { id });
}

export async function openFileLocation(path: string): Promise<void> {
  return invoke("open_file_location", { path });
}

export async function openFile(path: string): Promise<void> {
  return invoke("open_file", { path });
}

export async function getImage(id: number): Promise<string | null> {
  return invoke("get_image", { id });
}

export async function getThumb(id: number): Promise<string | null> {
  return invoke("get_thumb", { id });
}

export async function getFileIcon(path: string): Promise<string | null> {
  return invoke("get_file_icon", { path });
}

export async function getFileThumb(path: string): Promise<string | null> {
  return invoke("get_file_thumb", { path });
}

export async function getFilePreview(path: string): Promise<string | null> {
  return invoke("get_file_preview", { path });
}

export async function getSettings(): Promise<SettingsDto> {
  return invoke("get_settings");
}

export async function setMaxItems(maxItems: number): Promise<void> {
  return invoke("set_max_items", { maxItems });
}

export async function setTheme(theme: string): Promise<void> {
  return invoke("set_theme", { theme });
}

export async function setToggle(key: string, value: "on" | "off"): Promise<void> {
  return invoke("set_toggle", { key, value });
}

export async function setAutostart(enabled: boolean): Promise<void> {
  return invoke("set_autostart", { enabled });
}

export async function setWindowSize(w: number, h: number): Promise<void> {
  return invoke("set_window_size", { w, h });
}

export async function openDataDir(): Promise<void> {
  return invoke("open_data_dir");
}

export interface StatsDto {
  total: number;
  text: number;
  image: number;
  files: number;
  db_size: number;
  media_size: number;
}

export async function getStats(): Promise<StatsDto> {
  return invoke("get_stats");
}

export async function resetSettings(): Promise<void> {
  return invoke("reset_settings");
}

export async function setHotkey(hotkey: string): Promise<void> {
  return invoke("set_hotkey", { hotkey });
}

/** 订阅剪贴板变化事件（新条目 / 上限清理），返回取消订阅函数 */
export async function onChange(cb: () => void): Promise<UnlistenFn> {
  const unlisteners = await Promise.all([
    listen("clipboard://changed", cb),
    listen("clipboard://pruned", cb),
  ]);
  return () => unlisteners.forEach((un) => un());
}
