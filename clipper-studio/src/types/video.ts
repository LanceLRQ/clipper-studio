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
  page?: number;
  page_size?: number;
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

export interface StreamerInfo {
  id: number;
  platform: string;
  room_id: string | null;
  name: string;
  video_count: number;
}
