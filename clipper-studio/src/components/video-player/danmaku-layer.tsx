import { useEffect, useRef } from "react";
import Danmaku from "danmaku";
import type { DanmakuItem } from "@/services/danmaku";

interface DanmakuLayerProps {
  /** Container div for danmaku overlay */
  container: HTMLDivElement | null;
  /** Underlying <video> element for media sync */
  media: HTMLVideoElement | null;
  /** All danmaku items from backend */
  items: DanmakuItem[];
  /** Whether danmaku is enabled */
  enabled?: boolean;
  /** Opacity 0.0~1.0 */
  opacity?: number;
  /** Font size scale factor */
  fontScale?: number;
}

/** Convert decimal RGB color to hex string */
function colorToHex(color: number): string {
  const r = (color >> 16) & 0xff;
  const g = (color >> 8) & 0xff;
  const b = color & 0xff;
  return `#${r.toString(16).padStart(2, "0")}${g.toString(16).padStart(2, "0")}${b.toString(16).padStart(2, "0")}`;
}

/** Danmaku comment format accepted by the library */
interface DanmakuComment {
  text: string;
  time: number;
  mode: "rtl" | "top" | "bottom";
  style: {
    font: string;
    fillStyle: string;
    strokeStyle: string;
    lineWidth: number;
    textBaseline: CanvasTextBaseline;
  };
}

/** Convert our DanmakuItem[] to library Comment[] format */
function toComments(items: DanmakuItem[], fontScale: number): DanmakuComment[] {
  return items.map((item) => {
    const modeMap: Record<string, "rtl" | "top" | "bottom"> = {
      scroll: "rtl",
      top: "top",
      bottom: "bottom",
    };
    const fontSize = Math.round(item.font_size * fontScale);
    return {
      text: item.text,
      time: item.time_ms / 1000,
      mode: modeMap[item.mode] || "rtl",
      style: {
        font: `bold ${fontSize}px sans-serif`,
        fillStyle: colorToHex(item.color),
        strokeStyle: "rgba(0,0,0,0.5)",
        lineWidth: 2,
        textBaseline: "bottom" as CanvasTextBaseline,
      },
    };
  });
}

export function DanmakuLayer({
  container,
  media,
  items,
  enabled = true,
  fontScale = 1.0,
}: DanmakuLayerProps) {
  const danmakuRef = useRef<Danmaku | null>(null);

  // Create / destroy Danmaku instance when container or media changes
  useEffect(() => {
    console.log("[DanmakuLayer] effect:", {
      container: !!container,
      media: !!media,
      items: items.length,
    });
    if (!container || !media) return;

    const comments = toComments(items, fontScale);
    console.log("[DanmakuLayer] Creating instance:", {
      commentsCount: comments.length,
      containerSize: `${container.clientWidth}x${container.clientHeight}`,
      firstComment: comments[0],
      mediaSrc: media.src,
      mediaPaused: media.paused,
      mediaCurrentTime: media.currentTime,
    });

    const dm = new Danmaku({
      container,
      media,
      comments,
      engine: "canvas",
      speed: 144,
    });

    console.log("[DanmakuLayer] Instance created, stage:", (dm as { container?: { innerHTML?: { length: number } } }).container?.innerHTML?.length, "chars");

    danmakuRef.current = dm;

    return () => {
      dm.destroy();
      danmakuRef.current = null;
    };
  }, [container, media, items, fontScale]);

  // Show / hide
  useEffect(() => {
    const dm = danmakuRef.current;
    if (!dm) return;
    enabled ? dm.show() : dm.hide();
  }, [enabled]);

  // Resize when container dimensions change
  useEffect(() => {
    const dm = danmakuRef.current;
    if (!dm || !container) return;
    dm.resize();
  }, [container]);

  return null;
}
