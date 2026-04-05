import { invoke } from "@tauri-apps/api/core";
import type {
  VideoInfo,
  ImportVideoRequest,
  ListVideosRequest,
  ListVideosResponse,
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
