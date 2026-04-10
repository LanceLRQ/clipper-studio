import { useState, useEffect } from "react";
import { TagIcon } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import type { TagInfo } from "@/types/tag";
import { listTags } from "@/services/tag";

interface TagFilterProps {
  selectedTagIds: number[];
  onChange: (tagIds: number[]) => void;
}

export function TagFilter({ selectedTagIds, onChange }: TagFilterProps) {
  const [allTags, setAllTags] = useState<TagInfo[]>([]);

  useEffect(() => {
    listTags()
      .then(setAllTags)
      .catch(() => {});
  }, []);

  if (allTags.length === 0) return null;

  const selectedSet = new Set(selectedTagIds);
  const count = selectedTagIds.length;

  const toggleTag = (tagId: number) => {
    if (selectedSet.has(tagId)) {
      onChange(selectedTagIds.filter((id) => id !== tagId));
    } else {
      onChange([...selectedTagIds, tagId]);
    }
  };

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            variant="outline"
            size="sm"
            className={`h-8 text-xs gap-1 ${count > 0 ? "border-primary text-primary" : ""}`}
          />
        }
      >
        <TagIcon className="h-3 w-3" />
        {count > 0 ? `已选 ${count} 个标签` : "标签筛选"}
      </PopoverTrigger>
      <PopoverContent className="w-48 p-2">
        <p className="text-xs text-muted-foreground font-medium px-1 mb-1">
          按标签筛选
        </p>
        <div className="max-h-48 overflow-y-auto space-y-0.5">
          {allTags.map((tag) => (
            <button
              key={tag.id}
              type="button"
              className={`flex items-center gap-2 w-full rounded px-2 py-1 text-xs hover:bg-accent ${
                selectedSet.has(tag.id) ? "bg-accent" : ""
              }`}
              onClick={() => toggleTag(tag.id)}
            >
              <span
                className="h-3 w-3 rounded-full shrink-0"
                style={{ backgroundColor: tag.color || "#6b7280" }}
              />
              <span className="flex-1 text-left truncate">{tag.name}</span>
              {selectedSet.has(tag.id) && (
                <span className="text-primary text-xs">✓</span>
              )}
            </button>
          ))}
        </div>
        {count > 0 && (
          <div className="border-t mt-1 pt-1">
            <button
              type="button"
              className="text-xs text-muted-foreground hover:text-foreground w-full text-left px-2 py-1"
              onClick={() => onChange([])}
            >
              清除筛选
            </button>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
