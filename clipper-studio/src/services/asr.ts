import { invoke } from "@tauri-apps/api/core";

export interface SubtitleSegment {
  id: number;
  video_id: number;
  language: string;
  start_ms: number;
  end_ms: number;
  text: string;
  source: string;
}

export interface ASRTaskInfo {
  id: number;
  video_id: number;
  status: string;
  progress: number;
  error_message: string | null;
  retry_count: number;
  segment_count: number | null;
  created_at: string;
  completed_at: string | null;
}

export interface ASRHealthInfo {
  status: string;
  device: string | null;
  model_size: string | null;
}

export async function submitASR(
  videoId: number,
  language?: string,
  force?: boolean
): Promise<number> {
  return invoke<number>("submit_asr", { videoId, language, force });
}

export async function pollASR(asrTaskId: number): Promise<ASRTaskInfo> {
  return invoke<ASRTaskInfo>("poll_asr", { asrTaskId });
}

export async function listASRTasks(
  videoId?: number
): Promise<ASRTaskInfo[]> {
  return invoke<ASRTaskInfo[]>("list_asr_tasks", { videoId });
}

export async function listSubtitles(
  videoId: number
): Promise<SubtitleSegment[]> {
  return invoke<SubtitleSegment[]>("list_subtitles", { videoId });
}

export async function searchSubtitles(
  query: string,
  videoId?: number
): Promise<SubtitleSegment[]> {
  return invoke<SubtitleSegment[]>("search_subtitles", { query, videoId });
}

export async function checkASRHealth(): Promise<ASRHealthInfo> {
  return invoke<ASRHealthInfo>("check_asr_health");
}
