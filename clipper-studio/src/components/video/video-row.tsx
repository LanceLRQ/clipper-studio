import { Button } from "@/components/ui/button";
import type { VideoInfo } from "@/types/video";
import {
  formatDuration,
  formatFileSize,
  buildVideoTitle,
} from "@/lib/video-utils";

/** Status tag badges for a group of videos */
export function StatusTags({ videos }: { videos: VideoInfo[] }) {
  return (
    <div className="flex gap-1.5 shrink-0">
      {videos.some((v) => v.has_danmaku) && (
        <span className="text-xs px-1.5 py-0.5 rounded bg-blue-100 text-blue-700">
          弹幕
        </span>
      )}
      {videos.some((v) => v.has_subtitle) && (
        <span className="text-xs px-1.5 py-0.5 rounded bg-green-100 text-green-700">
          字幕
        </span>
      )}
      {videos.some((v) => v.has_envelope) && (
        <span className="text-xs px-1.5 py-0.5 rounded bg-orange-100 text-orange-700">
          热度
        </span>
      )}
    </div>
  );
}

export function VideoRow({
  video,
  compact = false,
  indent,
  onNavigate,
  onDelete,
  selected,
  onToggleSelect,
}: {
  video: VideoInfo;
  compact?: boolean;
  indent?: boolean;
  onNavigate: () => void;
  onDelete: (e: React.MouseEvent) => void;
  selected?: boolean;
  onToggleSelect?: (id: number) => void;
}) {
  const title = buildVideoTitle(video);
  const showTitle = title !== video.file_name;

  const paddingClass = indent
    ? "px-4 py-2 pl-16"
    : compact
      ? "px-4 py-2 pl-10"
      : "rounded-lg border p-3";

  return (
    <div
      className={`flex items-center justify-between hover:bg-accent/30 cursor-pointer transition-colors ${paddingClass} ${selected ? "bg-accent/40" : ""}`}
      onClick={onNavigate}
    >
      <div className="flex items-center gap-3 min-w-0">
        {onToggleSelect && (
          <input
            type="checkbox"
            checked={selected ?? false}
            onClick={(e) => e.stopPropagation()}
            onChange={() => onToggleSelect(video.id)}
            className="rounded shrink-0"
          />
        )}
        <div className="min-w-0 space-y-0.5">
          <div className="text-sm truncate font-medium">
            {showTitle ? title : video.file_name}
          </div>
          <div className="flex gap-3 text-xs text-muted-foreground">
            {showTitle && (
              <span className="truncate max-w-[300px]">{video.file_name}</span>
            )}
            <span>{formatDuration(video.duration_ms)}</span>
            <span>{formatFileSize(video.file_size)}</span>
            {video.width && video.height && (
              <span>
                {video.width}x{video.height}
              </span>
            )}
          </div>
        </div>
      </div>
      <div className="flex items-center gap-1.5 shrink-0">
        {video.has_danmaku && (
          <span className="text-xs px-1 py-0.5 rounded bg-blue-100 text-blue-700">
            弹幕
          </span>
        )}
        <Button
          variant="ghost"
          size="sm"
          className="text-red-500 hover:text-red-600 h-7 px-2"
          onClick={onDelete}
        >
          删除
        </Button>
      </div>
    </div>
  );
}
