import { invoke } from "@tauri-apps/api/core";
import type {
  CreateClipRequest,
  ClipTaskInfo,
  EncodingPreset,
} from "@/types/clip";

export async function createClip(
  req: CreateClipRequest
): Promise<ClipTaskInfo> {
  return invoke<ClipTaskInfo>("create_clip", { req });
}

export async function cancelClip(taskId: number): Promise<boolean> {
  return invoke<boolean>("cancel_clip", { taskId });
}

export async function listClipTasks(
  videoId?: number | null
): Promise<ClipTaskInfo[]> {
  return invoke<ClipTaskInfo[]>("list_clip_tasks", { videoId });
}

export async function listPresets(): Promise<EncodingPreset[]> {
  return invoke<EncodingPreset[]>("list_presets");
}

export async function deleteClipTask(
  taskId: number,
  deleteFiles = false
): Promise<void> {
  return invoke<void>("delete_clip_task", { taskId, deleteFiles });
}

export interface DeleteBatchResult {
  deleted: number;
  skipped: number;
}

export async function deleteClipBatch(
  batchId: string,
  deleteFiles = false
): Promise<DeleteBatchResult> {
  return invoke<DeleteBatchResult>("delete_clip_batch", { batchId, deleteFiles });
}

export async function clearFinishedClipTasks(
  deleteFiles = false
): Promise<number> {
  return invoke<number>("clear_finished_clip_tasks", { deleteFiles });
}

import type { BatchClipRequest, BurnAvailability } from "@/types/multi-clip";

export async function createBatchClips(
  req: BatchClipRequest
): Promise<ClipTaskInfo[]> {
  return invoke<ClipTaskInfo[]>("create_batch_clips", { req });
}

/** Check what burn options (danmaku/subtitle) are available for a video */
export async function checkVideoBurnAvailability(
  videoId: number
): Promise<BurnAvailability> {
  return invoke<BurnAvailability>("check_video_burn_availability", { videoId });
}

/** Auto-detect segment boundaries from audio silence */
export interface DetectedSegment {
  start_ms: number;
  end_ms: number;
}

export async function autoSegment(
  videoId: number,
  silenceThreshold?: number,
  minSilenceMs?: number,
  minSegmentMs?: number,
): Promise<DetectedSegment[]> {
  return invoke<DetectedSegment[]>("auto_segment", {
    videoId,
    silenceThreshold,
    minSilenceMs,
    minSegmentMs,
  });
}
