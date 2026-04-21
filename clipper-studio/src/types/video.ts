export interface VideoInfo {
  id: number;
  file_path: string;
  file_name: string;
  file_hash: string | null;
  file_size: number;
  duration_ms: number | null;
  width: number | null;
  height: number | null;
  format_name: string | null;
  video_codec: string | null;
  audio_codec: string | null;
  has_subtitle: boolean;
  has_danmaku: boolean;
  has_envelope: boolean;
  file_missing: boolean;
  workspace_id: number | null;
  session_id: number | null;
  streamer_id: number | null;
  streamer_name: string | null;
  stream_title: string | null;
  recorded_at: string | null;
  created_at: string;
}

export interface ImportVideoRequest {
  file_path: string;
  workspace_id?: number | null;
}

export interface ListVideosRequest {
  workspace_id?: number | null;
  streamer_id?: number | null;
  sort_by?: "created_at" | "recorded_at";
  sort_order?: "asc" | "desc";
  search?: string;
  date_from?: string;
  date_to?: string;
  page?: number;
  page_size?: number;
  tag_ids?: number[];
}

export interface ListVideosResponse {
  videos: VideoInfo[];
  total: number;
  page: number;
  page_size: number;
}

export interface SessionInfo {
  id: number;
  workspace_id: number;
  streamer_id: number | null;
  streamer_name: string | null;
  title: string | null;
  started_at: string | null;
  file_count: number;
  videos: VideoInfo[];
}

export interface ListSessionsRequest {
  workspace_id?: number | null;
  streamer_id?: number | null;
  sort_order?: "asc" | "desc";
  search?: string;
  date_from?: string;
  date_to?: string;
  page?: number;
  page_size?: number;
}

export interface ListSessionsResponse {
  sessions: SessionInfo[];
  total: number;
  page: number;
  page_size: number;
}

export interface StreamerInfo {
  id: number;
  platform: string;
  room_id: string | null;
  name: string;
  video_count: number;
  total_duration_ms: number | null;
}

export interface ListStreamersRequest {
  workspace_id?: number | null;
  page?: number;
  page_size?: number;
}

export interface ListStreamersResponse {
  streamers: StreamerInfo[];
  total: number;
  page: number;
  page_size: number;
}
