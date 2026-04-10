import { useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { ask } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import type { ClipRegion, ClipOptions } from "@/types/multi-clip";
import { createBatchClips } from "@/services/clip";

interface ClipActionsProps {
  videoId: number;
  clips: ClipRegion[];
  presetId?: number | null;
  clipOptions?: Record<string, ClipOptions>;
  disabled?: boolean;
}

export function ClipActions({
  videoId,
  clips,
  presetId,
  clipOptions = {},
  disabled = false,
}: ClipActionsProps) {
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);

  const validClips = clips.filter((c) => c.end > c.start);

  const handleSubmit = async () => {
    if (validClips.length === 0) return;
    if (!(await ask(`将创建 ${validClips.length} 个切片任务，确认？`, { title: "创建切片" }))) return;

    setLoading(true);
    try {
      await createBatchClips({
        video_id: videoId,
        clips: validClips.map((clip) => {
          const opts = clipOptions[clip.id];
          return {
            start_ms: Math.round(clip.start * 1000),
            end_ms: Math.round(clip.end * 1000),
            title: clip.name,
            preset_id: presetId,
            offset_before_ms: Math.round((opts?.clip_offset_before ?? 0) * 1000),
            offset_after_ms: Math.round((opts?.clip_offset_after ?? 0) * 1000),
            audio_only: opts?.audio_only ?? false,
            include_danmaku: opts?.include_danmaku ?? false,
            include_subtitle: opts?.include_subtitle ?? false,
          };
        }),
      });
      navigate({ to: "/dashboard/tasks" });
    } catch (e) {
      alert("创建切片任务失败: " + String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Button
      className="w-full"
      onClick={handleSubmit}
      disabled={loading || validClips.length === 0 || disabled}
    >
      {loading
        ? "创建中..."
        : `创建切片任务（${validClips.length} 个片段）`}
    </Button>
  );
}
