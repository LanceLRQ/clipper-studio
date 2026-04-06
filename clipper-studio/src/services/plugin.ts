import { invoke } from "@tauri-apps/api/core";

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  plugin_type: string;
  transport: string;
  managed: boolean;
  status: string;
  description: string | null;
  has_config: boolean;
}

export async function scanPlugins(): Promise<PluginInfo[]> {
  return invoke<PluginInfo[]>("scan_plugins");
}

export async function listPlugins(): Promise<PluginInfo[]> {
  return invoke<PluginInfo[]>("list_plugins");
}

export async function loadPlugin(pluginId: string): Promise<void> {
  return invoke("load_plugin", { pluginId });
}

export async function unloadPlugin(pluginId: string): Promise<void> {
  return invoke("unload_plugin", { pluginId });
}

export async function startPluginService(pluginId: string): Promise<void> {
  return invoke("start_plugin_service", { pluginId });
}

export async function stopPluginService(pluginId: string): Promise<void> {
  return invoke("stop_plugin_service", { pluginId });
}
