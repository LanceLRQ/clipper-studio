import { invoke } from "@tauri-apps/api/core";
import type {
  TagInfo,
  CreateTagRequest,
  UpdateTagRequest,
  SetVideoTagsRequest,
} from "@/types/tag";

export async function createTag(req: CreateTagRequest): Promise<TagInfo> {
  return invoke<TagInfo>("create_tag", { req });
}

export async function listTags(): Promise<TagInfo[]> {
  return invoke<TagInfo[]>("list_tags");
}

export async function updateTag(req: UpdateTagRequest): Promise<TagInfo> {
  return invoke<TagInfo>("update_tag", { req });
}

export async function deleteTag(tagId: number): Promise<void> {
  return invoke("delete_tag", { tagId });
}

export async function getVideoTags(videoId: number): Promise<TagInfo[]> {
  return invoke<TagInfo[]>("get_video_tags", { videoId });
}

export async function setVideoTags(req: SetVideoTagsRequest): Promise<TagInfo[]> {
  return invoke<TagInfo[]>("set_video_tags", { req });
}
