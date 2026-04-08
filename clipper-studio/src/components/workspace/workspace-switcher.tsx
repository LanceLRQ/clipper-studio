import { useEffect, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { FolderOpenIcon, PlusIcon, SettingsIcon } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { WorkspaceInfo } from "@/types/workspace";
import {
  listWorkspaces,
  getActiveWorkspace,
  setActiveWorkspace,
} from "@/services/workspace";

export function WorkspaceSwitcher() {
  const navigate = useNavigate();
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [activeId, setActiveId] = useState<number | null>(null);

  const loadData = async () => {
    try {
      const [ws, active] = await Promise.all([
        listWorkspaces(),
        getActiveWorkspace(),
      ]);
      setWorkspaces(ws);
      setActiveId(active);
    } catch (e) {
      console.error("Failed to load workspaces:", e);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const activeWorkspace = workspaces.find((w) => w.id === activeId);
  const displayName = activeId === null ? "全部工作区" : activeWorkspace?.name ?? "未知";

  const handleSwitch = async (id: number | null) => {
    await setActiveWorkspace(id);
    setActiveId(id);
  };

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
        <DropdownMenuItem
          onClick={() => handleSwitch(null)}
          className={activeId === null ? "bg-accent" : ""}
        >
          全部工作区
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        {workspaces.map((ws) => (
          <DropdownMenuItem
            key={ws.id}
            onClick={() => handleSwitch(ws.id)}
            className={ws.id === activeId ? "bg-accent" : ""}
          >
            <div className="flex flex-col">
              <span>{ws.name}</span>
              <span className="text-xs text-muted-foreground truncate">
                {ws.path}
              </span>
            </div>
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onClick={() => navigate({ to: "/welcome" })}
        >
          <PlusIcon className="h-4 w-4 mr-1" />
          添加工作区
        </DropdownMenuItem>
        <DropdownMenuItem
          onClick={() => navigate({ to: "/dashboard/workspaces" })}
        >
          <SettingsIcon className="h-4 w-4 mr-1" />
          管理工作区
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
