import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import type { WorkspaceInfo } from "@/types/workspace";
import { listWorkspaces, deleteWorkspace, scanWorkspace } from "@/services/workspace";
import { useWorkspaceStore } from "@/stores/workspace";

function WorkspacesPage() {
  const navigate = useNavigate();
  const activeId = useWorkspaceStore((s) => s.activeId);
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const loadWorkspaces = async () => {
    try {
      const ws = await listWorkspaces();
      setWorkspaces(ws);
    } catch (e) {
      console.error("Failed to load workspaces:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadWorkspaces();
  }, []);

  const handleDelete = async (ws: WorkspaceInfo) => {
    if (!(await ask(`确定要删除工作区「${ws.name}」吗？\n\n注意：仅删除工作区记录，不会删除磁盘上的文件。`, { title: "删除工作区", kind: "warning" }))) {
      return;
    }
    try {
      await deleteWorkspace(ws.id);
      const remaining = await listWorkspaces();
      setWorkspaces(remaining);
      // If deleted workspace was the active one, switch or redirect
      if (ws.id === activeId) {
        if (remaining.length > 0) {
          await switchWorkspace(remaining[0].id);
        } else {
          navigate({ to: "/welcome", replace: true });
          return;
        }
      }
    } catch (e) {
      console.error("Failed to delete workspace:", e);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-semibold">工作区管理</h2>
        <Button onClick={() => navigate({ to: "/welcome" })}>
          + 添加工作区
        </Button>
      </div>

      {loading ? (
        <div className="text-muted-foreground">加载中...</div>
      ) : workspaces.length === 0 ? (
        <div className="rounded-lg border border-dashed p-12 text-center">
          <p className="text-muted-foreground mb-4">还没有工作区</p>
          <Button onClick={() => navigate({ to: "/welcome" })}>
            添加第一个工作区
          </Button>
        </div>
      ) : (
        <div className="space-y-3">
          {workspaces.map((ws) => (
            <div
              key={ws.id}
              className="flex items-center justify-between rounded-lg border p-4"
            >
              <div className="space-y-1">
                <div className="font-medium">{ws.name}</div>
                <div className="text-sm text-muted-foreground">{ws.path}</div>
                <div className="flex gap-3 text-xs text-muted-foreground">
                  <span>适配器: {ws.adapter_id}</span>
                  <span>创建于: {ws.created_at}</span>
                </div>
              </div>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={async () => {
                    try {
                      const result = await scanWorkspace(ws.id);
                      alert(`扫描完成：${result.total_files} 个视频，${result.total_sessions} 个场次`);
                    } catch (e) {
                      alert("扫描失败: " + String(e));
                    }
                  }}
                >
                  重新扫描
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-red-500 hover:text-red-600"
                  onClick={() => handleDelete(ws)}
                >
                  删除
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/workspaces")({
  component: WorkspacesPage,
});
