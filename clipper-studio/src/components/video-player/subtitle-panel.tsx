import { useEffect, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { SubtitleSegment, ASRTaskInfo, ASRServiceStatusInfo } from "@/services/asr";
import {
  submitASR,
  pollASR,
  listSubtitles,
  listASRTasks,
  checkASRHealth,
  getASRServiceStatus,
  exportSubtitlesSrt,
  exportSubtitlesAss,
  exportSubtitlesVtt,
} from "@/services/asr";
import { getSettings } from "@/services/settings";

interface SubtitlePanelProps {
  videoId: number;
  /** Current playback time in seconds */
  currentTime: number;
  /** Seek callback */
  onSeek?: (timeSecs: number) => void;
  /** Set clip start/end time */
  onSetClipStart?: (timeSecs: number) => void;
  onSetClipEnd?: (timeSecs: number) => void;
}

function formatTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  const sStr = s.toFixed(1).padStart(4, "0");
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${sStr}`;
  return `${m.toString().padStart(2, "0")}:${sStr}`;
}

export function SubtitlePanel({
  videoId,
  currentTime,
  onSeek,
  onSetClipStart,
  onSetClipEnd,
}: SubtitlePanelProps) {
  const [segments, setSegments] = useState<SubtitleSegment[]>([]);
  const [baseTimeMs, setBaseTimeMs] = useState(0);
  const [asrTask, setAsrTask] = useState<ASRTaskInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const activeRef = useRef<HTMLDivElement>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // ASR service availability
  const [asrMode, setAsrMode] = useState<string>("local");
  const [serviceStatus, setServiceStatus] = useState<ASRServiceStatusInfo | null>(null);

  // Whether ASR is available for use
  const asrAvailable =
    asrMode === "disabled" ? false
    : asrMode === "remote" ? true  // remote mode always allows attempt
    : serviceStatus?.status === "running";  // local mode requires running service

  const loadSubtitles = async () => {
    const resp = await listSubtitles(videoId);
    setSegments(resp.segments);
    setBaseTimeMs(resp.base_ms);
  };

  // Load ASR mode and service status
  useEffect(() => {
    getSettings(["asr_mode"]).then((s) => {
      if (s.asr_mode) setAsrMode(s.asr_mode);
    }).catch(console.error);

    getASRServiceStatus().then(setServiceStatus).catch(console.error);
  }, []);

  // Listen for real-time service status changes
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<ASRServiceStatusInfo>("asr-service-status", (event) => {
      setServiceStatus(event.payload);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  // Load subtitles and ASR task status
  useEffect(() => {
    loadSubtitles().catch(console.error);
    listASRTasks(videoId).then((tasks) => {
      const active = tasks.find(
        (t) => t.status === "processing" || t.status === "pending"
      );
      if (active) setAsrTask(active);
    }).catch(console.error);
  }, [videoId]);

  // Poll ASR task progress
  useEffect(() => {
    if (!asrTask || (asrTask.status !== "processing" && asrTask.status !== "pending")) {
      return;
    }

    pollRef.current = setInterval(async () => {
      try {
        const updated = await pollASR(asrTask.id);
        setAsrTask(updated);
        if (updated.status === "completed") {
          // Reload subtitles
          await loadSubtitles();
          if (pollRef.current) clearInterval(pollRef.current);
        } else if (updated.status === "failed") {
          if (pollRef.current) clearInterval(pollRef.current);
        }
      } catch (e) {
        console.error("ASR poll error:", e);
      }
    }, 3000);

    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [asrTask?.id, asrTask?.status, videoId]);

  // Auto-scroll to active subtitle
  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [currentTime]);

  const handleSubmitASR = async () => {
    // Pre-check: ASR must be available
    if (asrMode === "disabled") {
      alert("ASR 功能已禁用，请在「设置 → 语音识别」中启用");
      return;
    }
    if (asrMode === "local" && serviceStatus?.status !== "running") {
      alert("本地 ASR 服务未运行，请先在「设置 → 语音识别」中启动服务");
      return;
    }

    setLoading(true);
    try {
      // Check health first
      const health = await checkASRHealth();
      if (health.status !== "ready") {
        alert("ASR 引擎未就绪，请检查「设置 → 语音识别」中的服务状态");
        return;
      }

      const taskId = await submitASR(videoId, undefined, true);
      setAsrTask({
        id: taskId,
        video_id: videoId,
        status: "processing",
        progress: 0,
        error_message: null,
        retry_count: 0,
        segment_count: null,
        created_at: new Date().toISOString(),
        completed_at: null,
      });
    } catch (e) {
      alert("ASR 提交失败: " + String(e));
    } finally {
      setLoading(false);
    }
  };

  // Convert absolute time to file-relative seconds
  const toRelativeSecs = (absoluteMs: number): number => {
    return (absoluteMs - baseTimeMs) / 1000;
  };

  // Find currently active segment
  const currentAbsoluteMs = baseTimeMs + currentTime * 1000;
  const activeIndex = segments.findIndex(
    (s) => s.start_ms <= currentAbsoluteMs && s.end_ms > currentAbsoluteMs
  );

  // Filter by search
  const displaySegments = searchQuery
    ? segments.filter((s) =>
        s.text.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : segments;

  return (
    <div className="rounded-lg border p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">字幕</h3>
        <div className="flex items-center gap-2">
          {segments.length > 0 && (
            <>
              <span className="text-xs text-muted-foreground">
                {segments.length} 条
              </span>
              <select
                className="h-6 rounded border text-xs px-1 bg-background"
                defaultValue=""
                onChange={async (e) => {
                  const fmt = e.target.value;
                  if (!fmt) return;
                  e.target.value = "";
                  try {
                    let content: string;
                    let ext: string;
                    if (fmt === "srt") {
                      content = await exportSubtitlesSrt(videoId);
                      ext = "srt";
                    } else if (fmt === "ass") {
                      content = await exportSubtitlesAss(videoId);
                      ext = "ass";
                    } else {
                      content = await exportSubtitlesVtt(videoId);
                      ext = "vtt";
                    }
                    // Download via blob
                    const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement("a");
                    a.href = url;
                    a.download = `subtitles.${ext}`;
                    a.click();
                    URL.revokeObjectURL(url);
                  } catch (err) {
                    alert("导出失败: " + String(err));
                  }
                }}
              >
                <option value="">导出...</option>
                <option value="srt">SRT</option>
                <option value="ass">ASS</option>
                <option value="vtt">VTT</option>
              </select>
              <Button
                size="sm"
                variant="ghost"
                className="h-6 px-1 text-xs"
                onClick={async () => {
                  if (!(await ask("重新识别将覆盖当前字幕，确定继续？", { title: "重新 ASR 识别", kind: "warning" }))) return;
                  await handleSubmitASR();
                }}
                disabled={!asrAvailable || loading || (asrTask?.status === "processing" || asrTask?.status === "pending")}
                title={!asrAvailable ? "ASR 服务未就绪" : "重新识别（将覆盖当前字幕）"}
              >
                重新生成
              </Button>
            </>
          )}
        </div>
      </div>

      {/* ASR progress or submit button */}
      {asrTask &&
      (asrTask.status === "processing" || asrTask.status === "pending") ? (
        <div className="space-y-1">
          <div className="flex justify-between text-xs text-muted-foreground">
            <span>ASR 识别中...</span>
            <span>{Math.round(asrTask.progress * 100)}%</span>
          </div>
          <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-primary transition-all duration-300"
              style={{ width: `${asrTask.progress * 100}%` }}
            />
          </div>
        </div>
      ) : asrTask?.status === "failed" ? (
        <div className="space-y-1">
          <div className="text-xs text-red-500">
            ASR 失败: {asrTask.error_message}
          </div>
          <Button size="sm" variant="outline" className="text-xs" onClick={handleSubmitASR}
            disabled={!asrAvailable}>
            重试
          </Button>
        </div>
      ) : segments.length === 0 ? (
        <div className="space-y-1.5">
          <Button
            size="sm"
            variant="outline"
            className="w-full text-xs"
            onClick={handleSubmitASR}
            disabled={!asrAvailable || loading}
          >
            {loading ? "提交中..." : "开始 ASR 语音识别"}
          </Button>
          {asrMode === "local" && !asrAvailable && (
            <p className="text-[11px] text-muted-foreground text-center">
              本地 ASR 服务未运行，请在「设置 → 语音识别」中启动
            </p>
          )}
          {asrMode === "disabled" && (
            <p className="text-[11px] text-muted-foreground text-center">
              ASR 功能已禁用
            </p>
          )}
        </div>
      ) : null}

      {/* Search */}
      {segments.length > 0 && (
        <Input
          placeholder="搜索字幕..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="text-xs h-7"
        />
      )}

      {/* Subtitle list */}
      {displaySegments.length > 0 && (
        <div className="max-h-[400px] overflow-y-auto space-y-0.5">
          {displaySegments.map((seg) => {
            const isActive =
              !searchQuery && segments.indexOf(seg) === activeIndex;
            const startSecs = toRelativeSecs(seg.start_ms);
            const endSecs = toRelativeSecs(seg.end_ms);

            return (
              <div
                key={seg.id}
                ref={isActive ? activeRef : undefined}
                className={`flex gap-2 px-2 py-1 rounded text-xs cursor-pointer hover:bg-accent/30 transition-colors ${
                  isActive ? "bg-accent/50" : ""
                }`}
                onClick={() => onSeek?.(startSecs)}
              >
                <span className="text-muted-foreground shrink-0 w-20">
                  {formatTime(startSecs)}
                </span>
                <span className="flex-1 break-all">{seg.text}</span>
                {/* Clip position helpers */}
                {(onSetClipStart || onSetClipEnd) && (
                  <div className="flex gap-0.5 shrink-0 opacity-0 hover:opacity-100">
                    {onSetClipStart && (
                      <button
                        className="text-muted-foreground hover:text-primary"
                        onClick={(e) => {
                          e.stopPropagation();
                          onSetClipStart(startSecs);
                        }}
                        title="设为起点"
                      >
                        [
                      </button>
                    )}
                    {onSetClipEnd && (
                      <button
                        className="text-muted-foreground hover:text-primary"
                        onClick={(e) => {
                          e.stopPropagation();
                          onSetClipEnd(endSecs);
                        }}
                        title="设为终点"
                      >
                        ]
                      </button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
