import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState } from "react";
import { revealFile } from "@/services/system";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { VideoPlayer } from "@/components/video-player/player";
import { HeatmapBar } from "@/components/video-player/heatmap-bar";
import { ClipTimeline } from "@/components/video-player/clip-timeline";
import { ClipActions } from "@/components/video-player/clip-actions";
import { SubtitlePanel } from "@/components/video-player/subtitle-panel";
import { DanmakuLayer } from "@/components/video-player/danmaku-layer";
import type { VideoInfo } from "@/types/video";
import type { ClipRegion, ClipOptions, BurnAvailability } from "@/types/multi-clip";
import type { EncodingPreset } from "@/types/clip";
import { CLIP_COLORS } from "@/lib/clip-colors";
import {
  getVideo,
  getEnvelope,
  extractEnvelope,
  checkVideoIntegrity,
  remuxVideo,
} from "@/services/video";
import { listPresets, checkVideoBurnAvailability } from "@/services/clip";
import { loadDanmaku, type DanmakuItem } from "@/services/danmaku";
import { transcodeVideo } from "@/services/media";
import type { TagInfo } from "@/types/tag";
import { getVideoTags, setVideoTags } from "@/services/tag";
import { TagBadge } from "@/components/tag/tag-badge";
import { TagSelector } from "@/components/tag/tag-selector";
import { useWorkspaceStore } from "@/stores/workspace";
import { getAppInfo } from "@/services/workspace";

function formatDuration(ms: number | null): string {
  if (!ms) return "--:--";
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0)
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

