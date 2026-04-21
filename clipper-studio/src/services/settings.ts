import { invoke } from "@tauri-apps/api/core";

export async function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>("get_setting", { key });
}

export async function setSetting(key: string, value: string): Promise<void> {
  return invoke("set_setting", { key, value });
}

export async function getSettings(
  keys: string[]
): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_settings", { keys });
}
