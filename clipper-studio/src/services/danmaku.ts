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

export async function loadDanmaku(videoId: number): Promise<DanmakuItem[]> {
  return invoke<DanmakuItem[]>("load_danmaku", { videoId });
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
