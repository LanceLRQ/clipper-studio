/** Dependency installation status */
export type DepStatus =
  | "not_installed"
  | "downloading"
  | "installing"
  | "installed"
  | "error";

/** Dependency type */
export type DepType = "binary" | "runtime";

/** Full dependency status returned from backend */
export interface DependencyStatus {
  id: string;
  name: string;
  description: string;
  required: boolean;
  dep_type: DepType;
  status: DepStatus;
  version: string | null;
  installed_path: string | null;
  custom_path: string | null;
  error_message: string | null;
  auto_install_available: boolean;
  /** Manual download URL (fallback for users who can't access auto-download sources) */
  manual_download_url: string | null;
  /** Whether already found via config.toml / bin dir / system PATH */
  system_available: boolean;
  /** Path where found in system */
  system_path: string | null;
  /** Version detected from system installation */
  system_version: string | null;
}

/** Install progress event payload */
export interface InstallProgress {
  dep_id: string;
  phase: "downloading" | "extracting" | "verifying";
  progress: number;
  message: string;
}

/** Install complete event payload */
export interface InstallComplete {
  dep_id: string;
  version: string | null;
}

/** Install error event payload */
export interface InstallError {
  dep_id: string;
  error: string;
}
