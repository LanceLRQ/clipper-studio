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

export interface SubtitleListResponse {
  segments: SubtitleSegment[];
  base_ms: number;
}

export async function listSubtitles(
  videoId: number
): Promise<SubtitleListResponse> {
  return invoke<SubtitleListResponse>("list_subtitles", { videoId });
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

// ==================== Subtitle Editing ====================

export async function updateSubtitle(
  segmentId: number,
  text: string,
  startMs: number,
  endMs: number
): Promise<void> {
  return invoke("update_subtitle", { segmentId, text, startMs, endMs });
}

export async function deleteSubtitle(segmentId: number): Promise<void> {
  return invoke("delete_subtitle", { segmentId });
}

export async function mergeSubtitles(
  segmentIds: number[]
): Promise<SubtitleSegment> {
  return invoke<SubtitleSegment>("merge_subtitles", { segmentIds });
}

export async function splitSubtitle(
  segmentId: number,
  splitAtMs: number
): Promise<[SubtitleSegment, SubtitleSegment]> {
  return invoke<[SubtitleSegment, SubtitleSegment]>("split_subtitle", {
    segmentId,
    splitAtMs,
  });
}

// ==================== Subtitle Export ====================

export async function exportSubtitlesSrt(videoId: number): Promise<string> {
  return invoke<string>("export_subtitles_srt", { videoId });
}

export async function exportSubtitlesAss(videoId: number): Promise<string> {
  return invoke<string>("export_subtitles_ass", { videoId });
}

export async function exportSubtitlesVtt(videoId: number): Promise<string> {
  return invoke<string>("export_subtitles_vtt", { videoId });
}
