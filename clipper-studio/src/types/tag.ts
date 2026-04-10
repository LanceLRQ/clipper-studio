export interface TagInfo {
  id: number;
  name: string;
  color: string | null;
}

export interface CreateTagRequest {
  name: string;
  color?: string | null;
}

export interface UpdateTagRequest {
  id: number;
  name?: string;
  color?: string | null;
}

export interface SetVideoTagsRequest {
  video_id: number;
  tag_ids: number[];
}
