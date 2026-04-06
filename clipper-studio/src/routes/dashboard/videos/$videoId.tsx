import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { VideoPlayer } from "@/components/video-player/player";
import { ClipPanel } from "@/components/video-player/clip-panel";
import { HeatmapBar } from "@/components/video-player/heatmap-bar";
import type { VideoInfo } from "@/types/video";
import {
  getVideo,
  getEnvelope,
  extractEnvelope,
  checkVideoIntegrity,
  remuxVideo,
} from "@/services/video";

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

  // Track playback time from the video element
  useEffect(() => {
    const interval = setInterval(() => {
      const videoEl = document.querySelector("video");
      if (videoEl) {
        setCurrentTime(videoEl.currentTime);
      }
    }, 250);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    const id = parseInt(videoId, 10);
    if (isNaN(id)) {
      setError("无效的视频 ID");
      return;
    }
    getVideo(id)
      .then((v) => {
        setVideo(v);
        // Load envelope if available
        getEnvelope(v.id).then(setEnvelope).catch(console.error);
        // Check integrity for FLV files
        const ext = v.file_name.split(".").pop()?.toLowerCase();
        if (ext === "flv" || ext === "ts") {
          checkVideoIntegrity(v.id)
            .then((r) => {
              if (!r.is_intact) setIntegrityIssues(r.issues);
            })
            .catch(console.error);
        }
      })
      .catch((e) => setError(String(e)));
  }, [videoId]);

  const handleExtractEnvelope = async () => {
    if (!video) return;
    setEnvelopeLoading(true);
    try {
      const data = await extractEnvelope(video.id);
      setEnvelope(data);
    } catch (e) {
      console.error("Envelope extraction failed:", e);
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

  const handleSeek = (timeSecs: number) => {
    const videoEl = document.querySelector("video");
    if (videoEl) {
      videoEl.currentTime = timeSecs;
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

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => navigate({ to: "/dashboard/videos" })}
        >
          ← 返回
        </Button>
        <h2 className="text-lg font-semibold truncate">{video.file_name}</h2>
      </div>

      {/* Player + Side panels */}
      <div className="grid grid-cols-1 xl:grid-cols-3 gap-4">
        {/* Player + Heatmap */}
        <div className="xl:col-span-2 space-y-2">
          <VideoPlayer src={video.file_path} title={video.file_name} />

          {/* Heatmap bar */}
          {envelope ? (
            <HeatmapBar
              envelope={envelope}
              duration={durationSecs}
              currentTime={currentTime}
              onSeek={handleSeek}
            />
          ) : (
            <div className="flex items-center justify-center h-10 bg-muted/30 rounded">
              <Button
                variant="ghost"
                size="sm"
                className="text-xs"
                onClick={handleExtractEnvelope}
                disabled={envelopeLoading}
              >
                {envelopeLoading ? "提取中..." : "生成音量热度条"}
              </Button>
            </div>
          )}
        </div>

        {/* Side panel: Info + Clip */}
        <div className="space-y-4">
          {/* Video Info */}
          <div className="rounded-lg border p-4 space-y-3">
            <h3 className="font-medium">视频信息</h3>
            <div className="space-y-2 text-sm">
              <InfoRow label="文件名" value={video.file_name} />
              <InfoRow
                label="时长"
                value={formatDuration(video.duration_ms)}
              />
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

          {/* Clip Panel */}
          <ClipPanel
            videoId={video.id}
            currentTime={currentTime}
            duration={durationSecs}
            onSeek={handleSeek}
          />
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
