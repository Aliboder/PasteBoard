import { describe, expect, it } from "vitest";
import { splitHighlight, timeLabel, PREVIEW_IMAGE_EXTS } from "./utils";

describe("splitHighlight", () => {
  it("无关键字时返回原文", () => {
    expect(splitHighlight("hello", "")).toEqual([{ t: "hello", m: false }]);
  });

  it("命中关键字标记高亮", () => {
    const parts = splitHighlight("hello world", "lo");
    expect(parts).toEqual([
      { t: "hel", m: false },
      { t: "lo", m: true },
      { t: " world", m: false },
    ]);
  });

  it("大小写不敏感（与 SQL LIKE 一致）", () => {
    const parts = splitHighlight("Hello World", "hello");
    expect(parts[0]).toEqual({ t: "Hello", m: true });
  });

  it("多次命中全部标记", () => {
    const parts = splitHighlight("aba", "a");
    expect(parts.filter((p) => p.m)).toHaveLength(2);
  });

  it("无命中返回原文", () => {
    expect(splitHighlight("abc", "xyz")).toEqual([{ t: "abc", m: false }]);
  });
});

describe("timeLabel", () => {
  it("统一 YYYY/MM/DD HH:mm 格式（两位补零）", () => {
    const d = new Date(2026, 7, 16, 14, 23); // 2026-08-16 14:23
    expect(timeLabel(d.getTime())).toBe("2026/08/16 14:23");
  });

  it("凌晨时间补零", () => {
    const d = new Date(2026, 0, 5, 9, 5); // 2026-01-05 09:05
    expect(timeLabel(d.getTime())).toBe("2026/01/05 09:05");
  });

  it("跨年日期正确", () => {
    const d = new Date(2025, 11, 31, 23, 59); // 2025-12-31 23:59
    expect(timeLabel(d.getTime())).toBe("2025/12/31 23:59");
  });
});

describe("PREVIEW_IMAGE_EXTS", () => {
  it("覆盖常见图片格式", () => {
    for (const ext of ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "avif"]) {
      expect(PREVIEW_IMAGE_EXTS.has(ext)).toBe(true);
    }
  });

  it("不含非图片格式", () => {
    expect(PREVIEW_IMAGE_EXTS.has("docx")).toBe(false);
    expect(PREVIEW_IMAGE_EXTS.has("pdf")).toBe(false);
  });
});
