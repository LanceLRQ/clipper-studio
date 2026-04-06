import { invoke } from "@tauri-apps/api/core";
import type {
  VideoInfo,
  ImportVideoRequest,
  ListVideosRequest,
  ListVideosResponse,
  SessionInfo,
  StreamerInfo,
} from "@/types/video";

export async function importVideo(req: ImportVideoRequest): Promise<VideoInfo> {
  return invoke<VideoInfo>("import_video", { req });
}

export async function listVideos(
  req: ListVideosRequest
): Promise<ListVideosResponse> {
  return invoke<ListVideosResponse>("list_videos", { req });
}

export async function getVideo(videoId: number): Promise<VideoInfo> {
  return invoke<VideoInfo>("get_video", { videoId });
}

export async function deleteVideo(videoId: number): Promise<void> {
  return invoke("delete_video", { videoId });
}

export async function listSessions(
  workspaceId?: number | null
): Promise<SessionInfo[]> {
  return invoke<SessionInfo[]>("list_sessions", { workspaceId });
}

export async function listStreamers(): Promise<StreamerInfo[]> {
  return invoke<StreamerInfo[]>("list_streamers");
}

export async function extractEnvelope(videoId: number): Promise<number[]> {
  return invoke<number[]>("extract_envelope", { videoId });
}

export async function getEnvelope(
  videoId: number
): Promise<number[] | null> {
  return invoke<number[] | null>("get_envelope", { videoId });
}

export interface IntegrityResult {
  is_intact: boolean;
  issues: string[];
}

export async function checkVideoIntegrity(
  videoId: number
): Promise<IntegrityResult> {
  return invoke<IntegrityResult>("check_video_integrity", { videoId });
}

export async function remuxVideo(videoId: number): Promise<string> {
  return invoke<string>("remux_video", { videoId });
}
