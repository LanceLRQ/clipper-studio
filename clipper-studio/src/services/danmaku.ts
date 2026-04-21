import { invoke } from "@tauri-apps/api/core";

export interface DanmakuItem {
  time_ms: number;
  text: string;
  mode: "scroll" | "bottom" | "top";
  color: number;
  font_size: number;
}

export interface DanmakuAssOptions {
  width?: number;
  height?: number;
  scroll_time?: number;
  font_size?: number;
  opacity?: number;
  density?: number;
}

export interface DanmakuParseResult {
  items: DanmakuItem[];
  is_truncated: boolean;
  parse_error: string | null;
}

/**
 * 加载视频对应的弹幕。
 *
 * 后端返回 `DanmakuParseResult`（含 items / is_truncated / parse_error），
 * 本函数解包 items 以便多数调用方直接拿到数组；若 XML 不完整则在 console 输出警告。
 */
export async function loadDanmaku(videoId: number): Promise<DanmakuItem[]> {
  const result = await invoke<DanmakuParseResult>("load_danmaku", { videoId });
  if (result.is_truncated) {
    console.warn(
      `[Danmaku] XML 不完整，已解析 ${result.items.length} 条: ${result.parse_error ?? "unknown"}`
    );
  }
  return result.items;
}

/** 需要访问 is_truncated / parse_error 的调用方使用此版本 */
export async function loadDanmakuWithMeta(
  videoId: number
): Promise<DanmakuParseResult> {
  return invoke<DanmakuParseResult>("load_danmaku", { videoId });
}

export async function getDanmakuDensity(
  videoId: number,
  windowMs?: number
): Promise<number[]> {
  return invoke<number[]>("get_danmaku_density", { videoId, windowMs });
}

export async function convertDanmakuToAss(
  videoId: number,
  options?: DanmakuAssOptions
): Promise<string> {
  return invoke<string>("convert_danmaku_to_ass", { videoId, options });
}

export async function checkDanmakuFactory(): Promise<boolean> {
  return invoke<boolean>("check_danmaku_factory");
}
