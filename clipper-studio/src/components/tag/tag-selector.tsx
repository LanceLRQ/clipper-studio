import { useState, useEffect } from "react";
import { CheckIcon, PlusIcon, TagIcon } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { TagBadge, TAG_COLORS } from "@/components/tag/tag-badge";
import type { TagInfo } from "@/types/tag";
import { listTags, createTag } from "@/services/tag";

interface TagSelectorProps {
  selectedTags: TagInfo[];
  onChange: (tags: TagInfo[]) => void;
}

export function TagSelector({ selectedTags, onChange }: TagSelectorProps) {
  const [allTags, setAllTags] = useState<TagInfo[]>([]);
  const [newTagName, setNewTagName] = useState("");
  const [creating, setCreating] = useState(false);

  const loadTags = async () => {
    try {
      const tags = await listTags();
      setAllTags(tags);
    } catch {
      // ignore
    }
  };

  useEffect(() => {
    loadTags();
  }, []);

  const selectedIds = new Set(selectedTags.map((t) => t.id));

  const toggleTag = (tag: TagInfo) => {
    if (selectedIds.has(tag.id)) {
      onChange(selectedTags.filter((t) => t.id !== tag.id));
    } else {
      onChange([...selectedTags, tag]);
    }
  };

  const handleCreateTag = async () => {
    const name = newTagName.trim();
    if (!name || creating) return;
    setCreating(true);
    try {
      // Pick a random color from presets
      const color =
        TAG_COLORS[Math.floor(Math.random() * TAG_COLORS.length)].value;
      const tag = await createTag({ name, color });
      setAllTags((prev) => [...prev, tag].sort((a, b) => a.name.localeCompare(b.name)));
      onChange([...selectedTags, tag]);
      setNewTagName("");
    } catch (e) {
      alert("创建标签失败: " + String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button variant="outline" size="sm" className="h-7 text-xs gap-1" />
        }
      >
        <TagIcon className="h-3 w-3" />
        标签
      </PopoverTrigger>
      <PopoverContent className="w-56 p-2 space-y-2">
        <p className="text-xs text-muted-foreground font-medium px-1">选择标签</p>
        {allTags.length === 0 && (
          <p className="text-xs text-muted-foreground px-1">暂无标签</p>
        )}
        <div className="max-h-40 overflow-y-auto space-y-0.5">
          {allTags.map((tag) => (
            <button
              key={tag.id}
              type="button"
              className="flex items-center gap-2 w-full rounded px-2 py-1 text-xs hover:bg-accent"
              onClick={() => toggleTag(tag)}
            >
              <span
                className="h-3 w-3 rounded-full shrink-0"
                style={{ backgroundColor: tag.color || "#6b7280" }}
              />
              <span className="flex-1 text-left truncate">{tag.name}</span>
              {selectedIds.has(tag.id) && (
                <CheckIcon className="h-3 w-3 text-primary shrink-0" />
              )}
            </button>
          ))}
        </div>
        <div className="border-t pt-2">
          <div className="flex gap-1">
            <Input
              value={newTagName}
              onChange={(e) => setNewTagName(e.target.value)}
              placeholder="新标签..."
              className="h-6 text-xs flex-1"
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleCreateTag();
                }
              }}
            />
            <Button
              variant="ghost"
              size="icon-sm"
              className="h-6 w-6 shrink-0"
              onClick={handleCreateTag}
              disabled={!newTagName.trim() || creating}
            >
              <PlusIcon className="h-3 w-3" />
            </Button>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
