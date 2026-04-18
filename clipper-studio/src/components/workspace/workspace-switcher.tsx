import { useEffect, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { FolderOpenIcon, PlusIcon, SettingsIcon, SlidersHorizontalIcon, HardDriveIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { WorkspaceInfo } from "@/types/workspace";
import { listWorkspaces } from "@/services/workspace";
import { useWorkspaceStore } from "@/stores/workspace";
import { AddWorkspaceDialog } from "@/components/workspace/add-workspace-dialog";

export function WorkspaceSwitcher() {
  const navigate = useNavigate();
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const activeId = useWorkspaceStore((s) => s.activeId);
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);

  const loadWorkspaces = () =>
    listWorkspaces().then(setWorkspaces).catch(console.error);

  useEffect(() => {
    loadWorkspaces();
  }, []);

  const isSmbWorkspace = (ws: WorkspaceInfo) => {
    try {
      if (ws.adapter_config) {
        const cfg = JSON.parse(ws.adapter_config);
        return cfg.source === "smb";
      }
    } catch { /* ignore */ }
    return false;
  };

  const activeWorkspace = workspaces.find((w) => w.id === activeId);
  const displayName = activeWorkspace?.name ?? "加载中...";

  if (workspaces.length === 0) {
    return null;
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="inline-flex items-center gap-2 rounded-md border border-input bg-background px-3 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground cursor-pointer"
      >
        <FolderOpenIcon className="h-4 w-4 text-muted-foreground" />
        <span className="max-w-[180px] truncate">{displayName}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        {workspaces.map((ws) => (
          <DropdownMenuItem
            key={ws.id}
            onClick={() => switchWorkspace(ws.id)}
            className={ws.id === activeId ? "bg-accent" : ""}
          >
            <div className="flex flex-col min-w-0">
              <span className="flex items-center gap-1.5">
                {ws.name}
                {isSmbWorkspace(ws) && (
                  <span className="inline-flex items-center gap-0.5 text-[10px] px-1 py-0.5 rounded bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300 shrink-0">
                    <HardDriveIcon className="h-2.5 w-2.5" />
                    SMB
                  </span>
                )}
              </span>
              <span className="text-xs text-muted-foreground truncate">
                {ws.path}
              </span>
            </div>
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => setAddOpen(true)}>
          <PlusIcon className="h-4 w-4 mr-1" />
          添加工作区
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() =>
            navigate({
              to: "/dashboard/settings",
              search: { section: "workspaces" },
            })
          }
        >
          <SettingsIcon className="h-4 w-4 mr-1" />
          管理工作区
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() => navigate({ to: "/dashboard/settings" })}
        >
          <SlidersHorizontalIcon className="h-4 w-4 mr-1" />
          工作区设置
        </DropdownMenuItem>
      </DropdownMenuContent>
      <AddWorkspaceDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        onCreated={() => {
          loadWorkspaces();
          navigate({ to: "/dashboard/videos" });
        }}
      />
    </DropdownMenu>
  );
}
