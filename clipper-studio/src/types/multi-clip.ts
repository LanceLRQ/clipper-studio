export interface ClipRegion {
  /** Unique ID, e.g. "clip-1712345678" */
  id: string;
  /** Start time in seconds */
  start: number;
  /** End time in seconds */
  end: number;
  /** Hex color from palette */
  color: string;
  /** Display name, e.g. "片段1" */
  name: string;
}

export interface ClipOptions {
  /** Extend start by N seconds */
  clip_offset_before: number;
  /** Extend end by N seconds */
  clip_offset_after: number;
  /** Extract audio only */
  audio_only: boolean;
}

export interface BatchClipItem {
  start_ms: number;
  end_ms: number;
  title?: string;
  preset_id?: number | null;
  offset_before_ms: number;
  offset_after_ms: number;
  audio_only: boolean;
}

export interface BatchClipRequest {
  video_id: number;
  clips: BatchClipItem[];
}
