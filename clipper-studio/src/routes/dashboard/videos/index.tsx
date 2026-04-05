import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import type { VideoInfo, ListVideosResponse } from "@/types/video";
import { listVideos, importVideo, deleteVideo } from "@/services/video";
import { getActiveWorkspace } from "@/services/workspace";

function formatDuration(ms: number | null): string {
  if (!ms) return "--:--";
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0) return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function VideosPage() {
  const navigate = useNavigate();
  const [response, setResponse] = useState<ListVideosResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [importing, setImporting] = useState(false);

  const loadVideos = async () => {
    setLoading(true);
    try {
      const activeWs = await getActiveWorkspace();
      const res = await listVideos({
        workspace_id: activeWs,
        page: 1,
        page_size: 50,
      });
      setResponse(res);
    } catch (e) {
      console.error("Failed to load videos:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadVideos();
  }, []);

  const handleImport = async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "视频文件",
            extensions: ["mp4", "mkv", "flv", "ts", "avi", "mov", "wmv", "webm"],
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

      await loadVideos();
    } catch (e) {
      console.error("Import failed:", e);
    } finally {
      setImporting(false);
    }
  };

  const handleDelete = async (video: VideoInfo) => {
    if (!confirm(`确定要删除「${video.file_name}」吗？\n\n注意：仅删除记录，不会删除磁盘文件。`)) {
      return;
    }
    try {
      await deleteVideo(video.id);
      await loadVideos();
    } catch (e) {
      console.error("Delete failed:", e);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-semibold">视频列表</h2>
          {response && (
            <p className="text-sm text-muted-foreground">
              共 {response.total} 个视频
            </p>
          )}
        </div>
        <Button onClick={handleImport} disabled={importing}>
          {importing ? "导入中..." : "+ 导入视频"}
        </Button>
      </div>

      {loading ? (
        <div className="text-muted-foreground">加载中...</div>
      ) : !response || response.videos.length === 0 ? (
        <div className="rounded-lg border border-dashed p-12 text-center">
          <p className="text-muted-foreground mb-4">暂无视频</p>
          <Button onClick={handleImport}>导入视频文件</Button>
        </div>
      ) : (
        <div className="space-y-2">
          {response.videos.map((video) => (
            <div
              key={video.id}
              className="flex items-center justify-between rounded-lg border p-3 hover:bg-accent/50 cursor-pointer transition-colors"
              onClick={() =>
                navigate({
                  to: "/dashboard/videos/$videoId",
                  params: { videoId: String(video.id) },
                })
              }
            >
              <div className="flex items-center gap-4 min-w-0">
                {/* Thumbnail placeholder */}
                <div className="w-32 h-20 bg-muted rounded flex items-center justify-center text-muted-foreground text-xs shrink-0">
                  {video.width && video.height
                    ? `${video.width}x${video.height}`
                    : "视频"}
                </div>
                <div className="min-w-0 space-y-1">
                  <div className="font-medium truncate">{video.file_name}</div>
                  <div className="flex gap-3 text-xs text-muted-foreground">
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
              <Button
                variant="ghost"
                size="sm"
                className="text-red-500 hover:text-red-600 shrink-0"
                onClick={(e) => {
                  e.stopPropagation();
                  handleDelete(video);
                }}
              >
                删除
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/videos/")({
  component: VideosPage,
});
