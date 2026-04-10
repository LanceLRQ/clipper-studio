import { useState, useEffect, useCallback } from "react";
import { PlusIcon, PencilIcon, TrashIcon } from "lucide-react";
import { ask } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { TagBadge, TAG_COLORS } from "@/components/tag/tag-badge";
import type { TagInfo } from "@/types/tag";
import { listTags, createTag, updateTag, deleteTag } from "@/services/tag";

export function TagManager() {
  const [tags, setTags] = useState<TagInfo[]>([]);
  const [loading, setLoading] = useState(true);

  // Dialog state
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingTag, setEditingTag] = useState<TagInfo | null>(null);
  const [formName, setFormName] = useState("");
  const [formColor, setFormColor] = useState(TAG_COLORS[0].value);
  const [saving, setSaving] = useState(false);

  const loadTags = useCallback(async () => {
    setLoading(true);
    try {
      const data = await listTags();
      setTags(data);
    } catch (e) {
      console.error("Failed to load tags:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTags();
  }, [loadTags]);

  const openCreateDialog = () => {
    setEditingTag(null);
    setFormName("");
    setFormColor(TAG_COLORS[0].value);
    setDialogOpen(true);
  };

  const openEditDialog = (tag: TagInfo) => {
    setEditingTag(tag);
    setFormName(tag.name);
    setFormColor(tag.color || TAG_COLORS[0].value);
    setDialogOpen(true);
  };

  const handleSave = async () => {
    if (!formName.trim()) return;
    setSaving(true);
    try {
      if (editingTag) {
        await updateTag({
          id: editingTag.id,
          name: formName.trim(),
          color: formColor,
        });
      } else {
        await createTag({
          name: formName.trim(),
          color: formColor,
        });
      }
      setDialogOpen(false);
      loadTags();
    } catch (e) {
      alert(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (tag: TagInfo) => {
    const confirmed = await ask(
      `确定要删除标签 "${tag.name}" 吗？\n该标签会从所有视频中移除。`,
      { title: "删除标签", kind: "warning" }
    );
    if (!confirmed) return;
    try {
      await deleteTag(tag.id);
      loadTags();
    } catch (e) {
      alert("删除失败: " + String(e));
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-medium text-lg">标签管理</h3>
        <Button size="sm" className="gap-1" onClick={openCreateDialog}>
          <PlusIcon className="h-4 w-4" />
          新建标签
        </Button>
      </div>

      <p className="text-sm text-muted-foreground">
        创建标签并为视频分类，方便筛选和管理。
      </p>

      {loading ? (
        <div className="text-sm text-muted-foreground">加载中...</div>
      ) : tags.length === 0 ? (
        <div className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
          还没有标签，点击上方按钮创建第一个标签吧！
        </div>
      ) : (
        <div className="rounded-lg border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/50">
                <th className="text-left px-4 py-2 font-medium">标签</th>
                <th className="text-left px-4 py-2 font-medium">颜色</th>
                <th className="text-right px-4 py-2 font-medium w-24">操作</th>
              </tr>
            </thead>
            <tbody>
              {tags.map((tag) => (
                <tr key={tag.id} className="border-b last:border-0 hover:bg-muted/30">
                  <td className="px-4 py-2">
                    <TagBadge tag={tag} size="md" />
                  </td>
                  <td className="px-4 py-2">
                    <span className="flex items-center gap-2 text-xs text-muted-foreground">
                      <span
                        className="h-3 w-3 rounded-full"
                        style={{ backgroundColor: tag.color || "#6b7280" }}
                      />
                      {tag.color || "默认"}
                    </span>
                  </td>
                  <td className="px-4 py-2 text-right">
                    <div className="flex items-center justify-end gap-1">
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        onClick={() => openEditDialog(tag)}
                        title="编辑"
                      >
                        <PencilIcon className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        onClick={() => handleDelete(tag)}
                        title="删除"
                        className="text-destructive hover:text-destructive"
                      >
                        <TrashIcon className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Create / Edit Dialog */}
      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{editingTag ? "编辑标签" : "新建标签"}</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-2">
            <div className="space-y-1">
              <Label className="text-sm">名称</Label>
              <Input
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
                placeholder="输入标签名称"
                className="h-8 text-sm"
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    handleSave();
                  }
                }}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-sm">颜色</Label>
              <div className="flex flex-wrap gap-2">
                {TAG_COLORS.map((c) => (
                  <button
                    key={c.value}
                    type="button"
                    className={`h-6 w-6 rounded-full border-2 transition-all ${
                      formColor === c.value
                        ? "border-foreground scale-110"
                        : "border-transparent hover:scale-105"
                    }`}
                    style={{ backgroundColor: c.value }}
                    onClick={() => setFormColor(c.value)}
                    title={c.name}
                  />
                ))}
              </div>
            </div>
            <div className="space-y-1">
              <Label className="text-sm">预览</Label>
              <div>
                <TagBadge
                  tag={{ id: 0, name: formName || "示例标签", color: formColor }}
                  size="md"
                />
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button onClick={handleSave} disabled={saving || !formName.trim()}>
              {saving ? "保存中..." : "保存"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
