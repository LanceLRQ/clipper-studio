import { create } from "zustand";
import {
  getActiveWorkspace,
  setActiveWorkspace as setActiveWorkspaceApi,
  listWorkspaces,
  checkWorkspacePath,
} from "@/services/workspace";

interface WorkspaceState {
  /** 当前激活的工作区 ID，null 仅在初始化阶段出现 */
  activeId: number | null;
  /** 递增版本号，任何页面监听此值即可响应工作区切换 */
  version: number;
  initialized: boolean;
  /** 没有任何工作区，需引导用户创建 */
  noWorkspaces: boolean;
  /** 当前工作区路径是否可达 */
  pathAccessible: boolean;
  initialize: () => Promise<void>;
  switchWorkspace: (id: number) => Promise<void>;
  /** Re-check path accessibility for current workspace */
  recheckPath: () => Promise<void>;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  activeId: null,
  version: 0,
  initialized: false,
  noWorkspaces: false,
  pathAccessible: true,

  async initialize() {
    if (get().initialized) return;
    try {
      const id = await getActiveWorkspace();
      if (id != null) {
        const accessible = await checkWorkspacePath(id).catch(() => true);
        set({ activeId: id, initialized: true, noWorkspaces: false, pathAccessible: accessible });
        return;
      }
      // No active workspace persisted — try to auto-select the first one
      const workspaces = await listWorkspaces();
      if (workspaces.length > 0) {
        const firstId = workspaces[0].id;
        await setActiveWorkspaceApi(firstId);
        const accessible = await checkWorkspacePath(firstId).catch(() => true);
        set({ activeId: firstId, initialized: true, noWorkspaces: false, pathAccessible: accessible });
      } else {
        set({ initialized: true, noWorkspaces: true });
      }
    } catch {
      set({ initialized: true });
    }
  },

  async switchWorkspace(id: number) {
    await setActiveWorkspaceApi(id);
    const accessible = await checkWorkspacePath(id).catch(() => true);
    set((s) => ({ activeId: id, version: s.version + 1, noWorkspaces: false, pathAccessible: accessible }));
  },

  async recheckPath() {
    const id = get().activeId;
    if (id == null) return;
    const accessible = await checkWorkspacePath(id).catch(() => true);
    set({ pathAccessible: accessible });
  },
}));
