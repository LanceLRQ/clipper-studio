import {
  useState,
  useRef,
  useCallback,
  useMemo,
  useEffect,
} from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { ClipRegion, ClipOptions, BurnAvailability } from "@/types/multi-clip";
import { CLIP_COLORS, MAX_CLIPS } from "@/lib/clip-colors";
import { autoSegment } from "@/services/clip";
import { ask } from "@tauri-apps/plugin-dialog";

function formatTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

interface ClipTimelineProps {
  duration: number;
  currentTime: number;
  clips: ClipRegion[];
  onCurrentTimeChange?: (time: number) => void;
  onClipsChange?: (clips: ClipRegion[]) => void;
  onClipSelect?: (clipId: string | null) => void;
  selectedClipId?: string | null;
  maxClips?: number;
  /** Per-clip options (offset, audio_only, burn) */
  clipOptions?: Record<string, ClipOptions>;
  onClipOptionsChange?: (options: Record<string, ClipOptions>) => void;
  /** Burn availability info for the current video */
  burnAvailability?: BurnAvailability;
  /** Video ID (needed for auto-segment) */
  videoId?: number;
  /** Called when clips are created via "新增" or "自动分段" */
  onClipCreated?: () => void;
  /** Disable clip creation/editing (e.g. ffprobe missing) */
  disabled?: boolean;
}

interface DragState {
  clipId: string;
  type: "move" | "left" | "right";
  startX: number;
  startValue: number;
  clip: ClipRegion;
}

