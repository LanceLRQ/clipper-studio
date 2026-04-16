import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { EncodingPreset, TaskProgressEvent } from "@/types/clip";
import { createClip, cancelClip, listPresets } from "@/services/clip";

interface ClipPanelProps {
  videoId: number;
  /** Current playback time in seconds */
  currentTime: number;
  /** Video duration in seconds */
  duration: number;
  /** Seek callback to control the video player */
  onSeek?: (timeSecs: number) => void;
}

function formatTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0) return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

function parseTime(str: string): number | null {
  const parts = str.split(":").map(Number);
  if (parts.some(isNaN)) return null;
  if (parts.length === 3) return parts[0] * 3600 + parts[1] * 60 + parts[2];
  if (parts.length === 2) return parts[0] * 60 + parts[1];
  return null;
}

export function ClipPanel({ videoId, currentTime, duration: _duration, onSeek }: ClipPanelProps) {
  const [presets, setPresets] = useState<EncodingPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<number | null>(null);
  const [startTime, setStartTime] = useState(0);
  const [endTime, setEndTime] = useState(0);
  const [startInput, setStartInput] = useState("00:00");
  const [endInput, setEndInput] = useState("00:00");
  const [title, setTitle] = useState("");
  const [clipping, setClipping] = useState(false);
  const [currentTaskId, setCurrentTaskId] = useState<number | null>(null);
  const [taskProgress, setTaskProgress] = useState<TaskProgressEvent | null>(null);
  const [previewing, setPreviewing] = useState(false);

  // Load presets
  useEffect(() => {
    listPresets().then((p) => {
      setPresets(p);
      if (p.length > 0) setSelectedPresetId(p[0].id);
    });
  }, []);

  // Listen for task progress events
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | undefined;

    listen<TaskProgressEvent>("task-progress", (event) => {
      setTaskProgress(event.payload);
      if (
        event.payload.status === "completed" ||
        event.payload.status === "failed" ||
        event.payload.status === "cancelled"
      ) {
        setClipping(false);
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlistenFn = fn; }
    });

    return () => {
      cancelled = true;
      unlistenFn?.();
    };
  }, []);

  // Preview: auto-pause when reaching end time
  useEffect(() => {
    if (!previewing) return;
    if (currentTime >= endTime) {
      setPreviewing(false);
      const videoEl = document.querySelector("video");
      if (videoEl) videoEl.pause();
    }
  }, [previewing, currentTime, endTime]);

  const handlePreview = () => {
    if (!onSeek || startTime >= endTime) return;
    onSeek(startTime);
    setPreviewing(true);
    // Start playback after seeking
    requestAnimationFrame(() => {
      const videoEl = document.querySelector("video");
      if (videoEl) videoEl.play();
    });
  };

  const handleStopPreview = () => {
    setPreviewing(false);
    const videoEl = document.querySelector("video");
    if (videoEl) videoEl.pause();
  };

  const handleSetStart = () => {
    setStartTime(currentTime);
    setStartInput(formatTime(currentTime));
  };

  const handleSetEnd = () => {
    setEndTime(currentTime);
    setEndInput(formatTime(currentTime));
  };

  const handleStartInputBlur = () => {
    const t = parseTime(startInput);
    if (t !== null) setStartTime(t);
  };

  const handleEndInputBlur = () => {
    const t = parseTime(endInput);
    if (t !== null) setEndTime(t);
  };

  const handleClip = async () => {
    if (startTime >= endTime) {
      alert("起点时间必须小于终点时间");
      return;
    }

    setClipping(true);
    setTaskProgress(null);
    setCurrentTaskId(null);

    try {
      const task = await createClip({
        video_id: videoId,
        start_ms: Math.round(startTime * 1000),
        end_ms: Math.round(endTime * 1000),
        title: title || undefined,
        preset_id: selectedPresetId,
      });
      setCurrentTaskId(task.id);
    } catch (e) {
      setClipping(false);
      alert("创建切片失败: " + String(e));
    }
  };

  const handleCancel = async () => {
    if (currentTaskId == null) return;
    try {
      await cancelClip(currentTaskId);
    } catch (e) {
      console.error("取消切片失败:", e);
    }
    setClipping(false);
    setCurrentTaskId(null);
  };

  const clipDuration = Math.max(0, endTime - startTime);

  return (
    <div className="rounded-lg border p-4 space-y-4">
      <h3 className="font-medium">切片</h3>

      {/* Time range selection */}
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1">
          <Label className="text-xs">起点</Label>
          <div className="flex gap-1">
            <Input
              value={startInput}
              onChange={(e) => setStartInput(e.target.value)}
              onBlur={handleStartInputBlur}
              className="text-sm h-8"
              placeholder="00:00"
            />
            <Button size="sm" variant="outline" className="h-8 px-2 shrink-0" onClick={handleSetStart}>
              标记
            </Button>
          </div>
        </div>
        <div className="space-y-1">
          <Label className="text-xs">终点</Label>
          <div className="flex gap-1">
            <Input
              value={endInput}
              onChange={(e) => setEndInput(e.target.value)}
              onBlur={handleEndInputBlur}
              className="text-sm h-8"
              placeholder="00:00"
            />
            <Button size="sm" variant="outline" className="h-8 px-2 shrink-0" onClick={handleSetEnd}>
              标记
            </Button>
          </div>
        </div>
      </div>

      {clipDuration > 0 && (
        <div className="flex items-center justify-between">
          <span className="text-xs text-muted-foreground">
            片段时长: {formatTime(clipDuration)}
          </span>
          {onSeek && (
            previewing ? (
              <Button
                size="sm"
                variant="outline"
                className="h-6 px-2 text-xs"
                onClick={handleStopPreview}
              >
                ■ 停止预览
              </Button>
            ) : (
              <Button
                size="sm"
                variant="outline"
                className="h-6 px-2 text-xs"
                onClick={handlePreview}
              >
                ▶ 预览片段
              </Button>
            )
          )}
        </div>
      )}

      {/* Title (optional) */}
      <div className="space-y-1">
        <Label className="text-xs">片段标题（可选）</Label>
        <Input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="留空则自动生成"
          className="text-sm h-8"
        />
      </div>

      {/* Preset selection */}
      <div className="space-y-1">
        <Label className="text-xs">编码预设</Label>
        <select
          value={selectedPresetId ?? ""}
          onChange={(e) => setSelectedPresetId(Number(e.target.value))}
          className="w-full h-8 rounded-md border border-input bg-background px-2 text-sm"
        >
          {presets.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      {/* Progress */}
      {taskProgress && clipping && (
        <div className="space-y-1">
          <div className="flex justify-between items-center text-xs text-muted-foreground">
            <span>{taskProgress.message}</span>
            <div className="flex items-center gap-1.5">
              <span>{Math.round(taskProgress.progress * 100)}%</span>
              <button
                className="px-1 py-0.5 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors text-xs leading-none"
                onClick={handleCancel}
                title="取消切片"
              >
                ✕
              </button>
            </div>
          </div>
          <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-primary transition-all duration-300"
              style={{ width: `${taskProgress.progress * 100}%` }}
            />
          </div>
        </div>
      )}

      {/* Completion message */}
      {taskProgress?.status === "completed" && !clipping && (
        <div className="text-sm text-green-600">✓ 切片完成</div>
      )}
      {taskProgress?.status === "failed" && !clipping && (
        <div className="text-sm text-red-500">
          ✗ 切片失败: {taskProgress.message}
        </div>
      )}

      {/* Clip button */}
      {clipping ? (
        <Button
          className="w-full"
          variant="destructive"
          onClick={handleCancel}
        >
          取消切片
        </Button>
      ) : (
        <Button
          className="w-full"
          onClick={handleClip}
          disabled={clipDuration <= 0}
        >
          开始切片
        </Button>
      )}
    </div>
  );
}
