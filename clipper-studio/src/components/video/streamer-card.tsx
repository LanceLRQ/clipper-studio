import { useNavigate } from "@tanstack/react-router";
import { VideoIcon } from "lucide-react";
import type { StreamerInfo } from "@/types/video";
import { formatDuration } from "@/lib/video-utils";

export function StreamerCard({ streamer }: { streamer: StreamerInfo }) {
  const navigate = useNavigate();

  return (
    <div
      className="rounded-lg border p-4 hover:bg-accent/30 cursor-pointer transition-colors space-y-2"
      onClick={() =>
        navigate({
          to: "/dashboard/videos/streamer/$streamerId",
          params: { streamerId: String(streamer.id) },
        })
      }
    >
      <div className="flex items-center gap-2">
        <span className="font-medium text-primary truncate">
          {streamer.name}
        </span>
      </div>
      {streamer.room_id && (
        <div className="text-xs text-muted-foreground">
          房间号 {streamer.room_id}
        </div>
      )}
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span className="flex items-center gap-1">
          <VideoIcon className="h-3 w-3" />
          {streamer.video_count} 个视频
        </span>
        {streamer.total_duration_ms && (
          <span>{formatDuration(streamer.total_duration_ms)}</span>
        )}
      </div>
    </div>
  );
}

/** Special card for videos without a streamer */
export function UnassociatedCard({
  count,
  onClick,
}: {
  count: number;
  onClick: () => void;
}) {
  if (count === 0) return null;

  return (
    <div
      className="rounded-lg border border-dashed p-4 hover:bg-accent/30 cursor-pointer transition-colors space-y-2"
      onClick={onClick}
    >
      <div className="font-medium text-muted-foreground">未关联主播</div>
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span className="flex items-center gap-1">
          <VideoIcon className="h-3 w-3" />
          {count} 个视频
        </span>
      </div>
    </div>
  );
}
