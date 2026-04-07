import { invoke } from "@tauri-apps/api/core";

export interface MediaTaskInfo {
  id: number;
  task_type: string;
  video_ids: number[];
  output_path: string | null;
  status: string;
  progress: number;
  error_message: string | null;
  created_at: string;
  completed_at: string | null;
}

export interface TranscodeRequest {
  video_id: number;
  preset_id: number;
  output_dir?: string;
}

export async function transcodeVideo(
  req: TranscodeRequest,
): Promise<MediaTaskInfo> {
  return invoke<MediaTaskInfo>("transcode_video", { req });
}

export async function listMediaTasks(
  taskType?: string,
): Promise<MediaTaskInfo[]> {
  return invoke<MediaTaskInfo[]>("list_media_tasks", { taskType });
}

export interface MergeRequest {
  video_ids: number[];
  mode: string;
  preset_id?: number;
  output_dir?: string;
}

export async function mergeVideos(
  req: MergeRequest,
): Promise<MediaTaskInfo> {
  return invoke<MediaTaskInfo>("merge_videos", { req });
}
