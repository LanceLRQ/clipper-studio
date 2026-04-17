import { useEffect, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Mic } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { SubtitleSegment, ASRServiceStatusInfo } from "@/services/asr";
import {
  listSubtitles,
  checkASRHealth,
  getASRServiceStatus,
  exportSubtitlesSrt,
  exportSubtitlesAss,
  exportSubtitlesVtt,
} from "@/services/asr";
import { getSettings } from "@/services/settings";
import { useASRQueueStore, useASRTaskForVideo } from "@/stores/asr-queue";

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
  const s = Math.floor(secs % 60);
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

/** Human-readable status label */
function statusLabel(status: string): string {
  switch (status) {
    case "queued": return "排队中...";
    case "converting": return "音频转换中...";
    case "submitting": return "提交识别任务...";
    case "processing": return "ASR 识别中...";
    default: return status;
  }
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
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const scrollParentRef = useRef<HTMLDivElement>(null);

  // ASR service availability
  const [asrMode, setAsrMode] = useState<string>("local");
  const [serviceStatus, setServiceStatus] = useState<ASRServiceStatusInfo | null>(null);
  const [remoteHealthy, setRemoteHealthy] = useState<boolean | null>(null);

  // ASR queue store
  const queueTask = useASRTaskForVideo(videoId);
  const submitTask = useASRQueueStore((s) => s.submitTask);
  const cancelTask = useASRQueueStore((s) => s.cancelTask);

  // Whether ASR is available for use
  const asrAvailable =
    asrMode === "disabled" ? false
    : asrMode === "remote" ? remoteHealthy === true
    : serviceStatus?.status === "running";

  // Determine if there is an active task from the store
  const isTaskActive = queueTask != null &&
    ["queued", "converting", "submitting", "processing"].includes(queueTask.status);

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

  // Periodically check remote ASR health when in remote mode
  useEffect(() => {
    if (asrMode !== "remote") {
      setRemoteHealthy(null);
      return;
    }
    let cancelled = false;
    const check = () => {
      checkASRHealth()
        .then((h) => { if (!cancelled) setRemoteHealthy(h.status === "ready"); })
        .catch(() => { if (!cancelled) setRemoteHealthy(false); });
    };
    check();
    const interval = setInterval(check, 30000);
    return () => { cancelled = true; clearInterval(interval); };
  }, [asrMode]);

  // Listen for real-time service status changes
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: UnlistenFn | undefined;
    listen<ASRServiceStatusInfo>("asr-service-status", (event) => {
      setServiceStatus(event.payload);
    }).then((fn) => { if (cancelled) { fn(); } else { unlistenFn = fn; } });
    return () => { cancelled = true; unlistenFn?.(); };
  }, []);

  // Load subtitles on mount
  useEffect(() => {
    loadSubtitles().catch(console.error);
  }, [videoId]);

  // When task completes via store, reload subtitles
  useEffect(() => {
    if (queueTask?.status === "completed") {
      loadSubtitles().catch(console.error);
    }
  }, [queueTask?.status, videoId]);

  // Auto-scroll handled by virtualizer (see below, gated on activeIndex change)

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

      await submitTask(videoId);
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

  // Virtualized row rendering (P5-PERF-09): avoid mounting thousands of DOM nodes
  const rowVirtualizer = useVirtualizer({
    count: displaySegments.length,
    getScrollElement: () => scrollParentRef.current,
    estimateSize: () => 52,
    overscan: 8,
  });

  // Auto-scroll to active subtitle: delegate to virtualizer to keep only visible
  // rows mounted. Only triggers when activeIndex or search toggle changes, not
  // on every currentTime tick.
  useEffect(() => {
    if (searchQuery) return;
    if (activeIndex < 0) return;
    rowVirtualizer.scrollToIndex(activeIndex, { align: "auto", behavior: "smooth" });
  }, [activeIndex, searchQuery, rowVirtualizer]);

  return (
    <div className="rounded-lg border p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">字幕</h3>
        <div className="flex items-center gap-2">
          {segments.length === 0 && !isTaskActive && queueTask?.status !== "failed" && (
            <Button
              size="sm"
              className="h-7 px-2.5 gap-1.5"
              onClick={handleSubmitASR}
              disabled={!asrAvailable || loading}
              title={
                asrMode === "disabled" ? "ASR 功能已禁用"
                  : asrMode === "remote" && remoteHealthy !== true
                    ? (remoteHealthy === false ? "远程 ASR 服务无法连接" : "远程 ASR 服务检测中...")
                  : !asrAvailable ? "ASR 服务未就绪"
                  : "开始 ASR 语音识别"
              }
            >
              <Mic className="h-3.5 w-3.5" />
              <span className="text-xs">{loading ? "提交中..." : "开始识别"}</span>
            </Button>
          )}
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
                disabled={!asrAvailable || loading || isTaskActive}
                title={!asrAvailable ? "ASR 服务未就绪" : "重新识别（将覆盖当前字幕）"}
              >
                重新生成
              </Button>
            </>
          )}
        </div>
      </div>

      {/* ASR progress (from global store) or submit button */}
      {isTaskActive ? (
        <div className="space-y-1">
          <div className="flex justify-between items-center text-xs text-muted-foreground">
            <span>{statusLabel(queueTask!.status)}</span>
            <div className="flex items-center gap-1.5">
              {queueTask!.status === "processing" && (
                <span>{Math.round(queueTask!.progress * 100)}%</span>
              )}
              <button
                className="px-1 py-0.5 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors text-xs leading-none"
                onClick={() => cancelTask(queueTask!.task_id)}
                title="取消任务"
              >
                ✕
              </button>
            </div>
          </div>
          {queueTask!.status === "processing" && (
            <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-primary transition-all duration-300"
                style={{ width: `${queueTask!.progress * 100}%` }}
              />
            </div>
          )}
          {(queueTask!.status === "converting" || queueTask!.status === "submitting") && (
            <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
              <div className="h-full bg-primary/60 animate-pulse rounded-full" style={{ width: "100%" }} />
            </div>
          )}
          {queueTask!.status === "queued" && (
            <div className="w-full h-2 bg-muted rounded-full overflow-hidden">
              <div className="h-full bg-muted-foreground/30 rounded-full" style={{ width: "100%" }} />
            </div>
          )}
        </div>
      ) : queueTask?.status === "failed" ? (
        <div className="space-y-1">
          <div className="text-xs text-red-500">
            ASR 失败: {queueTask.error_message}
          </div>
          <Button size="sm" variant="outline" className="text-xs" onClick={handleSubmitASR}
            disabled={!asrAvailable}>
            重试
          </Button>
        </div>
      ) : segments.length === 0 ? (
        <div className="py-3 text-center space-y-1">
          <p className="text-xs text-muted-foreground">暂无字幕</p>
          {asrMode === "local" && !asrAvailable && (
            <p className="text-[11px] text-muted-foreground">
              本地 ASR 服务未运行，请在「设置 → 语音识别」中启动
            </p>
          )}
          {asrMode === "remote" && remoteHealthy === false && (
            <p className="text-[11px] text-red-500">
              远程 ASR 服务无法连接，请检查「设置 → 语音识别」
            </p>
          )}
          {asrMode === "disabled" && (
            <p className="text-[11px] text-muted-foreground">
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

      {/* Subtitle list (virtualized) */}
      {displaySegments.length > 0 && (
        <div
          ref={scrollParentRef}
          className="max-h-[400px] overflow-y-auto pr-1"
        >
          <div
            style={{
              height: `${rowVirtualizer.getTotalSize()}px`,
              width: "100%",
              position: "relative",
            }}
          >
            {rowVirtualizer.getVirtualItems().map((virtualRow) => {
              const seg = displaySegments[virtualRow.index];
              if (!seg) return null;
              const isActive =
                !searchQuery && segments.indexOf(seg) === activeIndex;
              const startSecs = toRelativeSecs(seg.start_ms);
              const endSecs = toRelativeSecs(seg.end_ms);

              return (
                <div
                  key={seg.id}
                  data-index={virtualRow.index}
                  ref={rowVirtualizer.measureElement}
                  className={`group px-2.5 py-1.5 rounded cursor-pointer hover:bg-accent/30 transition-colors ${
                    isActive ? "bg-accent/50" : ""
                  }`}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                  onClick={() => onSeek?.(startSecs)}
                >
                  {/* Row 1: timestamp + clip buttons */}
                  <div className="flex items-center text-[11px] text-muted-foreground">
                    <span>{formatTime(startSecs)} — {formatTime(endSecs)}</span>
                    {(onSetClipStart || onSetClipEnd) && (
                      <div className="flex gap-0.5 ml-auto opacity-0 group-hover:opacity-100 transition-opacity">
                        {onSetClipStart && (
                          <button
                            className="px-1 py-0.5 rounded hover:bg-accent text-muted-foreground hover:text-primary transition-colors font-bold text-xs leading-none"
                            onClick={(e) => { e.stopPropagation(); onSetClipStart(startSecs); }}
                            title="设为切片起点"
                          >
                            [
                          </button>
                        )}
                        {onSetClipEnd && (
                          <button
                            className="px-1 py-0.5 rounded hover:bg-accent text-muted-foreground hover:text-primary transition-colors font-bold text-xs leading-none"
                            onClick={(e) => { e.stopPropagation(); onSetClipEnd(endSecs); }}
                            title="设为切片终点"
                          >
                            ]
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                  {/* Row 2: text */}
                  <div className="text-xs break-all mt-0.5">{seg.text}</div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
