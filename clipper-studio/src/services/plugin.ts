import { invoke } from "@tauri-apps/api/core";

export interface PluginConfigField {
  type: "string" | "boolean";
  default: string | boolean;
  description: string;
}

export interface PluginFrontend {
  entry: string;
  target: string;
}

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
  /** Configuration schema (field name -> schema). Only present if has_config is true. */
  config_schema?: Record<string, PluginConfigField>;
  /** Frontend entry for plugin UI. If present, plugin provides a custom React component. */
  frontend?: PluginFrontend;
  /** Plugin directory path (for external plugins to resolve frontend entry). */
  dir?: string;
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

export async function callPlugin(
  pluginId: string,
  action: string,
  payload: Record<string, unknown> = {}
): Promise<unknown> {
  return invoke("call_plugin", { pluginId, action, payload });
}

/** Get all saved config values for a plugin */
export async function getPluginConfig(
  pluginId: string
): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_plugin_config", { pluginId });
}

/** Set a single config value for a plugin */
export async function setPluginConfig(
  pluginId: string,
  key: string,
  value: string
): Promise<void> {
  return invoke("set_plugin_config", { pluginId, key, value });
}

export interface RecorderRoom {
  roomId: number;
  shortId: number;
  name: string;
  title: string;
  areaNameParent: string;
  areaNameChild: string;
  recording: boolean;
  streaming: boolean;
  danmakuConnected: boolean;
  autoRecord: boolean;
}

export interface RecorderStatus {
  connected: boolean;
  version: string | null;
  rooms: RecorderRoom[];
}
