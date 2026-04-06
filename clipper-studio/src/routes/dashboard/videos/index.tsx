import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import type {
  VideoInfo,
  SessionInfo,
  StreamerInfo,
  ListVideosResponse,
} from "@/types/video";
import {
  listVideos,
  listSessions,
  listStreamers,
  importVideo,
  deleteVideo,
} from "@/services/video";
import { getActiveWorkspace } from "@/services/workspace";

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

/** Compute end time from recorded_at + duration_ms */
function computeEndTime(recordedAt: string | null, durationMs: number | null): string | null {
  if (!recordedAt || !durationMs) return null;
  // recorded_at format: "yyyy-MM-dd HH:mm:ss"
  const match = recordedAt.match(/^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/);
  if (!match) return null;
  const date = new Date(
    parseInt(match[1]), parseInt(match[2]) - 1, parseInt(match[3]),
    parseInt(match[4]), parseInt(match[5]), parseInt(match[6])
  );
  date.setMilliseconds(date.getMilliseconds() + durationMs);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** Format time range: show date once if same day */
function formatTimeRange(start: string | null, end: string | null): string {
  if (!start) return "";
  const startDate = start.slice(0, 10);
  const startTime = start.slice(11);
  if (!end) return start;
  const endDate = end.slice(0, 10);
  const endTime = end.slice(11);
  if (startDate === endDate) {
    return `${startDate} ${startTime} ~ ${endTime}`;
  }
  return `${start} ~ ${end}`;
}

/** Build display title: 主播 - 标题 - 时间段 */
function buildVideoTitle(video: VideoInfo): string {
  const parts: string[] = [];
  if (video.streamer_name) parts.push(video.streamer_name);
  if (video.stream_title) parts.push(video.stream_title);
  const endTime = computeEndTime(video.recorded_at, video.duration_ms);
  const timeRange = formatTimeRange(video.recorded_at, endTime);
  if (timeRange) parts.push(timeRange);
  return parts.length > 0 ? parts.join(" - ") : video.file_name;
}

type ViewMode = "streamers" | "flat";

function VideosPage() {
  const navigate = useNavigate();
  const [viewMode, setViewMode] = useState<ViewMode>("streamers");
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [flatVideos, setFlatVideos] = useState<ListVideosResponse | null>(null);
  const [streamers, setStreamers] = useState<StreamerInfo[]>([]);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [importing, setImporting] = useState(false);

  const loadData = async () => {
    setLoading(true);
    try {
      const activeWs = await getActiveWorkspace();
      const [sess, flat, strs] = await Promise.all([
        listSessions(activeWs),
        listVideos({ workspace_id: activeWs, page: 1, page_size: 200 }),
        listStreamers(),
      ]);
      setSessions(sess);
      setFlatVideos(flat);
      setStreamers(strs);
    } catch (e) {
      console.error("Failed to load videos:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const toggleGroup = (key: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  // Group sessions by streamer for "streamers" view
  const sessionsByStreamer = useMemo(() => {
    const map = new Map<number | null, SessionInfo[]>();
    for (const s of sessions) {
      const key = s.streamer_id;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(s);
    }
    return map;
  }, [sessions]);

  const handleImport = async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "视频文件",
            extensions: [
              "mp4", "mkv", "flv", "ts", "avi", "mov", "wmv", "webm",
            ],
          },
        ],
      });
      if (!selected) return;

      setImporting(true);
      const paths = Array.isArray(selected) ? selected : [selected];
      const activeWs = await getActiveWorkspace();
      for (const filePath of paths) {
        try {
          await importVideo({ file_path: filePath, workspace_id: activeWs });
        } catch (e) {
          console.error(`Failed to import ${filePath}:`, e);
        }
      }
      await loadData();
    } catch (e) {
      console.error("Import failed:", e);
    } finally {
      setImporting(false);
    }
  };

  const handleDelete = async (video: VideoInfo, e: React.MouseEvent) => {
    e.stopPropagation();
    if (
      !confirm(
        `确定要删除「${video.file_name}」吗？\n\n注意：仅删除记录，不会删除磁盘文件。`
      )
    )
      return;
    try {
      await deleteVideo(video.id);
      await loadData();
    } catch (e) {
      console.error("Delete failed:", e);
    }
  };

  const navigateToVideo = (videoId: number) => {
    navigate({
      to: "/dashboard/videos/$videoId",
      params: { videoId: String(videoId) },
    });
  };

  const totalVideos = flatVideos?.total ?? 0;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-semibold">视频列表</h2>
          <p className="text-sm text-muted-foreground">
            共 {totalVideos} 个视频
            {sessions.length > 0 && `，${sessions.length} 个场次`}
            {streamers.length > 0 && `，${streamers.length} 个主播`}
          </p>
        </div>
        <div className="flex gap-2">
          {/* View mode toggle */}
          <div className="flex rounded-md border">
            {(["streamers", "flat"] as const).map((mode) => (
              <button
                key={mode}
                className={`px-3 py-1 text-sm ${viewMode === mode ? "bg-accent" : ""}`}
                onClick={() => setViewMode(mode)}
              >
                {{ streamers: "主播", flat: "列表" }[mode]}
              </button>
            ))}
          </div>
          <Button onClick={handleImport} disabled={importing}>
            {importing ? "导入中..." : "+ 导入视频"}
          </Button>
        </div>
      </div>

      {loading ? (
        <div className="text-muted-foreground">加载中...</div>
      ) : totalVideos === 0 ? (
        <div className="rounded-lg border border-dashed p-12 text-center">
          <p className="text-muted-foreground mb-4">暂无视频</p>
          <Button onClick={handleImport}>导入视频文件</Button>
        </div>
      ) : viewMode === "streamers" ? (
        /* ===== Streamer → Session → Video view ===== */
        <div className="space-y-4">
          {streamers.map((streamer) => {
            const streamerSessions = sessionsByStreamer.get(streamer.id) ?? [];
            if (streamerSessions.length === 0) return null;
            const streamerKey = `streamer-${streamer.id}`;
            const isStreamerExpanded = expandedGroups.has(streamerKey);
            const allVideos = streamerSessions.flatMap((s) => s.videos);
            const totalDuration = allVideos.reduce(
              (sum, v) => sum + (v.duration_ms ?? 0), 0
            );
            return (
              <div key={streamer.id} className="rounded-lg border">
                {/* Streamer header */}
                <div
                  className="flex items-center justify-between p-3 cursor-pointer hover:bg-accent/30 transition-colors"
                  onClick={() => toggleGroup(streamerKey)}
                >
                  <div className="flex items-center gap-3 min-w-0">
                    <span className="text-muted-foreground text-sm">
                      {isStreamerExpanded ? "▼" : "▶"}
                    </span>
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-primary">
                          {streamer.name}
                        </span>
                        {streamer.room_id && (
                          <span className="text-xs text-muted-foreground">
                            房间号 {streamer.room_id}
                          </span>
                        )}
                      </div>
                      <div className="flex gap-3 text-xs text-muted-foreground">
                        <span>{streamerSessions.length} 个场次</span>
                        <span>{allVideos.length} 个视频</span>
                        <span>总时长 {formatDuration(totalDuration)}</span>
                      </div>
                    </div>
                  </div>
                  <StatusTags videos={allVideos} />
                </div>

                {/* Expanded: show sessions under this streamer */}
                {isStreamerExpanded && (
                  <div className="border-t">
                    {streamerSessions.map((session) => {
                      const sessKey = `streamer-sess-${session.id}`;
                      const isSessExpanded = expandedGroups.has(sessKey);
                      const sessDuration = session.videos.reduce(
                        (sum, v) => sum + (v.duration_ms ?? 0), 0
                      );
                      return (
                        <div key={session.id}>
                          {/* Session header (indented) */}
                          <div
                            className="flex items-center justify-between px-4 py-2 pl-10 cursor-pointer hover:bg-accent/20 transition-colors border-b last:border-b-0"
                            onClick={() => toggleGroup(sessKey)}
                          >
                            <div className="flex items-center gap-3 min-w-0">
                              <span className="text-muted-foreground text-xs">
                                {isSessExpanded ? "▼" : "▶"}
                              </span>
                              <div className="min-w-0">
                                <div className="text-sm font-medium truncate">
                                  {session.title || "未命名场次"}
                                </div>
                                <div className="flex gap-3 text-xs text-muted-foreground">
                                  {session.started_at && (
                                    <span>
                                      {formatTimeRange(
                                        session.started_at,
                                        computeEndTime(session.started_at, sessDuration)
                                      )}
                                    </span>
                                  )}
                                  <span>{session.videos.length} 个分片</span>
                                  <span>{formatDuration(sessDuration)}</span>
                                </div>
                              </div>
                            </div>
                            <StatusTags videos={session.videos} />
                          </div>

                          {/* Expanded: show videos in session */}
                          {isSessExpanded && (
                            <div className="bg-muted/10">
                              {session.videos.map((video) => (
                                <VideoRow
                                  key={video.id}
                                  video={video}
                                  compact
                                  indent
                                  onNavigate={() => navigateToVideo(video.id)}
                                  onDelete={(e) => handleDelete(video, e)}
                                />
                              ))}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}

          {/* Sessions without streamer */}
          {(sessionsByStreamer.get(null)?.length ?? 0) > 0 && (
            <div className="rounded-lg border">
              <div className="p-3 text-sm text-muted-foreground font-medium">
                未关联主播的场次
              </div>
              <div className="border-t">
                {sessionsByStreamer.get(null)!.map((session) => {
                  const sessKey = `orphan-sess-${session.id}`;
                  const isSessExpanded = expandedGroups.has(sessKey);
                  const sessDuration = session.videos.reduce(
                    (sum, v) => sum + (v.duration_ms ?? 0), 0
                  );
                  return (
                    <div key={session.id}>
                      <div
                        className="flex items-center justify-between px-4 py-2 cursor-pointer hover:bg-accent/20 transition-colors border-b last:border-b-0"
                        onClick={() => toggleGroup(sessKey)}
                      >
                        <div className="flex items-center gap-3 min-w-0">
                          <span className="text-muted-foreground text-xs">
                            {isSessExpanded ? "▼" : "▶"}
                          </span>
                          <div className="min-w-0">
                            <div className="text-sm font-medium truncate">
                              {session.title || "未命名场次"}
                            </div>
                            <div className="flex gap-3 text-xs text-muted-foreground">
                              {session.started_at && (
                                <span>
                                  {formatTimeRange(
                                    session.started_at,
                                    computeEndTime(session.started_at, sessDuration)
                                  )}
                                </span>
                              )}
                              <span>{session.videos.length} 个分片</span>
                              <span>{formatDuration(sessDuration)}</span>
                            </div>
                          </div>
                        </div>
                        <StatusTags videos={session.videos} />
                      </div>
                      {isSessExpanded && (
                        <div className="bg-muted/10">
                          {session.videos.map((video) => (
                            <VideoRow
                              key={video.id}
                              video={video}
                              compact
                              onNavigate={() => navigateToVideo(video.id)}
                              onDelete={(e) => handleDelete(video, e)}
                            />
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Videos without any session */}
          {flatVideos &&
            flatVideos.videos.filter((v) => v.session_id === null).length > 0 && (
              <div className="rounded-lg border">
                <div className="p-3 text-sm text-muted-foreground font-medium">
                  未分组的视频
                </div>
                <div className="border-t">
                  {flatVideos.videos
                    .filter((v) => v.session_id === null)
                    .map((video) => (
                      <VideoRow
                        key={video.id}
                        video={video}
                        compact={false}
                        onNavigate={() => navigateToVideo(video.id)}
                        onDelete={(e) => handleDelete(video, e)}
                      />
                    ))}
                </div>
              </div>
            )}
        </div>
      ) : (
        /* ===== Flat list view ===== */
        <div className="space-y-2">
          {flatVideos?.videos.map((video) => (
            <VideoRow
              key={video.id}
              video={video}
              compact={false}
              onNavigate={() => navigateToVideo(video.id)}
              onDelete={(e) => handleDelete(video, e)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/** Status tag badges for a group of videos */
function StatusTags({ videos }: { videos: VideoInfo[] }) {
  return (
    <div className="flex gap-1.5 shrink-0">
      {videos.some((v) => v.has_danmaku) && (
        <span className="text-xs px-1.5 py-0.5 rounded bg-blue-100 text-blue-700">
          弹幕
        </span>
      )}
      {videos.some((v) => v.has_subtitle) && (
        <span className="text-xs px-1.5 py-0.5 rounded bg-green-100 text-green-700">
          字幕
        </span>
      )}
      {videos.some((v) => v.has_envelope) && (
        <span className="text-xs px-1.5 py-0.5 rounded bg-orange-100 text-orange-700">
          热度
        </span>
      )}
    </div>
  );
}

function VideoRow({
  video,
  compact,
  indent,
  onNavigate,
  onDelete,
}: {
  video: VideoInfo;
  compact: boolean;
  indent?: boolean;
  onNavigate: () => void;
  onDelete: (e: React.MouseEvent) => void;
}) {
  const title = buildVideoTitle(video);
  const showTitle = title !== video.file_name;

  const paddingClass = indent ? "px-4 py-2 pl-16" : compact ? "px-4 py-2 pl-10" : "rounded-lg border p-3";

  return (
    <div
      className={`flex items-center justify-between hover:bg-accent/30 cursor-pointer transition-colors ${paddingClass}`}
      onClick={onNavigate}
    >
      <div className="flex items-center gap-3 min-w-0">
        <div className="min-w-0 space-y-0.5">
          {/* Primary line: streamer - title - time range */}
          <div className="text-sm truncate font-medium">
            {showTitle ? title : video.file_name}
          </div>
          {/* Secondary line: file name (if title shown) + meta */}
          <div className="flex gap-3 text-xs text-muted-foreground">
            {showTitle && (
              <span className="truncate max-w-[300px]">{video.file_name}</span>
            )}
            <span>{formatDuration(video.duration_ms)}</span>
            <span>{formatFileSize(video.file_size)}</span>
            {video.width && video.height && (
              <span>
                {video.width}x{video.height}
              </span>
            )}
          </div>
        </div>
      </div>
      <div className="flex items-center gap-1.5 shrink-0">
        {video.has_danmaku && (
          <span className="text-xs px-1 py-0.5 rounded bg-blue-100 text-blue-700">
            弹幕
          </span>
        )}
        <Button
          variant="ghost"
          size="sm"
          className="text-red-500 hover:text-red-600 h-7 px-2"
          onClick={onDelete}
        >
          删除
        </Button>
      </div>
    </div>
  );
}

export const Route = createFileRoute("/dashboard/videos/")({
  component: VideosPage,
});