export function ClipTimeline({
  duration = 0,
  currentTime = 0,
  clips = [],
  onCurrentTimeChange,
  onClipsChange,
  onClipSelect,
  selectedClipId = null,
  maxClips = MAX_CLIPS,
  clipOptions = {},
  onClipOptionsChange,
  burnAvailability,
  videoId,
  onClipCreated,
  disabled = false,
}: ClipTimelineProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState(1);
  const [containerWidth, setContainerWidth] = useState(800);
  const [isDraggingPlayhead, setIsDraggingPlayhead] = useState(false);
  const [dragState, setDragState] = useState<DragState | null>(null);
  const clipsRef = useRef(clips);
  clipsRef.current = clips;
  const onClipsChangeRef = useRef(onClipsChange);
  onClipsChangeRef.current = onClipsChange;
  const onCurrentTimeChangeRef = useRef(onCurrentTimeChange);
  onCurrentTimeChangeRef.current = onCurrentTimeChange;

  // Track container width via ResizeObserver
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const w = entry.contentRect.width;
        if (w > 0) setContainerWidth(w);
      }
    });
    ro.observe(container);
    return () => ro.disconnect();
  }, []);

  const timelineWidth = containerWidth * zoom;

  const timeToPixel = useCallback(
    (time: number) => (duration <= 0 ? 0 : (time / duration) * timelineWidth),
    [duration, timelineWidth]
  );

  const pixelToTime = useCallback(
    (pixel: number) =>
      timelineWidth <= 0
        ? 0
        : Math.max(0, Math.min(duration, (pixel / timelineWidth) * duration)),
    [duration, timelineWidth]
  );

  // Dynamic tick interval based on zoom
  const tickInterval = useMemo(() => {
    const pps = timelineWidth / duration;
    if (zoom < 5) {
      if (pps > 10) return 30;
      if (pps > 5) return 90;
      if (pps > 2) return 180;
      if (pps > 1) return 300;
      return 600;
    }
    if (pps > 20) return 5;
    if (pps > 10) return 10;
    if (pps > 5) return 30;
    if (pps > 2) return 90;
    if (pps > 1) return 180;
    return 300;
  }, [timelineWidth, duration, zoom]);

  const ticks = useMemo(() => {
    if (duration <= 0) return [];
    const result: number[] = [];
    for (let t = 0; t <= duration; t += tickInterval) result.push(t);
    return result;
  }, [duration, tickInterval]);

  const selectedClip = useMemo(
    () => clips.find((c) => c.id === selectedClipId) ?? null,
    [clips, selectedClipId]
  );

  const getDefaultClipDuration = useCallback(() => {
    const visible = duration / zoom;
    return Math.max(Math.round(visible * 0.1), 1);
  }, [duration, zoom]);

  // ===== Actions =====

  const handleAddClip = useCallback(() => {
    if (disabled) return;
    if (clips.length >= maxClips) {
      showToast(`最多只能创建 ${maxClips} 个选区`);
      return;
    }
    if (duration <= 0) return;
    const start = Math.round(currentTime);
    const dur = getDefaultClipDuration();
    const end = Math.min(start + dur, duration);
    const effectiveStart = end <= start ? Math.max(0, end - dur) : start;
    if (end <= effectiveStart) return;
    const newClip: ClipRegion = {
      id: `clip-${Date.now()}`,
      start: effectiveStart,
      end,
      color: CLIP_COLORS[clips.length % CLIP_COLORS.length],
      name: `片段${clips.length + 1}`,
    };
    onClipsChange?.([...clips, newClip]);
    onClipSelect?.(newClip.id);
    onClipCreated?.();
  }, [clips, currentTime, duration, maxClips, getDefaultClipDuration, onClipsChange, onClipSelect, onClipCreated]);

  const handleCopyClip = useCallback(() => {
    if (!selectedClipId || clips.length >= maxClips) return;
    const clip = clips.find((c) => c.id === selectedClipId);
    if (!clip) return;
    const newClip: ClipRegion = {
      id: `clip-${Date.now()}`,
      start: clip.start,
      end: clip.end,
      color: CLIP_COLORS[clips.length % CLIP_COLORS.length],
      name: `片段${clips.length + 1}`,
    };
    onClipsChange?.([...clips, newClip]);
    onClipSelect?.(newClip.id);
  }, [clips, selectedClipId, maxClips, onClipsChange, onClipSelect]);

  const [toast, setToast] = useState<string | null>(null);
  const showToast = useCallback((msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 2000);
  }, []);

  const handleSetClipStart = useCallback(() => {
    if (!selectedClipId) {
      showToast("请先选择一个片段");
      return;
    }
    const clip = clips.find((c) => c.id === selectedClipId);
    if (!clip) return;
    const newStart = Math.round(currentTime);
    if (clip.end > 0 && newStart >= clip.end) {
      showToast("起点不能大于等于终点");
      return;
    }
    onClipsChange?.(clips.map((c) => (c.id === selectedClipId ? { ...c, start: newStart } : c)));
  }, [selectedClipId, clips, currentTime, onClipsChange, showToast]);

  const handleSetClipEnd = useCallback(() => {
    if (!selectedClipId) {
      showToast("请先选择一个片段");
      return;
    }
    const clip = clips.find((c) => c.id === selectedClipId);
    if (!clip) return;
    const newEnd = Math.round(currentTime);
    if (clip.start > 0 && newEnd <= clip.start) {
      showToast("终点不能小于等于起点");
      return;
    }
    onClipsChange?.(clips.map((c) => (c.id === selectedClipId ? { ...c, end: newEnd } : c)));
  }, [selectedClipId, clips, currentTime, onClipsChange, showToast]);

  const handleDeleteClip = useCallback(async () => {
    if (!selectedClipId) return;
    if (!(await ask("确定删除该选区？", { title: "删除选区", kind: "warning" }))) return;
    onClipsChange?.(clips.filter((c) => c.id !== selectedClipId));
    onClipSelect?.(null);
  }, [clips, selectedClipId, onClipsChange, onClipSelect]);

  const [autoSegLoading, setAutoSegLoading] = useState(false);

  const handleAutoSegment = useCallback(async () => {
    if (!videoId) return;
    if (clips.length > 0 && !(await ask("自动分段将替换现有选区，确认？", { title: "自动分段", kind: "warning" }))) return;

    setAutoSegLoading(true);
    try {
      const segments = await autoSegment(videoId);
      if (segments.length === 0) {
        showToast("未检测到有效分段");
        return;
      }
      const newClips: ClipRegion[] = segments
        .slice(0, maxClips)
        .map((seg, i) => ({
          id: `clip-${Date.now()}-${i}`,
          start: seg.start_ms / 1000,
          end: seg.end_ms / 1000,
          color: CLIP_COLORS[i % CLIP_COLORS.length],
          name: `片段${i + 1}`,
        }));
      onClipsChange?.(newClips);
      onClipSelect?.(newClips[0]?.id ?? null);
      onClipCreated?.();
      showToast(`检测到 ${newClips.length} 个片��`);
    } catch (e) {
      showToast("自动分段失败: " + String(e));
    } finally {
      setAutoSegLoading(false);
    }
  }, [videoId, clips, maxClips, onClipsChange, onClipSelect, onClipCreated, showToast]);

  // ===== Track interactions =====

  const handleTrackClick = useCallback(
    (e: React.MouseEvent) => {
      if (dragState || isDraggingPlayhead) return;
      const rect = trackRef.current?.getBoundingClientRect();
      if (!rect) return;
      onCurrentTimeChange?.(Math.round(pixelToTime(e.clientX - rect.left)));
    },
    [dragState, isDraggingPlayhead, pixelToTime, onCurrentTimeChange]
  );

  const handleTrackDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (disabled) return;
      if (dragState || isDraggingPlayhead) return;
      if (clips.length >= maxClips) return;
      if (duration <= 0) return;
      const rect = trackRef.current?.getBoundingClientRect();
      if (!rect) return;
      const clickTime = Math.round(pixelToTime(e.clientX - rect.left));
      const dur = getDefaultClipDuration();
      const end = Math.min(clickTime + dur, duration);
      const start = end <= clickTime ? Math.max(0, end - dur) : clickTime;
      if (end <= start) return;
      const newClip: ClipRegion = {
        id: `clip-${Date.now()}`,
        start,
        end,
        color: CLIP_COLORS[clips.length % CLIP_COLORS.length],
        name: `片段${clips.length + 1}`,
      };
      onClipsChange?.([...clips, newClip]);
      onClipSelect?.(newClip.id);
      onClipCreated?.();
    },
    [clips, duration, maxClips, dragState, isDraggingPlayhead, getDefaultClipDuration, pixelToTime, onClipsChange, onClipSelect, onClipCreated]
  );

  const handlePlayheadMouseDown = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    setIsDraggingPlayhead(true);
  }, []);

  const handleClipDragStart = useCallback(
    (e: React.MouseEvent, clipId: string, type: "move" | "left" | "right") => {
      e.stopPropagation();
      const clip = clips.find((c) => c.id === clipId);
      if (!clip) return;
      setDragState({
        clipId,
        type,
        startX: e.clientX,
        startValue: type === "right" ? clip.end : clip.start,
        clip: { ...clip },
      });
      onClipSelect?.(clipId);
    },
    [clips, onClipSelect]
  );

  // Global mousemove/mouseup for drag
  useEffect(() => {
    if (!isDraggingPlayhead && !dragState) return;

    const handleMouseMove = (e: MouseEvent) => {
      const rect = trackRef.current?.getBoundingClientRect();
      if (!rect) return;

      if (isDraggingPlayhead) {
        onCurrentTimeChangeRef.current?.(Math.round(pixelToTime(e.clientX - rect.left)));
      }

      if (dragState) {
        const deltaX = e.clientX - dragState.startX;
        const deltaTime = (deltaX / timelineWidth) * duration;
        const clip = dragState.clip;
        const newClip = { ...clip };

        if (dragState.type === "move") {
          const clipDur = clip.end - clip.start;
          let newStart = Math.max(0, clip.start + deltaTime);
          let newEnd = newStart + clipDur;
          if (newEnd > duration) {
            newEnd = duration;
            newStart = newEnd - clipDur;
          }
          newClip.start = Math.round(newStart);
          newClip.end = Math.round(newEnd);
        } else if (dragState.type === "left") {
          newClip.start = Math.round(Math.max(0, Math.min(clip.end - 1, clip.start + deltaTime)));
        } else if (dragState.type === "right") {
          newClip.end = Math.round(Math.min(duration, Math.max(clip.start + 1, clip.end + deltaTime)));
        }

        onClipsChangeRef.current?.(clipsRef.current.map((c) => (c.id === dragState.clipId ? newClip : c)));
      }
    };

    const handleMouseUp = () => {
      setIsDraggingPlayhead(false);
      setDragState(null);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDraggingPlayhead, dragState, duration, timelineWidth, pixelToTime]);

  const handleZoomIn = () => setZoom((z) => Math.min(z * 1.5, 10));
  const handleZoomOut = () => setZoom((z) => Math.max(z / 1.5, 1));

  return (
    <div className="w-full select-none rounded-lg border p-3 space-y-2 relative">
      {/* Toast notification */}
      {toast && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 z-30 px-3 py-1.5 rounded-md bg-foreground text-background text-xs shadow-lg animate-in fade-in">
          {toast}
        </div>
      )}

      {/* Toolbar row 1: selected clip info + action buttons */}
      <div className="flex items-center justify-between text-sm">
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground text-xs">选区：</span>
          {selectedClip ? (
            <>
              <span
                className="inline-block w-3 h-3 rounded-sm"
                style={{ backgroundColor: selectedClip.color }}
              />
              <span className="font-medium" style={{ color: selectedClip.color }}>
                {selectedClip.name}
              </span>
              {selectedClip.end > selectedClip.start ? (
                <span className="text-muted-foreground text-xs">
                  <button
                    className="hover:underline"
                    onClick={() => onCurrentTimeChange?.(selectedClip.start)}
                  >
                    {formatTime(selectedClip.start)}
                  </button>
                  {" - "}
                  <button
                    className="hover:underline"
                    onClick={() => onCurrentTimeChange?.(selectedClip.end)}
                  >
                    {formatTime(selectedClip.end)}
                  </button>
                </span>
              ) : (
                <span className="text-xs text-yellow-600">未设置时间</span>
              )}
            </>
          ) : (
            <span className="text-muted-foreground text-xs">请在下方选择或新增选区</span>
          )}
        </div>
        <div className="flex gap-1">
          <Button
            size="sm"
            variant="outline"
            className="h-7 px-2 text-xs"
            onClick={handleSetClipStart}
            disabled={!selectedClipId || disabled}
            title="将播放头位置设为起点"
          >
            起点
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 px-2 text-xs"
            onClick={handleSetClipEnd}
            disabled={!selectedClipId || disabled}
            title="将播放头位置设为终点"
          >
            终点
          </Button>
          <Button
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={handleAddClip}
            disabled={clips.length >= maxClips || disabled}
            title="添加新选区"
          >
            + 新增
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 px-2 text-xs"
            onClick={handleCopyClip}
            disabled={
              !selectedClipId ||
              !selectedClip ||
              selectedClip.end <= selectedClip.start ||
              clips.length >= maxClips ||
              disabled
            }
            title="复制当前选区"
          >
            复制
          </Button>
          {selectedClipId && (
            <Button
              size="sm"
              variant="destructive"
              className="h-7 px-2 text-xs"
              onClick={handleDeleteClip}
            >
              删除
            </Button>
          )}
          <div className="mx-1 h-4 border-l" />
          <Button
            size="sm"
            variant="outline"
            className="h-7 px-2 text-xs"
            onClick={handleAutoSegment}
            disabled={autoSegLoading || !videoId || disabled}
            title="根据音量自动检测分段（需要先提取音量包络）"
          >
            {autoSegLoading ? "分析中..." : "自动分段"}
          </Button>
        </div>
      </div>

      {/* Toolbar row 2: zoom controls */}
      <div className="flex items-center gap-2 text-xs">
        <Button
          size="sm"
          variant="ghost"
          className="h-6 w-6 p-0"
          onClick={handleZoomOut}
          disabled={zoom <= 1}
        >
          -
        </Button>
        <input
          type="range"
          min={1}
          max={10}
          step={0.5}
          value={zoom}
          onChange={(e) => setZoom(Number(e.target.value))}
          className="w-24 h-1 accent-primary"
        />
        <Button
          size="sm"
          variant="ghost"
          className="h-6 w-6 p-0"
          onClick={handleZoomIn}
          disabled={zoom >= 10}
        >
          +
        </Button>
        <span className="text-muted-foreground">{Math.round(zoom * 100)}%</span>
      </div>

      {/* Timeline container */}
      <div
        ref={containerRef}
        className="overflow-x-auto border rounded bg-muted/20"
      >
        <div style={{ width: timelineWidth, minWidth: "100%" }}>
          {/* Tick ruler */}
          <div className="relative h-5 border-b bg-background">
            {ticks.map((t) => (
              <div
                key={t}
                className="absolute top-0 flex flex-col items-center"
                style={{ left: timeToPixel(t) }}
              >
                <div className="w-px h-1.5 bg-muted-foreground/40" />
                <span className="text-[10px] text-muted-foreground whitespace-nowrap">
                  {formatTime(t)}
                </span>
              </div>
            ))}
          </div>

          {/* Track */}
          <div
            ref={trackRef}
            className="relative h-14 bg-muted/30 cursor-crosshair"
            onClick={handleTrackClick}
            onDoubleClick={handleTrackDoubleClick}
          >
            {/* Show selected clip region */}
            {selectedClip && selectedClip.end > selectedClip.start && (
              <ClipRegionBlock
                clip={selectedClip}
                isSelected
                left={timeToPixel(selectedClip.start)}
                width={timeToPixel(selectedClip.end) - timeToPixel(selectedClip.start)}
                onDragStart={handleClipDragStart}
              />
            )}

            {/* Playhead */}
            <div
              className="absolute top-0 bottom-0 w-0.5 bg-red-500 cursor-ew-resize z-20"
              style={{ left: timeToPixel(currentTime) }}
              onMouseDown={handlePlayheadMouseDown}
            >
              <div
                className="absolute -top-1 left-1/2 -translate-x-1/2 w-0 h-0"
                style={{
                  borderLeft: "6px solid transparent",
                  borderRight: "6px solid transparent",
                  borderTop: "8px solid #ef4444",
                }}
              />
            </div>
          </div>
        </div>
      </div>

      {/* Selected clip options: offset + audio only (horizontal) */}
      {selectedClip && onClipOptionsChange && (
        <div className="flex items-center gap-4 text-xs text-muted-foreground">
          <label className="flex items-center gap-1">
            前移
            <Input
              type="number"
              min={0}
              max={300}
              value={clipOptions[selectedClip.id]?.clip_offset_before ?? 0}
              onChange={(e) => {
                const val = Number(e.target.value) || 0;
                onClipOptionsChange({
                  ...clipOptions,
                  [selectedClip.id]: {
                    ...(clipOptions[selectedClip.id] ?? { clip_offset_before: 0, clip_offset_after: 0, audio_only: false, include_danmaku: false, include_subtitle: false, export_subtitle: false, export_danmaku: false }),
                    clip_offset_before: val,
                  },
                });
              }}
              className="w-14 h-6 text-xs text-center"
            />
            秒
          </label>
          <label className="flex items-center gap-1">
            后延
            <Input
              type="number"
              min={0}
              max={300}
              value={clipOptions[selectedClip.id]?.clip_offset_after ?? 0}
              onChange={(e) => {
                const val = Number(e.target.value) || 0;
                onClipOptionsChange({
                  ...clipOptions,
                  [selectedClip.id]: {
                    ...(clipOptions[selectedClip.id] ?? { clip_offset_before: 0, clip_offset_after: 0, audio_only: false, include_danmaku: false, include_subtitle: false, export_subtitle: false, export_danmaku: false }),
                    clip_offset_after: val,
                  },
                });
              }}
              className="w-14 h-6 text-xs text-center"
            />
            秒
          </label>
          <label className="flex items-center gap-1.5 cursor-pointer">
            <input
              type="checkbox"
              checked={clipOptions[selectedClip.id]?.audio_only ?? false}
              onChange={(e) => {
                const opts = clipOptions[selectedClip.id] ?? { clip_offset_before: 0, clip_offset_after: 0, audio_only: false, include_danmaku: false, include_subtitle: false, export_subtitle: false, export_danmaku: false };
                onClipOptionsChange({
                  ...clipOptions,
                  [selectedClip.id]: {
                    ...opts,
                    audio_only: e.target.checked,
                    // Disable burn options when audio_only
                    ...(e.target.checked ? { include_danmaku: false, include_subtitle: false } : {}),
                  },
                });
              }}
              className="rounded"
            />
            仅音频
          </label>
          <label
            className={`flex items-center gap-1.5 ${
              (clipOptions[selectedClip.id]?.audio_only) || !burnAvailability?.has_danmaku_xml || !burnAvailability?.has_danmaku_factory
                ? "opacity-40 cursor-not-allowed"
                : "cursor-pointer"
            }`}
            title={
              !burnAvailability?.has_danmaku_factory
                ? "需要安装弹幕转换工具"
                : !burnAvailability?.has_danmaku_xml
                  ? "该视频没有弹幕文件"
                  : clipOptions[selectedClip.id]?.audio_only
                    ? "仅音频模式下不可用"
                    : undefined
            }
          >
            <input
              type="checkbox"
              checked={clipOptions[selectedClip.id]?.include_danmaku ?? false}
              disabled={
                (clipOptions[selectedClip.id]?.audio_only) ||
                !burnAvailability?.has_danmaku_xml ||
                !burnAvailability?.has_danmaku_factory
              }
              onChange={(e) => {
                onClipOptionsChange({
                  ...clipOptions,
                  [selectedClip.id]: {
                    ...(clipOptions[selectedClip.id] ?? { clip_offset_before: 0, clip_offset_after: 0, audio_only: false, include_danmaku: false, include_subtitle: false, export_subtitle: false, export_danmaku: false }),
                    include_danmaku: e.target.checked,
                  },
                });
              }}
              className="rounded"
            />
            烧录弹幕
          </label>
          <label
            className={`flex items-center gap-1.5 ${
              (clipOptions[selectedClip.id]?.audio_only) || !burnAvailability?.has_subtitle
                ? "opacity-40 cursor-not-allowed"
                : "cursor-pointer"
            }`}
            title={
              !burnAvailability?.has_subtitle
                ? "该视频没有字幕"
                : clipOptions[selectedClip.id]?.audio_only
                  ? "仅音频模式下不可用"
                  : undefined
            }
          >
            <input
              type="checkbox"
              checked={clipOptions[selectedClip.id]?.include_subtitle ?? false}
              disabled={
                (clipOptions[selectedClip.id]?.audio_only) ||
                !burnAvailability?.has_subtitle
              }
              onChange={(e) => {
                onClipOptionsChange({
                  ...clipOptions,
                  [selectedClip.id]: {
                    ...(clipOptions[selectedClip.id] ?? { clip_offset_before: 0, clip_offset_after: 0, audio_only: false, include_danmaku: false, include_subtitle: false, export_subtitle: false, export_danmaku: false }),
                    include_subtitle: e.target.checked,
                  },
                });
              }}
              className="rounded"
            />
            烧录字幕
          </label>
          <label
            className={`flex items-center gap-1.5 ${
              !burnAvailability?.has_danmaku_xml
                ? "opacity-40 cursor-not-allowed"
                : "cursor-pointer"
            }`}
            title={
              !burnAvailability?.has_danmaku_xml
                ? "该视频没有弹幕文件"
                : undefined
            }
          >
            <input
              type="checkbox"
              checked={clipOptions[selectedClip.id]?.export_danmaku ?? false}
              disabled={!burnAvailability?.has_danmaku_xml}
              onChange={(e) => {
                onClipOptionsChange({
                  ...clipOptions,
                  [selectedClip.id]: {
                    ...(clipOptions[selectedClip.id] ?? { clip_offset_before: 0, clip_offset_after: 0, audio_only: false, include_danmaku: false, include_subtitle: false, export_subtitle: false, export_danmaku: false }),
                    export_danmaku: e.target.checked,
                  },
                });
              }}
              className="rounded"
            />
            导出弹幕
          </label>
          <label
            className={`flex items-center gap-1.5 ${
              !burnAvailability?.has_subtitle
                ? "opacity-40 cursor-not-allowed"
                : "cursor-pointer"
            }`}
            title={
              !burnAvailability?.has_subtitle
                ? "该视频没有字幕"
                : undefined
            }
          >
            <input
              type="checkbox"
              checked={clipOptions[selectedClip.id]?.export_subtitle ?? false}
              disabled={!burnAvailability?.has_subtitle}
              onChange={(e) => {
                onClipOptionsChange({
                  ...clipOptions,
                  [selectedClip.id]: {
                    ...(clipOptions[selectedClip.id] ?? { clip_offset_before: 0, clip_offset_after: 0, audio_only: false, include_danmaku: false, include_subtitle: false, export_subtitle: false, export_danmaku: false }),
                    export_subtitle: e.target.checked,
                  },
                });
              }}
              className="rounded"
            />
            导出字幕
          </label>
        </div>
      )}
    </div>
  );
}

