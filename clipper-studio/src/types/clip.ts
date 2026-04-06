export interface CreateClipRequest {
  video_id: number;
  start_ms: number;
  end_ms: number;
  title?: string;
  preset_id?: number | null;
  output_dir?: string | null;
}

export interface ClipTaskInfo {
  id: number;
  video_id: number;
  start_time_ms: number;
  end_time_ms: number;
  title: string | null;
  status: string;
  progress: number;
  error_message: string | null;
  created_at: string;
  completed_at: string | null;
}

export interface EncodingPreset {
  id: number;
  name: string;
  category: string;
  options: string;
  is_builtin: boolean;
}

export interface TaskProgressEvent {
  task_id: number;
  status: "pending" | "processing" | "completed" | "failed" | "cancelled";
  progress: number;
  message: string;
}
