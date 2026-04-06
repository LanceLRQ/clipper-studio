import { useEffect, useRef, useState, useCallback } from "react";
import type { DanmakuItem } from "@/services/danmaku";

/** Max danmaku rendered per frame to avoid lag */
const MAX_VISIBLE = 300;
/** Default scroll speed: seconds to cross the screen */
const SCROLL_DURATION = 12;
/** Fixed danmaku display duration */
const FIXED_DURATION = 5;

interface DanmakuLayerProps {
  /** All danmaku items */
  items: DanmakuItem[];
  /** Current playback time in seconds */
  currentTime: number;
  /** Video duration in seconds */
  duration: number;
  /** Container width */
  width: number;
  /** Container height */
  height: number;
  /** Whether danmaku is enabled */
  enabled?: boolean;
  /** Opacity 0.0~1.0 */
  opacity?: number;
  /** Font size scale factor */
  fontScale?: number;
}

interface ActiveDanmaku {
  item: DanmakuItem;
  x: number; // current x position (for scroll)
  y: number; // y position (row)
  textWidth: number;
}

export function DanmakuLayer({
  items,
  currentTime,
  duration,
  width,
  height,
  enabled = true,
  opacity = 0.8,
  fontScale = 1.0,
}: DanmakuLayerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const lastTimeRef = useRef(0);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || !enabled || width <= 0 || height <= 0) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = width;
    canvas.height = height;
    ctx.clearRect(0, 0, width, height);
    ctx.globalAlpha = opacity;

    const currentMs = currentTime * 1000;
    const scrollDurationMs = SCROLL_DURATION * 1000;
    const fixedDurationMs = FIXED_DURATION * 1000;

    // Collect visible danmaku within time window
    const visible: ActiveDanmaku[] = [];
    const baseFontSize = Math.round(24 * fontScale);

    // Track occupied rows for fixed danmaku
    const topRows: number[] = [];
    const bottomRows: number[] = [];
    const rowHeight = baseFontSize + 4;

    for (const item of items) {
      if (visible.length >= MAX_VISIBLE) break;

      const elapsed = currentMs - item.time_ms;

      if (item.mode === "scroll") {
        // Scroll: visible from 0 to scrollDuration
        if (elapsed < 0 || elapsed > scrollDurationMs) continue;
        const progress = elapsed / scrollDurationMs;
        const fontSize = Math.round(item.font_size * fontScale * 0.96);
        ctx.font = `bold ${fontSize}px sans-serif`;
        const textWidth = ctx.measureText(item.text).width;
        const x = width - progress * (width + textWidth);

        // Simple row assignment by hash
        const row = Math.abs(hashCode(item.text + item.time_ms)) % Math.max(1, Math.floor(height / rowHeight));
        visible.push({ item, x, y: row * rowHeight + rowHeight, textWidth });
      } else if (item.mode === "top") {
        if (elapsed < 0 || elapsed > fixedDurationMs) continue;
        const row = topRows.length;
        topRows.push(row);
        const y = row * rowHeight + rowHeight;
        if (y > height * 0.5) continue; // Don't overflow half screen
        const fontSize = Math.round(item.font_size * fontScale * 0.96);
        ctx.font = `bold ${fontSize}px sans-serif`;
        const textWidth = ctx.measureText(item.text).width;
        visible.push({ item, x: (width - textWidth) / 2, y, textWidth });
      } else if (item.mode === "bottom") {
        if (elapsed < 0 || elapsed > fixedDurationMs) continue;
        const row = bottomRows.length;
        bottomRows.push(row);
        const y = height - row * rowHeight - 4;
        if (y < height * 0.5) continue;
        const fontSize = Math.round(item.font_size * fontScale * 0.96);
        ctx.font = `bold ${fontSize}px sans-serif`;
        const textWidth = ctx.measureText(item.text).width;
        visible.push({ item, x: (width - textWidth) / 2, y, textWidth });
      }
    }

    // Render
    for (const dm of visible) {
      const fontSize = Math.round(dm.item.font_size * fontScale * 0.96);
      ctx.font = `bold ${fontSize}px sans-serif`;

      // Color from decimal RGB
      const r = (dm.item.color >> 16) & 0xff;
      const g = (dm.item.color >> 8) & 0xff;
      const b = dm.item.color & 0xff;

      // Outline/shadow
      ctx.strokeStyle = "rgba(0,0,0,0.5)";
      ctx.lineWidth = 2;
      ctx.strokeText(dm.item.text, dm.x, dm.y);

      // Fill
      ctx.fillStyle = `rgb(${r},${g},${b})`;
      ctx.fillText(dm.item.text, dm.x, dm.y);
    }
  }, [items, currentTime, width, height, enabled, opacity, fontScale]);

  // Redraw on time change
  useEffect(() => {
    draw();
  }, [draw]);

  if (!enabled) return null;

  return (
    <canvas
      ref={canvasRef}
      className="absolute inset-0 pointer-events-none"
      style={{ width, height }}
    />
  );
}

function hashCode(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i);
    hash |= 0;
  }
  return hash;
}
