import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { VideoPlayer } from "@/components/video-player/player";
import { HeatmapBar } from "@/components/video-player/heatmap-bar";
import { ClipTimeline } from "@/components/video-player/clip-timeline";
import { ClipActions } from "@/components/video-player/clip-actions";
import { SubtitlePanel } from "@/components/video-player/subtitle-panel";
import type { VideoInfo } from "@/types/video";
import type { ClipRegion, ClipOptions } from "@/types/multi-clip";
import type { EncodingPreset } from "@/types/clip";
import { CLIP_COLORS } from "@/lib/clip-colors";
import {
  getVideo,
  getEnvelope,
  extractEnvelope,
  checkVideoIntegrity,
  remuxVideo,
} from "@/services/video";
import { listPresets } from "@/services/clip";

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
  const navigate = useNavigate();
  const [video, setVideo] = useState<VideoInfo | null>(null);
  const [error, setError] = useState("");
  const [currentTime, setCurrentTime] = useState(0);
  const [envelope, setEnvelope] = useState<number[] | null>(null);
  const [envelopeLoading, setEnvelopeLoading] = useState(false);
  const [integrityIssues, setIntegrityIssues] = useState<string[] | null>(null);
  const [remuxing, setRemuxing] = useState(false);
  const [presets, setPresets] = useState<EncodingPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<number | null>(null);

  // Multi-clip state
  const [clips, setClips] = useState<ClipRegion[]>([]);
  const [selectedClipId, setSelectedClipId] = useState<string | null>(null);
  const [clipOptions, setClipOptions] = useState<Record<string, ClipOptions>>({});
  const [editingClipId, setEditingClipId] = useState<string | null>(null);

  const selectedClip = useMemo(
    () => clips.find((c) => c.id === selectedClipId) ?? null,
    [clips, selectedClipId]
  );

  // Track playback time
  useEffect(() => {
    const interval = setInterval(() => {
      const videoEl = document.querySelector("video");
      if (videoEl) setCurrentTime(videoEl.currentTime);
    }, 250);
    return () => clearInterval(interval);
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
  }, [videoId]);

  const handleSeek = (timeSecs: number) => {
    const videoEl = document.querySelector("video");
    if (videoEl) videoEl.currentTime = timeSecs;
  };

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
    if (selectedClipId) {
      // Update existing clip's start
      setClips((prev) =>
        prev.map((c) =>
          c.id === selectedClipId ? { ...c, start: Math.round(timeSecs) } : c
        )
      );
    } else {
      // Create new clip
      const newClip: ClipRegion = {
        id: `clip-${Date.now()}`,
        start: Math.round(timeSecs),
        end: 0,
        color: CLIP_COLORS[clips.length % CLIP_COLORS.length],
        name: `片段${clips.length + 1}`,
      };
      setClips((prev) => [...prev, newClip]);
      setSelectedClipId(newClip.id);
    }
  };

  const handleSetClipEnd = (timeSecs: number) => {
    if (selectedClipId) {
      setClips((prev) =>
        prev.map((c) =>
          c.id === selectedClipId ? { ...c, end: Math.round(timeSecs) } : c
        )
      );
    } else {
      const newClip: ClipRegion = {
        id: `clip-${Date.now()}`,
        start: 0,
        end: Math.round(timeSecs),
        color: CLIP_COLORS[clips.length % CLIP_COLORS.length],
        name: `片段${clips.length + 1}`,
      };
      setClips((prev) => [...prev, newClip]);
      setSelectedClipId(newClip.id);
    }
  };

  if (error) {
    return (
      <div className="space-y-4">
        <Button
          variant="ghost"
          onClick={() => navigate({ to: "/dashboard/videos" })}
        >
          ← 返回列表
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
    <div className="flex flex-col h-[calc(100vh-7rem)]">
      {/* Header */}
      <div className="flex items-center gap-2 shrink-0 pb-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => navigate({ to: "/dashboard/videos" })}
        >
          ← 返回
        </Button>
        <h2 className="text-lg font-semibold truncate">{video.file_name}</h2>
      </div>

      {/* Main layout: Left (scrollable) + Right (full height tabs) */}
      <div className="flex-1 flex gap-4 min-h-0">
        {/* Left column: Monitor (top, flex) + Timeline (bottom, fixed) — fills remaining space */}
        <div className="flex-1 flex flex-col min-h-0 min-w-0">
          {/* Monitor area: black bg, video centered, fills remaining space */}
          <div className="flex-1 min-h-0 bg-black rounded-lg flex items-center justify-center overflow-hidden">
            <div className="w-full max-h-full">
              <VideoPlayer src={video.file_path} title={video.file_name} />
            </div>
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
            />
          </div>
        </div>

        {/* Right column: fixed width Tabs */}
        <div className="w-[320px] shrink-0 flex flex-col min-h-0">
          <Tabs defaultValue={0} className="flex-1 flex flex-col min-h-0">
            <TabsList className="w-full shrink-0">
              <TabsTrigger value={0}>切片</TabsTrigger>
              <TabsTrigger value={1}>字幕</TabsTrigger>
              <TabsTrigger value={2}>信息</TabsTrigger>
            </TabsList>

            {/* Tab: Clips — full list + fixed bottom button */}
            <TabsContent value={0} className="flex-1 flex flex-col min-h-0 pt-2">
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

              {/* Fixed bottom: create task button */}
              {validClips.length > 0 && (
                <div className="shrink-0 pt-2 border-t mt-2">
                  <ClipActions
                    videoId={video.id}
                    clips={clips}
                    presetId={selectedPresetId}
                    clipOptions={clipOptions}
                  />
                </div>
              )}
            </TabsContent>

            {/* Tab: Subtitles */}
            <TabsContent value={1} className="flex-1 overflow-y-auto min-h-0 pt-2">
              <SubtitlePanel
                videoId={video.id}
                currentTime={currentTime}
                baseTimeMs={0}
                onSeek={handleSeek}
                onSetClipStart={handleSetClipStart}
                onSetClipEnd={handleSetClipEnd}
              />
            </TabsContent>

            {/* Tab: Video Info */}
            <TabsContent value={2} className="flex-1 overflow-y-auto min-h-0 pt-2 space-y-3">
              <div className="rounded-lg border p-4 space-y-3">
                <h3 className="font-medium">视频信息</h3>
                <div className="space-y-2 text-sm">
                  <InfoRow label="文件名" value={video.file_name} />
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
                    disabled={remuxing}
                  >
                    {remuxing ? "修复中..." : "转封装修复 (→ MP4)"}
                  </Button>
                </div>
              )}
            </TabsContent>
          </Tabs>
        </div>
      </div>
    </div>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right">{value}</span>
    </div>
  );
}

export const Route = createFileRoute("/dashboard/videos/$videoId")({
  component: VideoDetailPage,
});
