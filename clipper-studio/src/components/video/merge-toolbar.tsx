import { Button } from "@/components/ui/button";

export function MergeToolbar({
  selectedCount,
  mergeMode,
  onMergeModeChange,
  onMerge,
  onCancel,
  merging,
}: {
  selectedCount: number;
  mergeMode: "virtual" | "physical";
  onMergeModeChange: (mode: "virtual" | "physical") => void;
  onMerge: () => void;
  onCancel: () => void;
  merging: boolean;
}) {
  if (selectedCount === 0) return null;

  return (
    <div className="flex items-center gap-3 rounded-lg border bg-accent/30 p-2 text-sm">
      <span className="text-muted-foreground">
        已选 {selectedCount} 个视频
      </span>
      <select
        className="rounded-md border bg-background px-2 py-1 text-xs"
        value={mergeMode}
        onChange={(e) =>
          onMergeModeChange(e.target.value as "virtual" | "physical")
        }
      >
        <option value="virtual">快速合并（要求相同编码）</option>
        <option value="physical">重编码合并（通用）</option>
      </select>
      <Button
        size="sm"
        onClick={onMerge}
        disabled={merging || selectedCount < 2}
      >
        {merging ? "合并中..." : "合并视频"}
      </Button>
      <Button size="sm" variant="ghost" onClick={onCancel}>
        取消选择
      </Button>
    </div>
  );
}
