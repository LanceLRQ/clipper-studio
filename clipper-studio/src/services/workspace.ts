import { invoke } from "@tauri-apps/api/core";
import type {
  WorkspaceInfo,
  CreateWorkspaceRequest,
  UpdateWorkspaceRequest,
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

export async function updateWorkspace(
  req: UpdateWorkspaceRequest
): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("update_workspace", { req });
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

export interface ScanResult {
  new_files: number;
  total_files: number;
  total_sessions: number;
  streamers: number;
}

/** 启动扫描任务，返回 task_id。真实进度通过 `task-progress` 事件推送 */
export async function scanWorkspace(
  workspaceId: number
): Promise<number> {
  return invoke<number>("scan_workspace", { workspaceId });
}

/** 取消正在执行的扫描任务 */
export async function cancelScan(taskId: number): Promise<boolean> {
  return invoke<boolean>("cancel_scan", { taskId });
}

export type ScanStage =
  | "scanning"
  | "probing"
  | "grouping"
  | "writing";

export interface ScanProgressPayload {
  stage: ScanStage;
  current?: number;
  total?: number;
  file?: string;
  path?: string;
  result?: ScanResult;
}

export async function detectWorkspaceAdapter(
  path: string
): Promise<string> {
  return invoke<string>("detect_workspace_adapter", { path });
}

/** Check if workspace directory path is accessible */
export async function checkWorkspacePath(
  workspaceId: number
): Promise<boolean> {
  return invoke<boolean>("check_workspace_path", { workspaceId });
}

export interface DiskUsageInfo {
  output_dir: string;
  dir_size_bytes: number;
  disk_total_bytes: number;
  disk_available_bytes: number;
}

export async function getDiskUsage(
  workspaceId: number
): Promise<DiskUsageInfo> {
  return invoke<DiskUsageInfo>("get_disk_usage", { workspaceId });
}
