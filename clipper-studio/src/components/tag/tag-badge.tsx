import { XIcon } from "lucide-react";
import type { TagInfo } from "@/types/tag";

// Predefined tag colors
export const TAG_COLORS = [
  { name: "蓝色", value: "#3b82f6" },
  { name: "绿色", value: "#22c55e" },
  { name: "红色", value: "#ef4444" },
  { name: "紫色", value: "#a855f7" },
  { name: "橙色", value: "#f97316" },
  { name: "粉色", value: "#ec4899" },
  { name: "青色", value: "#06b6d4" },
  { name: "黄色", value: "#eab308" },
  { name: "靛蓝", value: "#6366f1" },
  { name: "灰色", value: "#6b7280" },
] as const;

const DEFAULT_COLOR = "#6b7280";

interface TagBadgeProps {
  tag: TagInfo;
  size?: "sm" | "md";
  removable?: boolean;
  onRemove?: () => void;
  onClick?: () => void;
}

export function TagBadge({
  tag,
  size = "sm",
  removable = false,
  onRemove,
  onClick,
}: TagBadgeProps) {
  const color = tag.color || DEFAULT_COLOR;
  const sizeClasses =
    size === "sm"
      ? "text-xs px-1.5 py-0.5 gap-0.5"
      : "text-sm px-2 py-0.5 gap-1";

  return (
    <span
      className={`inline-flex items-center rounded-full font-medium ${sizeClasses} ${onClick ? "cursor-pointer hover:opacity-80" : ""}`}
      style={{
        backgroundColor: `${color}20`,
        color: color,
        border: `1px solid ${color}40`,
      }}
      onClick={onClick}
    >
      {tag.name}
      {removable && onRemove && (
        <button
          type="button"
          className="inline-flex items-center justify-center rounded-full hover:bg-black/10"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          <XIcon className="h-3 w-3" />
        </button>
      )}
    </span>
  );
}
