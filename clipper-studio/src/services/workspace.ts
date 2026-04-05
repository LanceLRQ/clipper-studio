import { invoke } from "@tauri-apps/api/core";
import type {
  WorkspaceInfo,
  CreateWorkspaceRequest,
  AppInfo,
} from "@/types/workspace";

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export async function listWorkspaces(): Promise<WorkspaceInfo[]> {
  return invoke<WorkspaceInfo[]>("list_workspaces");
}

export async function createWorkspace(
  req: CreateWorkspaceRequest
): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("create_workspace", { req });
}

export async function deleteWorkspace(workspaceId: number): Promise<void> {
  return invoke("delete_workspace", { workspaceId });
}

export async function getActiveWorkspace(): Promise<number | null> {
  return invoke<number | null>("get_active_workspace");
}

export async function setActiveWorkspace(
  workspaceId: number | null
): Promise<void> {
  return invoke("set_active_workspace", { workspaceId });
}
