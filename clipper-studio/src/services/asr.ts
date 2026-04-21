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

export interface SubtitleSearchResult {
  id: number;
  video_id: number;
  language: string;
  start_ms: number;
  end_ms: number;
  text: string;
  source: string;
  video_file_name: string;
  video_duration_ms: number | null;
  streamer_name: string | null;
  stream_title: string | null;
  recorded_at: string | null;
}

export async function searchSubtitlesGlobal(
  query: string
): Promise<SubtitleSearchResult[]> {
  return invoke<SubtitleSearchResult[]>("search_subtitles_global", { query });
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

// ==================== ASR Service Management ====================

export interface ASRPathValidation {
  valid: boolean;
  has_python_env: boolean;
  has_main: boolean;
  python_path: string;
  platform: "windows" | "macos" | "linux";
  setup_hint: string | null;
}

export interface ASRServiceStatusInfo {
  status: "stopped" | "starting" | "running" | "stopping" | "error";
  health_info: ASRHealthInfo | null;
  /** "native" | "docker" — which backend is (was) used for the current run */
  launch_kind: "native" | "docker" | null;
  message?: string;
}

// ==================== Docker mode ====================

export interface DockerCapability {
  installed: boolean;
  daemon_running: boolean;
  /** "amd64" | "arm64" | "unknown" */
  host_arch: string;
  /** "macos" | "windows" | "linux" */
  host_platform: string;
  version: string | null;
  hint: string | null;
}

/** Error code returned by start_asr_service when a conflicting container exists */
export const ERR_CONTAINER_CONFLICT = "DOCKER_CONTAINER_CONFLICT";

export async function checkDockerCapability(): Promise<DockerCapability> {
  return invoke<DockerCapability>("check_docker_capability");
}

export async function checkDockerImagePulled(image: string): Promise<boolean> {
  return invoke<boolean>("check_docker_image_pulled", { image });
}

export async function openDockerPullTerminal(image: string): Promise<void> {
  return invoke("open_docker_pull_terminal", { image });
}

export async function forceRemoveASRContainer(): Promise<void> {
  return invoke("force_remove_asr_container");
}

export async function getDefaultASRDockerDataDir(): Promise<string> {
  return invoke<string>("get_default_asr_docker_data_dir");
}

export async function validateASRPath(
  path: string
): Promise<ASRPathValidation> {
  return invoke<ASRPathValidation>("validate_asr_path", { path });
}

export async function startASRService(): Promise<void> {
  return invoke("start_asr_service");
}

export async function stopASRService(): Promise<void> {
  return invoke("stop_asr_service");
}

export async function openASRSetupTerminal(): Promise<void> {
  return invoke("open_asr_setup_terminal");
}

export async function getASRServiceStatus(): Promise<ASRServiceStatusInfo> {
  return invoke<ASRServiceStatusInfo>("get_asr_service_status");
}

export async function getASRServiceLogs(
  limit?: number
): Promise<string[]> {
  return invoke<string[]>("get_asr_service_logs", { limit });
}

// ==================== ASR Task Queue ====================

export interface ASRQueueItem {
  task_id: number;
  video_id: number;
  video_file_name: string;
  status: string;
  progress: number;
  message?: string;
  error_message?: string;
}

export interface ASRTaskProgressEvent {
  task_id: number;
  video_id: number;
  status: string;
  progress: number;
  message?: string;
  error_message?: string;
  video_file_name: string;
}

export async function submitASRQueued(
  videoId: number,
  language?: string,
): Promise<number> {
  return invoke<number>("submit_asr_queued", { videoId, language });
}

export async function cancelASRTask(asrTaskId: number): Promise<boolean> {
  return invoke<boolean>("cancel_asr_task", { asrTaskId });
}

export async function getASRQueueSnapshot(): Promise<ASRQueueItem[]> {
  return invoke<ASRQueueItem[]>("get_asr_queue_snapshot");
}
