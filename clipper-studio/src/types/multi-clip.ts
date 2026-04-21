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
  /** Burn danmaku overlay into output video */
  include_danmaku: boolean;
  /** Burn subtitle overlay into output video */
  include_subtitle: boolean;
  /** Export subtitle as SRT file alongside video */
  export_subtitle: boolean;
  /** Export danmaku as XML file alongside video */
  export_danmaku: boolean;
}

export interface BatchClipItem {
  start_ms: number;
  end_ms: number;
  title?: string;
  preset_id?: number | null;
  offset_before_ms: number;
  offset_after_ms: number;
  audio_only: boolean;
  include_danmaku: boolean;
  include_subtitle: boolean;
  export_subtitle: boolean;
  export_danmaku: boolean;
}

/** Burn availability info returned by check_video_burn_availability */
export interface BurnAvailability {
  has_danmaku_xml: boolean;
  has_subtitle: boolean;
  has_danmaku_factory: boolean;
}

export interface BatchClipRequest {
  video_id: number;
  clips: BatchClipItem[];
}
