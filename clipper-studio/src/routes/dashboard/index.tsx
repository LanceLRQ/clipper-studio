import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useWorkspaceStore } from "@/stores/workspace";
import {
  Video,
  Clock,
  HardDrive,
  Users,
  Clapperboard,
  CircleCheck,
  CircleX,
  Subtitles,
  MessageSquareText,
  Layers,
} from "lucide-react";

// ===== Types =====

interface DashboardStats {
  video_count: number;
  total_duration_ms: number;
  total_storage_bytes: number;
  streamer_count: number;
  session_count: number;
  subtitle_video_count: number;
  danmaku_video_count: number;
  clip_total: number;
  clip_completed: number;
  clip_failed: number;
  clip_output_bytes: number;
  recent_clips: RecentClipInfo[];
  top_streamers: TopStreamerInfo[];
}

interface RecentClipInfo {
  id: number;
  title: string;
  status: string;
  created_at: string;
}

interface TopStreamerInfo {
  name: string;
  video_count: number;
  total_duration_ms: number;
}

interface AppInfo {
  version: string;
  data_dir: string;
  ffmpeg_available: boolean;
  ffmpeg_version: string | null;
  ffprobe_available: boolean;
}

// ===== Helpers =====

function formatDuration(ms: number): string {
  if (!ms) return "0 分钟";
  const totalMin = Math.floor(ms / 60000);
  if (totalMin < 60) return `${totalMin} 分钟`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h < 24) return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`;
  const d = Math.floor(h / 24);
  const rh = h % 24;
  return rh > 0 ? `${d} 天 ${rh} 小时` : `${d} 天`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function formatShortDuration(ms: number): string {
  const h = Math.floor(ms / 3600000);
  const m = Math.floor((ms % 3600000) / 60000);
  if (h > 0) return `${h}h${m > 0 ? `${m}m` : ""}`;
  return `${m}m`;
}

const STATUS_LABELS: Record<string, { label: string; className: string }> = {
  completed: { label: "完成", className: "text-green-600 dark:text-green-400" },
  failed: { label: "失败", className: "text-red-500" },
  processing: { label: "进行中", className: "text-blue-500" },
  pending: { label: "等待中", className: "text-muted-foreground" },
  cancelled: { label: "已取消", className: "text-muted-foreground" },
};

// ===== Components =====

function StatCard({
  icon: Icon,
  label,
  value,
  sub,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  value: string;
  sub?: string;
}) {
  return (
    <div className="rounded-lg border p-4 space-y-1">
      <div className="flex items-center gap-2 text-muted-foreground">
        <Icon className="h-4 w-4" />
        <span className="text-xs">{label}</span>
      </div>
      <div className="text-2xl font-semibold">{value}</div>
      {sub && <div className="text-xs text-muted-foreground">{sub}</div>}
    </div>
  );
}

// ===== Page =====

function DashboardIndex() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const workspaceId = useWorkspaceStore((s) => s.activeId);
  const wsVersion = useWorkspaceStore((s) => s.version);

  useEffect(() => {
    setLoading(true);
    Promise.all([
      invoke<DashboardStats>("get_dashboard_stats", {
        workspaceId,
      }),
      invoke<AppInfo>("get_app_info"),
    ])
      .then(([s, a]) => {
        setStats(s);
        setAppInfo(a);
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [workspaceId, wsVersion]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground text-sm">
        加载中...
      </div>
    );
  }

  if (!stats) {
    return (
      <div className="text-muted-foreground text-sm">
        无法加载统计数据
      </div>
    );
  }

  const clipSuccessRate =
    stats.clip_total > 0
      ? Math.round((stats.clip_completed / stats.clip_total) * 100)
      : 0;

  return (
    <div className="space-y-6 max-w-5xl p-6">
      <h2 className="text-2xl font-semibold">仪表盘</h2>

      {/* Overview Cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <StatCard
          icon={Video}
          label="视频总数"
          value={String(stats.video_count)}
          sub={`${stats.session_count} 场录制`}
        />
        <StatCard
          icon={Clock}
          label="总时长"
          value={formatDuration(stats.total_duration_ms)}
        />
        <StatCard
          icon={HardDrive}
          label="源文件占用"
          value={formatBytes(stats.total_storage_bytes)}
          sub={
            stats.clip_output_bytes > 0
              ? `切片产出 ${formatBytes(stats.clip_output_bytes)}`
              : undefined
          }
        />
        <StatCard
          icon={Users}
          label="主播数"
          value={String(stats.streamer_count)}
        />
      </div>

      {/* Second Row Cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <StatCard
          icon={Clapperboard}
          label="切片任务"
          value={String(stats.clip_total)}
          sub={
            stats.clip_total > 0
              ? `成功率 ${clipSuccessRate}%`
              : undefined
          }
        />
        <StatCard
          icon={CircleCheck}
          label="切片完成"
          value={String(stats.clip_completed)}
        />
        <StatCard
          icon={Subtitles}
          label="已识别字幕"
          value={`${stats.subtitle_video_count} 个视频`}
        />
        <StatCard
          icon={MessageSquareText}
          label="已导入弹幕"
          value={`${stats.danmaku_video_count} 个视频`}
        />
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Top Streamers */}
        {stats.top_streamers.length > 0 && (
          <div className="rounded-lg border p-4 space-y-3">
            <h3 className="font-medium text-sm flex items-center gap-2">
              <Users className="h-4 w-4" />
              主播排行
            </h3>
            <div className="space-y-2">
              {stats.top_streamers.map((s, i) => (
                <div
                  key={s.name}
                  className="flex items-center justify-between text-sm"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-muted-foreground w-4 text-right shrink-0">
                      {i + 1}
                    </span>
                    <span className="truncate">{s.name}</span>
                  </div>
                  <div className="flex items-center gap-3 text-xs text-muted-foreground shrink-0">
                    <span>{s.video_count} 个视频</span>
                    <span>{formatShortDuration(s.total_duration_ms)}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Recent Clips */}
        {stats.recent_clips.length > 0 && (
          <div className="rounded-lg border p-4 space-y-3">
            <h3 className="font-medium text-sm flex items-center gap-2">
              <Layers className="h-4 w-4" />
              最近切片
            </h3>
            <div className="space-y-2">
              {stats.recent_clips.slice(0, 8).map((c) => {
                const st = STATUS_LABELS[c.status] ?? {
                  label: c.status,
                  className: "",
                };
                return (
                  <div
                    key={c.id}
                    className="flex items-center justify-between text-sm"
                  >
                    <span className="truncate min-w-0 flex-1">
                      {c.title || `切片 #${c.id}`}
                    </span>
                    <div className="flex items-center gap-3 shrink-0">
                      <span className={`text-xs ${st.className}`}>
                        {st.label}
                      </span>
                      <span className="text-xs text-muted-foreground w-16 text-right">
                        {c.created_at.slice(5, 16).replace(" ", " ")}
                      </span>
                    </div>
                  </div>
                );
              })}
              {stats.clip_total > 8 && (
                <Link
                  to="/dashboard/tasks"
                  className="text-xs text-primary hover:underline block text-center pt-1"
                >
                  查看全部 →
                </Link>
              )}
            </div>
          </div>
        )}
      </div>

      {/* System Status */}
      {appInfo && (
        <div className="rounded-lg border p-4 space-y-2">
          <h3 className="font-medium text-sm">系统状态</h3>
          <div className="flex flex-wrap gap-x-6 gap-y-1 text-xs text-muted-foreground">
            <span>
              FFmpeg：
              {appInfo.ffmpeg_available ? (
                <span className="text-green-600 dark:text-green-400">
                  ✓ {appInfo.ffmpeg_version?.split(" ").slice(0, 3).join(" ")}
                </span>
              ) : (
                <span className="text-red-500">✗ 未检测到</span>
              )}
            </span>
            <span>
              FFprobe：
              {appInfo.ffprobe_available ? (
                <span className="text-green-600 dark:text-green-400">✓</span>
              ) : (
                <span className="text-red-500">✗</span>
              )}
            </span>
            <span>数据目录：{appInfo.data_dir}</span>
          </div>
        </div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/")({
  component: DashboardIndex,
});
