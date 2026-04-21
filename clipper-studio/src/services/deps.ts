import { invoke } from "@tauri-apps/api/core";
import type { DependencyStatus } from "@/types/deps";

/** List all managed dependencies and their status */
export async function listDeps(): Promise<DependencyStatus[]> {
  return invoke<DependencyStatus[]>("list_deps");
}

/** Check a single dependency status (force re-detect) */
export async function checkDep(depId: string): Promise<DependencyStatus> {
  return invoke<DependencyStatus>("check_dep", { depId });
}

/** Install a dependency (download + extract + verify) */
export async function installDep(depId: string): Promise<void> {
  return invoke<void>("install_dep", { depId });
}

/** Uninstall a dependency */
export async function uninstallDep(depId: string): Promise<void> {
  return invoke<void>("uninstall_dep", { depId });
}

/** Set a custom path for a dependency (writes to config.toml) */
export async function setDepCustomPath(
  depId: string,
  path: string
): Promise<void> {
  return invoke<void>("set_dep_custom_path", { depId, path });
}

/** Open the dependency installation directory in file manager */
export async function revealDepDir(depId: string): Promise<void> {
  return invoke<void>("reveal_dep_dir", { depId });
}

/** Set HTTP proxy for dependency downloads */
export async function setDepsProxy(proxyUrl: string): Promise<void> {
  return invoke<void>("set_deps_proxy", { proxyUrl });
}

/** Get current proxy URL */
export async function getDepsProxy(): Promise<string> {
  return invoke<string>("get_deps_proxy");
}