function VideoDetailPage() {
  const { videoId } = Route.useParams();
  const { t: seekToTime } = Route.useSearch();
  const navigate = useNavigate();
  const wsPathAccessible = useWorkspaceStore((s) => s.pathAccessible);
  const [video, setVideo] = useState<VideoInfo | null>(null);
  const [error, setError] = useState("");
  const [currentTime, setCurrentTime] = useState(0);
  const [envelope, setEnvelope] = useState<number[] | null>(null);
  const [envelopeLoading, setEnvelopeLoading] = useState(false);
  const [integrityIssues, setIntegrityIssues] = useState<string[] | null>(null);
  const [remuxing, setRemuxing] = useState(false);
  const [presets, setPresets] = useState<EncodingPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<number | null>(null);
  const [danmakuItems, setDanmakuItems] = useState<DanmakuItem[]>([]);
  const [danmakuEnabled, setDanmakuEnabled] = useState(true);
  const [danmakuContainer, setDanmakuContainer] = useState<HTMLDivElement | null>(null);
  const [mediaEl, setMediaEl] = useState<HTMLVideoElement | null>(null);

  // Callback ref for video element from VideoPlayer
  const handleVideoRef = useCallback((el: HTMLVideoElement | null) => {
    console.log("[VideoDetail] handleVideoRef:", !!el);
    setMediaEl(el);
  }, []);

  // Multi-clip state
  const [clips, setClips] = useState<ClipRegion[]>([]);
  const [selectedClipId, setSelectedClipId] = useState<string | null>(null);
  const [clipOptions, setClipOptions] = useState<Record<string, ClipOptions>>({});
  const [burnAvailability, setBurnAvailability] = useState<BurnAvailability | undefined>(undefined);
  const [editingClipId, setEditingClipId] = useState<string | null>(null);

  // Tags state
  const [videoTags, setVideoTagsState] = useState<TagInfo[]>([]);

  // FFprobe availability
  const [ffprobeAvailable, setFfprobeAvailable] = useState(true);

  // Right panel tab (0=info, 1=clips, 2=subtitles)
  const [activeTab, setActiveTab] = useState(0);

  // Resizable right panel
  const [rightPanelWidth, setRightPanelWidth] = useState(320);
  const containerRef = useRef<HTMLDivElement>(null);
  const isDragging = useRef(false);

  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isDragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    // rAF 节流：避免每次 mousemove 都触发 getBoundingClientRect + setState
    // 导致 reflow，合并到每帧一次
    let rafId: number | null = null;
    let pendingClientX = 0;

    const flush = () => {
      rafId = null;
      if (!isDragging.current || !containerRef.current) return;
      const containerRect = containerRef.current.getBoundingClientRect();
      const maxWidth = containerRect.width * 0.5;
      const minWidth = 260;
      const newWidth = containerRect.right - pendingClientX;
      setRightPanelWidth(Math.max(minWidth, Math.min(maxWidth, newWidth)));
    };

    const onMouseMove = (ev: MouseEvent) => {
      if (!isDragging.current) return;
      pendingClientX = ev.clientX;
      if (rafId === null) {
        rafId = requestAnimationFrame(flush);
      }
    };

    const onMouseUp = () => {
      isDragging.current = false;
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, []);

  // Track playback time：改用原生 timeupdate 事件（浏览器按帧触发，
  // 比 250ms 轮询 DOM 更轻量，且避免在 video 元素未挂载时的无效查询）
  useEffect(() => {
    let videoEl: HTMLVideoElement | null = null;
    let detachFn: (() => void) | null = null;

    const onTimeUpdate = () => {
      if (videoEl) setCurrentTime(videoEl.currentTime);
    };

    // video 元素可能由 vidstack 延迟挂载，轮询查找后仅绑定一次事件
    const attachTimer = setInterval(() => {
      const el = document.querySelector("video");
      if (el && el !== videoEl) {
        videoEl = el as HTMLVideoElement;
        videoEl.addEventListener("timeupdate", onTimeUpdate);
        detachFn = () => videoEl?.removeEventListener("timeupdate", onTimeUpdate);
        clearInterval(attachTimer);
      }
    }, 250);

    return () => {
      clearInterval(attachTimer);
      detachFn?.();
    };
  }, []);

  // Load video data
  useEffect(() => {
    const id = parseInt(videoId, 10);
    if (isNaN(id)) {
      setError("无效的视频 ID");
      return;
    }
    getVideo(id)
      .then((v) => {
        setVideo(v);
        getEnvelope(v.id).then(setEnvelope).catch(console.error);
        // Try to load danmaku (silently ignore if no XML found)
        loadDanmaku(v.id)
          .then((items) => {
            console.log("[Danmaku] Loaded", items.length, "items", items.slice(0, 3));
            setDanmakuItems(items);
          })
          .catch((e) => {
            console.warn("[Danmaku] Load failed:", e);
          });
        const ext = v.file_name.split(".").pop()?.toLowerCase();
        if (ext === "flv" || ext === "ts") {
          checkVideoIntegrity(v.id)
            .then((r) => { if (!r.is_intact) setIntegrityIssues(r.issues); })
            .catch(console.error);
        }
      })
      .catch((e) => setError(String(e)));

    listPresets().then((p) => {
      setPresets(p);
      if (p.length > 0) setSelectedPresetId(p[0].id);
    });

    // Load tags
    getVideoTags(parseInt(videoId, 10))
      .then(setVideoTagsState)
      .catch(console.error);

    // Load burn availability
    checkVideoBurnAvailability(parseInt(videoId, 10))
      .then(setBurnAvailability)
      .catch(() => setBurnAvailability(undefined));

    // Check ffprobe availability
    getAppInfo()
      .then((info) => setFfprobeAvailable(info.ffprobe_available))
      .catch(console.error);
  }, [videoId]);

  const handleSeek = (timeSecs: number) => {
    const videoEl = document.querySelector("video");
    if (videoEl) videoEl.currentTime = timeSecs;
  };

  // Seek to timestamp from search params (e.g. from global search)
  useEffect(() => {
    if (seekToTime == null || !mediaEl) return;

    const doSeek = () => {
      mediaEl.currentTime = seekToTime;
      // Clear the search param after seeking
      navigate({
        to: ".",
        search: {},
        replace: true,
      });
    };

    if (mediaEl.readyState >= 1) {
      doSeek();
    } else {
      const onLoaded = () => doSeek();
      mediaEl.addEventListener("loadedmetadata", onLoaded, { once: true });
      return () => mediaEl.removeEventListener("loadedmetadata", onLoaded);
    }
  }, [seekToTime, mediaEl, navigate]);

  const handleExtractEnvelope = async () => {
    if (!video) return;
    setEnvelopeLoading(true);
    try {
      const data = await extractEnvelope(video.id);
      setEnvelope(data);
    } catch (e) {
      alert("音量提取失败: " + String(e));
    } finally {
      setEnvelopeLoading(false);
    }
  };

  const handleRemux = async () => {
    if (!video) return;
    setRemuxing(true);
    try {
      const outputPath = await remuxVideo(video.id);
      alert(`修复完成！\n输出文件: ${outputPath}`);
      setIntegrityIssues(null);
    } catch (e) {
      alert("修复失败: " + String(e));
    } finally {
      setRemuxing(false);
    }
  };

  // Set clip start/end from subtitle panel or other sources
  const handleSetClipStart = (timeSecs: number) => {
    const rounded = Math.round(timeSecs);
    if (selectedClipId) {
      setClips((prev) =>
        prev.map((c) => {
          if (c.id !== selectedClipId) return c;
          // If end is already set, start must not exceed it
          if (c.end > 0 && rounded >= c.end) return c;
          return { ...c, start: rounded };
        })
      );
    } else {
      // Create new clip with default 10s duration
      const maxEnd = Math.round(durationSecs);
      const newClip: ClipRegion = {
        id: `clip-${Date.now()}`,
        start: rounded,
        end: Math.min(rounded + 10, maxEnd),
        color: CLIP_COLORS[clips.length % CLIP_COLORS.length],
        name: `片段${clips.length + 1}`,
      };
      setClips((prev) => [...prev, newClip]);
      setSelectedClipId(newClip.id);
    }
  };

  const handleSetClipEnd = (timeSecs: number) => {
    const rounded = Math.round(timeSecs);
    if (selectedClipId) {
      setClips((prev) =>
        prev.map((c) => {
          if (c.id !== selectedClipId) return c;
          // End must not be less than start
          if (c.start > 0 && rounded <= c.start) return c;
          return { ...c, end: rounded };
        })
      );
    } else {
      // Create new clip with default 10s duration
      const newClip: ClipRegion = {
        id: `clip-${Date.now()}`,
        start: Math.max(0, rounded - 10),
        end: rounded,
        color: CLIP_COLORS[clips.length % CLIP_COLORS.length],
        name: `片段${clips.length + 1}`,
      };
      setClips((prev) => [...prev, newClip]);
      setSelectedClipId(newClip.id);
    }
  };

  if (error) {
    return (
      <div className="space-y-4 p-6">
        <Button
          variant="ghost"
          onClick={() => window.history.back()}
        >
          <ArrowLeft className="mr-1 h-4 w-4" />
          返回列表
        </Button>
        <div className="text-red-500">{error}</div>
      </div>
    );
  }

  if (!video) {
    return <div className="text-muted-foreground">加载中...</div>;
  }

  const durationSecs = (video.duration_ms ?? 0) / 1000;

  const validClips = clips.filter((c) => c.end > c.start);

  return (
    <div className="flex flex-col h-full p-6">
      {/* Header */}
      <div className="flex items-center gap-2 shrink-0 pb-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => window.history.back()}
        >
          <ArrowLeft className="mr-1 h-4 w-4" />
          返回
        </Button>
        <h2 className="text-lg font-semibold truncate">{video.file_name}</h2>
      </div>

      {/* Path inaccessible warning */}
      {!wsPathAccessible && (
        <div className="shrink-0 rounded-lg border border-yellow-300 bg-yellow-50 dark:bg-yellow-950/20 p-3 mb-2 flex items-center gap-2">
          <AlertTriangle className="h-4 w-4 shrink-0 text-yellow-700 dark:text-yellow-300" />
          <span className="text-sm text-yellow-800 dark:text-yellow-200">
            工作区目录不可访问，视频播放和切片功能不可用。请检查网络存储连接。
          </span>
        </div>
      )}

      {/* FFprobe missing warning */}
      {!ffprobeAvailable && (
        <div className="shrink-0 rounded-lg border border-yellow-300 bg-yellow-50 dark:bg-yellow-950/20 p-3 mb-2 flex items-center gap-2">
          <AlertTriangle className="h-4 w-4 shrink-0 text-yellow-700 dark:text-yellow-300" />
          <span className="text-sm text-yellow-800 dark:text-yellow-200">
            FFprobe 未安装或不可用，切片、转码功能不可用。请在「设置 &gt; 依赖管理」中安装
            FFmpeg。
          </span>
        </div>
      )}

      {/* File missing warning */}
      {video.file_missing && (
        <div className="shrink-0 rounded-lg border border-destructive/40 bg-destructive/10 p-3 mb-2 flex items-center gap-2">
          <AlertTriangle className="h-4 w-4 shrink-0 text-destructive" />
          <span className="text-sm text-destructive">
            视频文件已从磁盘删除或移动，播放、切片、转码、ASR 等操作不可用。数据库记录（字幕、标签、历史切片）保留可查。
          </span>
        </div>
      )}

      {/* Main layout: Left (scrollable) + Right (full height tabs) */}
      <div ref={containerRef} className="flex-1 flex min-h-0">
        {/* Left column: Monitor (top, flex) + Timeline (bottom, fixed) — fills remaining space */}
        <div className="flex-1 flex flex-col min-h-0 min-w-0">
          {/* Monitor area: black bg, video centered, fills remaining space */}
          <div
            className="flex-1 min-h-0 bg-black rounded-lg overflow-hidden"
            style={{ position: "relative" }}
          >
            <div className="w-full h-full flex items-center justify-center">
              <div className="w-full max-h-full">
                <VideoPlayer src={video.file_path} title={video.file_name} onVideoRef={handleVideoRef} />
              </div>
            </div>
            {/* Danmaku overlay container — library appends its canvas here */}
            <div
              ref={setDanmakuContainer}
              style={{
                position: "absolute",
                inset: 0,
                zIndex: 2147483647,
                pointerEvents: "none",
              }}
            />
            {/* Danmaku toggle button */}
            {danmakuItems.length > 0 && (
              <button
                style={{ position: "absolute", top: 8, right: 8, zIndex: 2147483647 }}
                className="px-2 py-1 rounded text-xs bg-black/50 text-white hover:bg-black/70 transition-colors pointer-events-auto"
                onClick={() => setDanmakuEnabled((v) => !v)}
              >
                {danmakuEnabled ? "弹幕 ON" : "弹幕 OFF"}
              </button>
            )}
            {/* Danmaku logic (no visual output, manages library instance) */}
            <DanmakuLayer
              container={danmakuContainer}
              media={mediaEl}
              items={danmakuItems}
              enabled={danmakuEnabled && danmakuItems.length > 0}
            />
          </div>

          {/* Bottom fixed: Heatmap + Timeline */}
          <div className="shrink-0 space-y-2 pt-2">
            {/* Heatmap bar */}
            {envelope ? (
              <HeatmapBar
                envelope={envelope}
                duration={durationSecs}
                currentTime={currentTime}
                onSeek={handleSeek}
              />
            ) : (
              <div className="flex items-center justify-center h-8 bg-muted/30 rounded">
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-xs h-6"
                  onClick={handleExtractEnvelope}
                  disabled={envelopeLoading}
                >
                  {envelopeLoading ? "提取中..." : "生成音量热度条"}
                </Button>
              </div>
            )}

            {/* Multi-clip timeline */}
            <ClipTimeline
              duration={durationSecs}
              currentTime={currentTime}
              clips={clips}
              selectedClipId={selectedClipId}
              onCurrentTimeChange={handleSeek}
              onClipsChange={setClips}
              onClipSelect={setSelectedClipId}
              clipOptions={clipOptions}
              onClipOptionsChange={setClipOptions}
              burnAvailability={burnAvailability}
              videoId={video?.id}
              onClipCreated={() => setActiveTab(1)}
              disabled={!ffprobeAvailable || video.file_missing}
            />
          </div>
        </div>

        {/* Resize handle */}
        <div
          className="shrink-0 w-1 cursor-col-resize hover:bg-primary/30 active:bg-primary/50 transition-colors rounded-full mx-1"
          onMouseDown={handleResizeStart}
          title="拖动调整面板宽度"
        />

        {/* Right column: resizable Tabs */}
        <div className="shrink-0 flex flex-col min-h-0" style={{ width: rightPanelWidth }}>
          <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as number)} className="flex-1 flex flex-col min-h-0">
            <TabsList className="w-full shrink-0">
              <TabsTrigger value={0}>信息</TabsTrigger>
              <TabsTrigger value={1}>切片</TabsTrigger>
              <TabsTrigger value={2}>字幕</TabsTrigger>
            </TabsList>

            {/* Tab: Clips — full list + fixed bottom button */}
            <TabsContent value={1} className="flex-1 flex flex-col min-h-0 pt-2">
              {/* Preset selector */}
              <div className="shrink-0 mb-2">
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

              {/* Scrollable clip list */}
              <div className="flex-1 overflow-y-auto min-h-0">
                {clips.length === 0 ? (
                  <div className="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
                    在时间轴上双击创建选区，或点击"新增"按钮
                  </div>
                ) : (
                  <div className="space-y-1.5">
                    {clips.map((clip) => {
                      const isSelected = selectedClipId === clip.id;
                      const isValid = clip.end > clip.start;
                      return (
                        <div
                          key={clip.id}
                          className={`rounded p-2.5 text-xs cursor-pointer transition-colors ${
                            isSelected ? "bg-accent" : "hover:bg-accent/30"
                          }`}
                          onClick={() => setSelectedClipId(clip.id)}
                        >
                          <div className="flex items-center gap-2">
                            <span
                              className="inline-block w-2.5 h-2.5 rounded-sm shrink-0"
                              style={{ backgroundColor: clip.color }}
                            />
                            {editingClipId === clip.id ? (
                              <input
                                autoFocus
                                className="font-medium bg-transparent border-b border-current outline-none w-20"
                                style={{ color: clip.color }}
                                value={clip.name}
                                onClick={(e) => e.stopPropagation()}
                                onChange={(e) =>
                                  setClips((prev) =>
                                    prev.map((c) =>
                                      c.id === clip.id
                                        ? { ...c, name: e.target.value }
                                        : c
                                    )
                                  )
                                }
                                onBlur={() => setEditingClipId(null)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") setEditingClipId(null);
                                }}
                              />
                            ) : (
                              <span
                                className="font-medium cursor-text"
                                style={{ color: clip.color }}
                                onDoubleClick={(e) => {
                                  e.stopPropagation();
                                  setEditingClipId(clip.id);
                                }}
                              >
                                {clip.name}
                              </span>
                            )}
                            {isValid ? (
                              <span className="text-muted-foreground">
                                {formatTime(clip.start)} - {formatTime(clip.end)}
                                <span className="ml-1.5">
                                  ({formatTime(clip.end - clip.start)})
                                </span>
                              </span>
                            ) : (
                              <span className="text-yellow-600">未设置</span>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* Fixed bottom: clear + create task button */}
              {validClips.length > 0 && (
                <div className="shrink-0 pt-2 border-t mt-2 flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-red-500 hover:text-red-600 text-xs px-2"
                    onClick={async () => {
                      if (await ask(`确定清空全部 ${clips.length} 个选区？`, { title: "清空选区", kind: "warning" })) {
                        setClips([]);
                        setSelectedClipId(null);
                      }
                    }}
                  >
                    清空
                  </Button>
                  <div className="flex-1">
                    <ClipActions
                      videoId={video.id}
                      clips={clips}
                      presetId={selectedPresetId}
                      clipOptions={clipOptions}
                      disabled={!wsPathAccessible || !ffprobeAvailable || video.file_missing}
                    />
                  </div>
                </div>
              )}
            </TabsContent>

            {/* Tab: Subtitles */}
            <TabsContent value={2} className="flex-1 overflow-y-auto min-h-0 pt-2">
              <SubtitlePanel
                videoId={video.id}
                currentTime={currentTime}
                onSeek={handleSeek}
                onSetClipStart={handleSetClipStart}
                onSetClipEnd={handleSetClipEnd}
                disabled={video.file_missing}
              />
            </TabsContent>

            {/* Tab: Video Info */}
            <TabsContent value={0} className="flex-1 overflow-y-auto min-h-0 pt-2 space-y-3">
              <div className="rounded-lg border p-4 space-y-3">
                <h3 className="font-medium">视频信息</h3>
                <div className="space-y-2 text-sm">
                  <div className="flex justify-between gap-2">
                    <span className="text-muted-foreground shrink-0 whitespace-nowrap">文件名</span>
                    <span
                      className="text-right truncate cursor-pointer hover:text-primary hover:underline"
                      title={`在文件管理器中显示：${video.file_path}`}
                      onClick={() => revealFile(video.file_path).catch(console.error)}
                    >
                      {video.file_name}
                    </span>
                  </div>
                  <InfoRow label="时长" value={formatDuration(video.duration_ms)} />
                  <InfoRow
                    label="分辨率"
                    value={
                      video.width && video.height
                        ? `${video.width} x ${video.height}`
                        : "未知"
                    }
                  />
                  <InfoRow label="大小" value={formatFileSize(video.file_size)} />
                  {video.file_hash && (
                    <InfoRow
                      label="Blake3"
                      value={video.file_hash.slice(0, 16) + "..."}
                    />
                  )}
                </div>
              </div>

              {/* Tags */}
              <div className="rounded-lg border p-4 space-y-2">
                <div className="flex items-center justify-between">
                  <h3 className="font-medium text-sm">标签</h3>
                  {video && (
                    <TagSelector
                      selectedTags={videoTags}
                      onChange={async (tags) => {
                        setVideoTagsState(tags);
                        try {
                          await setVideoTags({
                            video_id: video.id,
                            tag_ids: tags.map((t) => t.id),
                          });
                        } catch (e) {
                          console.error("Failed to save tags:", e);
                        }
                      }}
                    />
                  )}
                </div>
                <div className="flex flex-wrap gap-1">
                  {videoTags.length === 0 ? (
                    <span className="text-xs text-muted-foreground">暂无标签</span>
                  ) : (
                    videoTags.map((tag) => (
                      <TagBadge
                        key={tag.id}
                        tag={tag}
                        removable
                        onRemove={async () => {
                          const newTags = videoTags.filter((t) => t.id !== tag.id);
                          setVideoTagsState(newTags);
                          if (video) {
                            try {
                              await setVideoTags({
                                video_id: video.id,
                                tag_ids: newTags.map((t) => t.id),
                              });
                            } catch (e) {
                              console.error("Failed to save tags:", e);
                            }
                          }
                        }}
                      />
                    ))
                  )}
                </div>
              </div>

              {/* Integrity warning */}
              {integrityIssues && integrityIssues.length > 0 && (
                <div className="rounded-lg border border-yellow-300 bg-yellow-50 p-4 space-y-2">
                  <h3 className="font-medium text-yellow-800 text-sm">
                    文件完整性问题
                  </h3>
                  <ul className="text-xs text-yellow-700 space-y-1">
                    {integrityIssues.map((issue, i) => (
                      <li key={i}>• {issue}</li>
                    ))}
                  </ul>
                  <Button
                    size="sm"
                    variant="outline"
                    className="text-xs"
                    onClick={handleRemux}
                    disabled={remuxing || video.file_missing}
                  >
                    {remuxing ? "修复中..." : "转封装修复为 MP4"}
                  </Button>
                </div>
              )}

              {/* Transcode */}
              <div className="mt-3 pt-3 border-t space-y-2">
                <h3 className="font-medium text-sm">转码</h3>
                <div className="flex items-center gap-2">
                  <select
                    className="flex-1 rounded-md border bg-background px-2 h-8 text-xs"
                    value={selectedPresetId ?? ""}
                    onChange={(e) => setSelectedPresetId(Number(e.target.value) || null)}
                  >
                    {presets
                      .filter((p) => p.name !== "极速（无重编码）")
                      .map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                  </select>
                  <Button
                    size="sm"
                    variant="outline"
                    className="text-xs"
                    onClick={async () => {
                      if (!video || !selectedPresetId) return;
                      if (!(await ask("开始转码？", { title: "转码确认" }))) return;
                      try {
                        await transcodeVideo({
                          video_id: video.id,
                          preset_id: selectedPresetId,
                        });
                        navigate({ to: "/dashboard/tasks" });
                      } catch (e) {
                        alert("转码失败: " + String(e));
                      }
                    }}
                    disabled={!selectedPresetId || !ffprobeAvailable || video.file_missing}
                  >
                    开始转码
                  </Button>
                </div>
              </div>
            </TabsContent>
          </Tabs>
        </div>
      </div>
    </div>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-2">
      <span className="text-muted-foreground shrink-0 whitespace-nowrap">{label}</span>
      <span className="text-right truncate">{value}</span>
    </div>
  );
}

type VideoDetailSearch = {
  t?: number;
};

export const Route = createFileRoute("/dashboard/videos/$videoId")({
  component: VideoDetailPage,
  validateSearch: (search: Record<string, unknown>): VideoDetailSearch => ({
    t: typeof search.t === "number" ? search.t : undefined,
  }),
});
