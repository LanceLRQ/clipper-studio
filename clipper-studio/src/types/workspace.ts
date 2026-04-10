export interface WorkspaceInfo {
  id: number;
  name: string;
  path: string;
  adapter_id: string;
  adapter_config: string | null;
  auto_scan: boolean;
  clip_output_dir: string | null;
  created_at: string;
}

/** SMB mount origin info stored in adapter_config */
export interface SmbMountConfig {
  source: "smb";
  server: string;
  share: string;
  mount_point: string;
}

export interface CreateWorkspaceRequest {
  name: string;
  path: string;
  adapter_id: string;
  adapter_config?: string;
}

export interface UpdateWorkspaceRequest {
  workspace_id: number;
  name?: string;
  auto_scan?: boolean;
  clip_output_dir?: string;
}

export interface AppInfo {
  version: string;
  data_dir: string;
  config_path: string;
  ffmpeg_available: boolean;
  ffmpeg_version: string | null;
  ffprobe_available: boolean;
  has_workspaces: boolean;
  media_server_port: number;
}
