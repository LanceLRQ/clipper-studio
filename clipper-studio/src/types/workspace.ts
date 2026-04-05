export interface WorkspaceInfo {
  id: number;
  name: string;
  path: string;
  adapter_id: string;
  auto_scan: boolean;
  created_at: string;
}

export interface CreateWorkspaceRequest {
  name: string;
  path: string;
  adapter_id: string;
}

export interface AppInfo {
  version: string;
  data_dir: string;
  config_path: string;
  ffmpeg_available: boolean;
  ffmpeg_version: string | null;
  ffprobe_available: boolean;
  has_workspaces: boolean;
}
