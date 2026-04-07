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
