import { useRef, useEffect, useCallback } from "react";

interface HeatmapBarProps {
  /** Volume values normalized to 0.0 ~ 1.0 */
  envelope: number[];
  /** Video duration in seconds */
  duration: number;
  /** Current playback position in seconds */
  currentTime: number;
  /** Click to seek callback */
  onSeek: (timeSecs: number) => void;
}

/**
 * Color stops for heatmap: gray → yellow → orange → red
 */
function valueToColor(value: number): string {
  if (value < 0.25) {
    // Gray to yellow
    const t = value / 0.25;
    const r = Math.round(128 + t * 127);
    const g = Math.round(128 + t * 72);
    const b = Math.round(128 - t * 128);
    return `rgb(${r},${g},${b})`;
  } else if (value < 0.5) {
    // Yellow to orange
    const t = (value - 0.25) / 0.25;
    const r = 255;
    const g = Math.round(200 - t * 65);
    const b = Math.round(0 + t * 0);
    return `rgb(${r},${g},${b})`;
  } else if (value < 0.75) {
    // Orange to red
    const t = (value - 0.5) / 0.25;
    const r = 255;
    const g = Math.round(135 - t * 100);
    const b = 0;
    return `rgb(${r},${g},${b})`;
  } else {
    // Red (intense)
    const t = (value - 0.75) / 0.25;
    const r = 255;
    const g = Math.round(35 - t * 35);
    const b = 0;
    return `rgb(${r},${g},${b})`;
  }
}

/**
 * Canvas-based audio volume heatmap bar.
 *
 * Displays volume intensity over time with color mapping:
 * gray(quiet) → yellow(moderate) → orange(loud) → red(peak)
 *
 * Features:
 * - Auto-downsample to canvas pixel width
 * - Playback position indicator
 * - Click to seek
 */
export function HeatmapBar({
  envelope,
  duration,
  currentTime,
  onSeek,
}: HeatmapBarProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || envelope.length === 0 || duration <= 0) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;

    ctx.clearRect(0, 0, width, height);

    // Draw heatmap bars
    const barsCount = Math.min(envelope.length, width);
    const barWidth = Math.max(1, width / barsCount);

    for (let i = 0; i < barsCount; i++) {
      // Downsample: pick max value in range
      const startIdx = Math.floor((i / barsCount) * envelope.length);
      const endIdx = Math.floor(((i + 1) / barsCount) * envelope.length);
      let maxVal = 0;
      for (let j = startIdx; j < endIdx && j < envelope.length; j++) {
        maxVal = Math.max(maxVal, envelope[j]);
      }

      const barHeight = maxVal * height;
      const x = i * barWidth;
      const y = height - barHeight;

      ctx.fillStyle = valueToColor(maxVal);
      ctx.fillRect(x, y, barWidth + 0.5, barHeight);
    }

    // Draw playback position indicator
    const posX = (currentTime / duration) * width;
    ctx.fillStyle = "rgba(255, 255, 255, 0.9)";
    ctx.fillRect(posX - 1, 0, 2, height);
  }, [envelope, duration, currentTime]);

  // Resize canvas to container width
  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;

    const observer = new ResizeObserver(() => {
      const rect = container.getBoundingClientRect();
      canvas.width = rect.width * window.devicePixelRatio;
      canvas.height = 40 * window.devicePixelRatio;
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = "40px";
      draw();
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, [draw]);

  // Redraw on data/time change
  useEffect(() => {
    draw();
  }, [draw]);

  const handleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas || duration <= 0) return;

    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const ratio = x / rect.width;
    const seekTime = ratio * duration;
    onSeek(seekTime);
  };

  if (envelope.length === 0) {
    return null;
  }

  return (
    <div
      ref={containerRef}
      className="w-full h-10 bg-muted/30 rounded cursor-pointer"
      title="点击跳转到对应位置"
    >
      <canvas ref={canvasRef} onClick={handleClick} className="rounded" />
    </div>
  );
}
