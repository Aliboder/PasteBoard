/** 可大预览的图片扩展名（files 类型判断用） */
export const PREVIEW_IMAGE_EXTS = new Set([
  "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "avif", "tif", "tiff",
]);

/** 时间显示：统一 YYYY/MM/DD HH:mm（月/日/时/分两位补零） */
export function timeLabel(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 搜索关键字高亮：把文本切成 [普通, 匹配, 普通...] 片段（大小写不敏感，与 SQL LIKE 一致） */
export function splitHighlight(text: string, keyword: string): { t: string; m: boolean }[] {
  if (!keyword) return [{ t: text, m: false }];
  const lower = text.toLowerCase();
  const kw = keyword.toLowerCase();
  const out: { t: string; m: boolean }[] = [];
  let i = 0;
  while (i < text.length) {
    const idx = lower.indexOf(kw, i);
    if (idx < 0) {
      out.push({ t: text.slice(i), m: false });
      break;
    }
    if (idx > i) out.push({ t: text.slice(i, idx), m: false });
    out.push({ t: text.slice(idx, idx + kw.length), m: true });
    i = idx + kw.length;
  }
  return out;
}
