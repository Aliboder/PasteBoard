import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ItemKind = "text" | "image" | "files";

export interface ItemDto {
  id: number;
  kind: ItemKind;
  preview: string;
  thumb: string | null;
  file_count: number;
  pinned: boolean;
  created_at: number;
}

export interface SettingsDto {
  max_items: number;
}

export async function getHistory(
  filter = "",
  limit = 200,
  offset = 0
): Promise<ItemDto[]> {
  return invoke("get_history", { filter, limit, offset });
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

export async function pasteItem(id: number): Promise<void> {
  return invoke("paste_item", { id });
}

export async function getSettings(): Promise<SettingsDto> {
  return invoke("get_settings");
}

export async function setMaxItems(maxItems: number): Promise<void> {
  return invoke("set_max_items", { maxItems });
}

/** 订阅剪贴板变化事件（新条目 / 上限清理），返回取消订阅函数 */
export async function onChange(cb: () => void): Promise<UnlistenFn> {
  const unlisteners = await Promise.all([
    listen("clipboard://changed", cb),
    listen("clipboard://pruned", cb),
  ]);
  return () => unlisteners.forEach((un) => un());
}