/** Single clip region block on the timeline track */
function ClipRegionBlock({
  clip,
  isSelected,
  left,
  width,
  onDragStart,
}: {
  clip: ClipRegion;
  isSelected: boolean;
  left: number;
  width: number;
  onDragStart: (e: React.MouseEvent, clipId: string, type: "move" | "left" | "right") => void;
}) {
  return (
    <div
      className={`absolute top-1 bottom-1 rounded cursor-move transition-shadow ${
        isSelected ? "ring-2 ring-white shadow-lg z-10" : "hover:shadow-md"
      }`}
      style={{
        left,
        width: Math.max(width, 20),
        backgroundColor: clip.color + "cc",
      }}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => onDragStart(e, clip.id, "move")}
    >
      {/* Left drag handle */}
      <div
        className="absolute left-0 top-0 bottom-0 w-2 cursor-ew-resize hover:bg-white/30"
        onMouseDown={(e) => onDragStart(e, clip.id, "left")}
      />
      {/* Label */}
      <div className="absolute inset-2 flex items-center justify-center overflow-hidden">
        <span className="text-white text-xs font-medium truncate drop-shadow">
          {clip.name}
        </span>
      </div>
      {/* Right drag handle */}
      <div
        className="absolute right-0 top-0 bottom-0 w-2 cursor-ew-resize hover:bg-white/30"
        onMouseDown={(e) => onDragStart(e, clip.id, "right")}
      />
    </div>
  );
}
