import { invoke } from "@tauri-apps/api/core";

export interface RecentClipInfo {
  id: number;
  title: string;
  status: string;
  created_at: string;
}

export interface TopStreamerInfo {
  name: string;
  video_count: number;
  total_duration_ms: number;
}

export interface DashboardStats {
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

export async function getDashboardStats(
  workspaceId: number | null
): Promise<DashboardStats> {
  return invoke<DashboardStats>("get_dashboard_stats", { workspaceId });
}
