import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getSettings, setSetting } from "@/services/settings";
import { checkASRHealth, type ASRHealthInfo } from "@/services/asr";
import { useThemeStore, type ThemeMode } from "@/stores/theme";
import {
  THEME_PRESETS,
  THEME_COLOR_OPTIONS,
  THEME_ACCENT_PRESETS,
  THEME_ACCENT_OPTIONS,
  type ThemeColor,
  type ThemeAccent,
} from "@/lib/theme-presets";
import {
  getAppInfo,
  listWorkspaces,
  updateWorkspace,
} from "@/services/workspace";
import type { WorkspaceInfo } from "@/types/workspace";
import { useWorkspaceStore } from "@/stores/workspace";
import {
  listPlugins,
  type PluginInfo,
  type PluginConfigField,
  getPluginConfig,
  setPluginConfig,
} from "@/services/plugin";

type ASRMode = "local" | "remote" | "disabled";

const ASR_LANGUAGES = [
  "Chinese",
  "English",
  "Cantonese",
  "Japanese",
  "Korean",
  "Arabic",
  "German",
  "French",
  "Spanish",
  "Portuguese",
  "Indonesian",
  "Italian",
  "Russian",
  "Thai",
  "Vietnamese",
  "Turkish",
  "Hindi",
  "Malay",
  "Dutch",
  "Swedish",
  "Danish",
  "Finnish",
  "Polish",
  "Czech",
  "Filipino",
  "Persian",
  "Greek",
  "Romanian",
  "Hungarian",
  "Macedonian",
] as const;

// ===== Workspace Settings Tab =====
function WorkspaceSettingsTab({ workspace }: { workspace: WorkspaceInfo }) {
  const [name, setName] = useState(workspace.name);
  const [autoScan, setAutoScan] = useState(workspace.auto_scan);
  const [clipOutputDir, setClipOutputDir] = useState(
    workspace.clip_output_dir ?? ""
  );
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setName(workspace.name);
    setAutoScan(workspace.auto_scan);
    setClipOutputDir(workspace.clip_output_dir ?? "");
  }, [workspace]);

  const handlePickDir = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      setClipOutputDir(selected as string);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    setSaved(false);
    try {
      await updateWorkspace({
        workspace_id: workspace.id,
        name: name.trim(),
        auto_scan: autoScan,
        clip_output_dir: clipOutputDir.trim(),
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      alert("保存失败: " + String(e));
    } finally {
      setSaving(false);
    }
  };

  const ADAPTER_LABELS: Record<string, string> = {
    "bililive-recorder": "录播姬 (BililiveRecorder)",
    generic: "通用目录",
  };

  return (
    <div className="space-y-6">
      {/* Basic Info */}
      <section className="rounded-lg border p-5 space-y-4">
        <h3 className="font-medium text-lg">基本信息</h3>

        <div className="space-y-1">
          <Label className="text-sm">工作区名称</Label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="输入工作区名称"
            className="text-sm h-8"
          />
        </div>

        <div className="space-y-1">
          <Label className="text-sm">工作区路径</Label>
          <p className="text-sm font-mono text-muted-foreground">
            {workspace.path}
          </p>
        </div>

        <div className="space-y-1">
          <Label className="text-sm">适配器类型</Label>
          <p className="text-sm text-muted-foreground">
            {ADAPTER_LABELS[workspace.adapter_id] ?? workspace.adapter_id}
          </p>
        </div>

        <div className="flex items-center gap-3">
          <Label className="text-sm">自动扫描目录</Label>
          <select
            value={autoScan ? "true" : "false"}
            onChange={(e) => setAutoScan(e.target.value === "true")}
            className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm"
          >
            <option value="true">开启</option>
            <option value="false">关闭</option>
          </select>
          <span className="text-xs text-muted-foreground">
            开启后，新文件会自动被发现并导入
          </span>
        </div>
      </section>

      {/* Clip Output Directory */}
      <section className="rounded-lg border p-5 space-y-4">
        <h3 className="font-medium text-lg">切片输出设置</h3>

        <div className="space-y-1">
          <Label className="text-sm">切片视频输出目录</Label>
          <div className="flex gap-2">
            <Input
              value={clipOutputDir}
              onChange={(e) => setClipOutputDir(e.target.value)}
              placeholder="留空则使用默认位置（源文件旁 clips/ 目录）"
              className="text-sm h-8 font-mono flex-1"
            />
            <Button variant="outline" size="sm" onClick={handlePickDir}>
              浏览...
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            自定义切片视频的输出位置。留空时，切片文件会保存在源视频同目录下的
            clips/ 子文件夹中。
          </p>
        </div>
      </section>

      {/* Save */}
      <div className="flex items-center gap-3">
        <Button onClick={handleSave} disabled={saving}>
          {saving ? "保存中..." : saved ? "已保存 ✓" : "保存设置"}
        </Button>
      </div>
    </div>
  );
}

// ===== System Settings Tab =====
function SystemSettingsTab() {
  // ASR settings
  const [asrMode, setAsrMode] = useState<ASRMode>("local");
  const [asrPort, setAsrPort] = useState("8765");
  const [asrUrl, setAsrUrl] = useState("");
  const [asrApiKey, setAsrApiKey] = useState("");
  const [asrLanguage, setAsrLanguage] = useState("Chinese");
  const [asrMaxChars, setAsrMaxChars] = useState("15");
  const [asrHealth, setAsrHealth] = useState<ASRHealthInfo | null>(null);
  const [asrChecking, setAsrChecking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // Plugin directory
  const [pluginDir, setPluginDir] = useState("");
  const [pluginDirLoaded, setPluginDirLoaded] = useState(false);

  // App info
  const [appInfo, setAppInfo] = useState<{
    version: string;
    data_dir: string;
    config_path: string;
    ffmpeg_available: boolean;
    ffmpeg_version: string | null;
    ffprobe_available: boolean;
  } | null>(null);

  useEffect(() => {
    getSettings([
      "asr_mode",
      "asr_port",
      "asr_url",
      "asr_api_key",
      "asr_language",
      "asr_max_chars",
      "plugin_dir",
    ])
      .then((settings) => {
        if (settings.asr_mode) setAsrMode(settings.asr_mode as ASRMode);
        if (settings.asr_port) setAsrPort(settings.asr_port);
        if (settings.asr_url) setAsrUrl(settings.asr_url);
        if (settings.asr_api_key) setAsrApiKey(settings.asr_api_key);
        if (settings.asr_language) setAsrLanguage(settings.asr_language);
        if (settings.asr_max_chars) setAsrMaxChars(settings.asr_max_chars);
        if (settings.plugin_dir) {
          setPluginDir(settings.plugin_dir);
        }
        setPluginDirLoaded(true);
      })
      .catch(console.error);

    getAppInfo()
      .then(setAppInfo)
      .catch(console.error);
  }, []);

  useEffect(() => {
    if (appInfo && pluginDirLoaded && !pluginDir) {
      setPluginDir(`${appInfo.data_dir}/plugins`);
    }
  }, [appInfo, pluginDirLoaded, pluginDir]);

  const handleSave = async () => {
    setSaving(true);
    setSaved(false);
    try {
      await setSetting("asr_mode", asrMode);
      await setSetting("asr_port", asrPort);
      if (asrUrl) await setSetting("asr_url", asrUrl);
      if (asrApiKey) await setSetting("asr_api_key", asrApiKey);
      await setSetting("asr_language", asrLanguage);
      await setSetting("asr_max_chars", asrMaxChars);
      if (pluginDir) await setSetting("plugin_dir", pluginDir);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      alert("保存失败: " + String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleCheckHealth = async () => {
    setAsrChecking(true);
    setAsrHealth(null);
    try {
      const health = await checkASRHealth();
      setAsrHealth(health);
    } catch {
      setAsrHealth({ status: "error", device: null, model_size: null });
    } finally {
      setAsrChecking(false);
    }
  };

  const themeMode = useThemeStore((s) => s.mode);
  const setThemeMode = useThemeStore((s) => s.setMode);
  const colorScheme = useThemeStore((s) => s.colorScheme);
  const setColorScheme = useThemeStore((s) => s.setColorScheme);
  const accent = useThemeStore((s) => s.accent);
  const setAccent = useThemeStore((s) => s.setAccent);

  return (
    <div className="space-y-6">
      {/* Appearance */}
      <section className="rounded-lg border p-5 space-y-4">
        <h3 className="font-medium text-lg">外观</h3>

        <div className="flex flex-wrap items-end gap-4">
          {/* Theme Mode */}
          <div className="space-y-1">
            <Label className="text-sm">主题模式</Label>
            <div className="flex rounded-md border w-fit">
              {(
                [
                  { value: "light", label: "浅色" },
                  { value: "dark", label: "深色" },
                  { value: "system", label: "跟随系统" },
                ] as const
              ).map((opt) => (
                <button
                  key={opt.value}
                  className={`px-4 py-1.5 text-sm ${themeMode === opt.value ? "bg-accent font-medium" : ""}`}
                  onClick={() => setThemeMode(opt.value as ThemeMode)}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          </div>

          {/* Base Color */}
          <div className="space-y-1">
            <Label className="text-sm">底色</Label>
            <select
              value={colorScheme}
              onChange={(e) => setColorScheme(e.target.value as ThemeColor)}
              className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm"
            >
              {THEME_COLOR_OPTIONS.map((color) => (
                <option key={color} value={color}>
                  {THEME_PRESETS[color].label} — {THEME_PRESETS[color].description}
                </option>
              ))}
            </select>
          </div>

        </div>

        {/* Accent Color Swatches */}
        <div className="space-y-1">
          <Label className="text-sm">主题色</Label>
          <div className="flex items-center gap-1.5 flex-wrap">
            {/* Default (no accent) */}
            <button
              title="默认（跟随底色）"
              className={`h-7 w-7 rounded-full border-2 flex items-center justify-center transition-all ${
                accent === "default"
                  ? "border-foreground scale-110"
                  : "border-transparent hover:border-muted-foreground/40"
              }`}
              onClick={() => setAccent("default")}
            >
              <span className="h-5 w-5 rounded-full border-2 border-dashed border-muted-foreground/50" />
            </button>
            {THEME_ACCENT_OPTIONS.filter((a) => a !== "default").map(
              (accentOpt) => {
                const preset = THEME_ACCENT_PRESETS[accentOpt];
                return (
                  <button
                    key={accentOpt}
                    title={preset.label}
                    className={`h-7 w-7 rounded-full border-2 flex items-center justify-center transition-all ${
                      accent === accentOpt
                        ? "border-foreground scale-110"
                        : "border-transparent hover:border-muted-foreground/40"
                    }`}
                    onClick={() => setAccent(accentOpt as ThemeAccent)}
                  >
                    <span
                      className="h-5 w-5 rounded-full"
                      style={{ background: preset.preview }}
                    />
                  </button>
                );
              }
            )}
          </div>
        </div>
      </section>

      {/* ASR Settings */}
      <section className="rounded-lg border p-5 space-y-4">
        <h3 className="font-medium text-lg">ASR 语音识别</h3>

        <div className="space-y-1">
          <Label className="text-sm">识别模式</Label>
          <div className="flex rounded-md border w-fit">
            {(
              [
                { value: "local", label: "本地引擎" },
                { value: "remote", label: "远程服务" },
                { value: "disabled", label: "禁用" },
              ] as const
            ).map((opt) => (
              <button
                key={opt.value}
                className={`px-4 py-1.5 text-sm ${asrMode === opt.value ? "bg-accent font-medium" : ""}`}
                onClick={() => setAsrMode(opt.value)}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>

        <div className="space-y-1">
          <Label className="text-sm">识别语言</Label>
          <select
            value={asrLanguage}
            onChange={(e) => setAsrLanguage(e.target.value)}
            className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm w-48"
          >
            {ASR_LANGUAGES.map((lang) => (
              <option key={lang} value={lang}>
                {lang}
              </option>
            ))}
          </select>
        </div>

        <div className="space-y-1">
          <Label className="text-sm">每条字幕最大字符数</Label>
          <Input
            value={asrMaxChars}
            onChange={(e) => setAsrMaxChars(e.target.value)}
            placeholder="15"
            className="w-32 text-sm h-8"
            type="number"
            min={0}
          />
          <p className="text-xs text-muted-foreground">
            ASR 识别后按标点和字数自动拆分字幕，设为 0 则不拆分
          </p>
        </div>

        {asrMode === "local" && (
          <div className="space-y-3 pl-1">
            <div className="space-y-1">
              <Label className="text-xs">本地服务端口</Label>
              <Input
                value={asrPort}
                onChange={(e) => setAsrPort(e.target.value)}
                placeholder="8765"
                className="w-32 text-sm h-8"
              />
              <p className="text-xs text-muted-foreground">
                qwen3-asr-service 的 HTTP 端口，默认 8765
              </p>
            </div>
          </div>
        )}

        {asrMode === "remote" && (
          <div className="space-y-3 pl-1">
            <div className="space-y-1">
              <Label className="text-xs">服务地址</Label>
              <Input
                value={asrUrl}
                onChange={(e) => setAsrUrl(e.target.value)}
                placeholder="http://192.168.1.100:8765"
                className="text-sm h-8"
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">API Key（可选）</Label>
              <Input
                value={asrApiKey}
                onChange={(e) => setAsrApiKey(e.target.value)}
                placeholder="留空则不使用认证"
                type="password"
                className="text-sm h-8"
              />
            </div>
          </div>
        )}

        {asrMode === "disabled" && (
          <p className="text-sm text-muted-foreground pl-1">
            ASR 功能已禁用，视频详情页中将不会显示语音识别按钮。
          </p>
        )}

        <div className="flex items-center gap-3 pt-2">
          <Button onClick={handleSave} disabled={saving}>
            {saving ? "保存中..." : saved ? "已保存 ✓" : "保存设置"}
          </Button>
          {asrMode !== "disabled" && (
            <Button
              variant="outline"
              onClick={handleCheckHealth}
              disabled={asrChecking}
            >
              {asrChecking ? "检测中..." : "测试连接"}
            </Button>
          )}
          {asrHealth && (
            <span
              className={`text-sm ${asrHealth.status === "ready" ? "text-green-600" : "text-red-500"}`}
            >
              {asrHealth.status === "ready"
                ? `连接成功 (${asrHealth.device ?? "unknown"}${asrHealth.model_size ? ` / ${asrHealth.model_size}` : ""})`
                : "连接失败"}
            </span>
          )}
        </div>
      </section>

      {/* Plugin Directory */}
      <section className="rounded-lg border p-5 space-y-4">
        <h3 className="font-medium text-lg">插件目录</h3>
        <div className="space-y-1">
          <Label className="text-xs">自定义插件目录（绝对路径）</Label>
          <Input
            value={pluginDir}
            onChange={(e) => setPluginDir(e.target.value)}
            placeholder="留空使用默认目录"
            className="text-sm h-8 font-mono"
          />
          <p className="text-xs text-muted-foreground">
            修改后需在「插件管理」页点击「扫描插件」生效。默认：
            {appInfo ? `${appInfo.data_dir}/plugins` : "加载中..."}
          </p>
        </div>
        <div className="flex items-center gap-3 pt-2">
          <Button onClick={handleSave} disabled={saving}>
            {saving ? "保存中..." : saved ? "已保存 ✓" : "保存"}
          </Button>
        </div>
      </section>

      {/* App Info */}
      {appInfo && (
        <section className="rounded-lg border p-5 space-y-3">
          <h3 className="font-medium text-lg">应用信息</h3>
          <div className="space-y-2 text-sm">
            <InfoRow label="版本" value={`v${appInfo.version}`} />
            <InfoRow label="数据目录" value={appInfo.data_dir} />
            <InfoRow label="配置文件" value={appInfo.config_path} />
            <InfoRow
              label="FFmpeg"
              value={
                appInfo.ffmpeg_available
                  ? appInfo.ffmpeg_version ?? "可用"
                  : "不可用"
              }
            />
            <InfoRow
              label="FFprobe"
              value={appInfo.ffprobe_available ? "可用" : "不可用"}
            />
          </div>
        </section>
      )}
    </div>
  );
}

// ===== Plugin Config Tab =====
function PluginConfigTab({ plugin }: { plugin: PluginInfo }) {
  const [configs, setConfigs] = useState<{
    values: Record<string, string>;
    loaded: boolean;
  }>({ values: {}, loaded: false });
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const schema = plugin.config_schema ?? {};

  useEffect(() => {
    if (!plugin.has_config) return;
    getPluginConfig(plugin.id)
      .then((vals) => {
        const filled: Record<string, string> = {};
        for (const [k, field] of Object.entries(schema)) {
          filled[k] = vals[k] ?? String(field.default ?? "");
        }
        setConfigs({ values: filled, loaded: true });
      })
      .catch(console.error);
  }, [plugin.id, plugin.has_config, schema]);

  const handleChange = useCallback(
    (key: string, value: string) => {
      setConfigs((prev) => ({
        ...prev,
        values: { ...prev.values, [key]: value },
      }));
    },
    []
  );

  const handleSave = async () => {
    setSaving(true);
    setSaved(false);
    try {
      for (const [key, value] of Object.entries(configs.values)) {
        await setPluginConfig(plugin.id, key, value);
      }
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      alert(`保存失败: ${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  if (!configs.loaded) {
    return <div className="text-muted-foreground text-sm p-4">加载配置中...</div>;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-muted-foreground">
          {plugin.description ?? "插件配置"}
        </p>
        <span
          className={`text-xs px-2 py-0.5 rounded ${
            plugin.status === "loaded" || plugin.status === "running"
              ? "bg-green-100 text-green-700"
              : "bg-gray-100 text-gray-500"
          }`}
        >
          {plugin.status === "loaded"
            ? "已加载"
            : plugin.status === "running"
              ? "运行中"
              : plugin.status === "discovered"
                ? "未加载"
                : plugin.status}
        </span>
      </div>

      <div className="space-y-3 text-sm">
        {Object.entries(schema)
          .sort(([, a], [, b]) => (a.order ?? 0) - (b.order ?? 0))
          .map(([key, field]) => (
          <ConfigField
            key={key}
            fieldKey={key}
            field={field}
            value={configs.values[key] ?? String(field.default ?? "")}
            onChange={handleChange}
          />
        ))}
      </div>

      <div className="flex items-center gap-3 pt-2">
        <Button onClick={handleSave} disabled={saving}>
          {saving ? "保存中..." : saved ? "已保存 ✓" : "保存配置"}
        </Button>
      </div>

      {/* Plugin UI mount point (for plugins with custom frontend) */}
      {plugin.frontend?.entry && plugin.dir && (
        <PluginUIMount plugin={plugin} />
      )}
    </div>
  );
}

// ===== Plugin UI Dynamic Loading =====
interface PluginModule {
  registerSettings: (
    container: HTMLElement,
    ctx: { pluginId: string; pluginDir: string }
  ) => void;
}

function PluginUIMount({ plugin }: { plugin: PluginInfo }) {
  const containerRef = useCallback(
    (node: HTMLDivElement | null) => {
      if (!node || !plugin.frontend?.entry || !plugin.dir) return;
      const entryPath = `${plugin.dir}/${plugin.frontend.entry}`;
      import(/* @vite-ignore */ entryPath)
        .then((mod: PluginModule) => {
          if (typeof mod.registerSettings === "function") {
            mod.registerSettings(node, {
              pluginId: plugin.id,
              pluginDir: plugin.dir!,
            });
          }
        })
        .catch((e) => console.error(`Failed to load plugin UI for ${plugin.id}:`, e));
    },
    [plugin]
  );

  return (
    <div className="mt-4 border-t pt-4">
      <h4 className="text-sm font-medium mb-2">插件界面</h4>
      <div ref={containerRef} className="plugin-ui-container min-h-[100px]" />
    </div>
  );
}

// ===== Config Field =====
function ConfigField({
  fieldKey,
  field,
  value,
  onChange,
}: {
  fieldKey: string;
  field: PluginConfigField;
  value: string;
  onChange: (key: string, value: string) => void;
}) {
  const displayName = field.label || fieldKey;

  if (field.type === "boolean") {
    return (
      <div className="space-y-1">
        <div className="flex items-center gap-3">
          <select
            value={value === "true" ? "true" : "false"}
            onChange={(e) => onChange(fieldKey, e.target.value)}
            className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm"
          >
            <option value="true">是</option>
            <option value="false">否</option>
          </select>
          <Label className="text-sm cursor-pointer">{displayName}</Label>
        </div>
        {field.description && (
          <p className="text-xs text-muted-foreground pl-2">
            {field.description}
          </p>
        )}
      </div>
    );
  }

  const isPassword =
    fieldKey.toLowerCase().includes("pass") ||
    fieldKey.toLowerCase().includes("secret") ||
    fieldKey.toLowerCase().includes("key");

  return (
    <div className="space-y-1">
      <Label className="text-xs">{displayName}</Label>
      <Input
        value={value}
        onChange={(e) => onChange(fieldKey, e.target.value)}
        placeholder={String(field.default ?? "")}
        type={isPassword ? "password" : "text"}
        className="text-sm h-8"
      />
      {field.description && (
        <p className="text-xs text-muted-foreground">{field.description}</p>
      )}
    </div>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right text-xs font-mono break-all min-w-0 shrink">
        {value}
      </span>
    </div>
  );
}

// ===== Main Settings Page =====
function SettingsPage() {
  const [configPlugins, setConfigPlugins] = useState<PluginInfo[]>([]);
  const [activeWorkspace, setActiveWorkspace] = useState<WorkspaceInfo | null>(
    null
  );
  const [loading, setLoading] = useState(true);
  const wsActiveId = useWorkspaceStore((s) => s.activeId);
  const wsVersion = useWorkspaceStore((s) => s.version);

  useEffect(() => {
    setLoading(true);

    // Load plugins with config
    listPlugins()
      .then((list) => {
        setConfigPlugins(list.filter((p) => p.has_config));
      })
      .catch(console.error)
      .finally(() => setLoading(false));

    // Load active workspace info
    if (wsActiveId != null) {
      listWorkspaces()
        .then((all) => {
          const ws = all.find((w) => w.id === wsActiveId) ?? null;
          setActiveWorkspace(ws);
        })
        .catch(console.error);
    }
  }, [wsActiveId, wsVersion]);

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-semibold">设置</h2>

      <Tabs defaultValue="workspace">
        <TabsList>
          <TabsTrigger value="workspace">工作区设置</TabsTrigger>
          <TabsTrigger value="system">系统设置</TabsTrigger>
          {configPlugins.map((plugin) => (
            <TabsTrigger key={plugin.id} value={plugin.id}>
              {plugin.name}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="workspace" className="mt-4">
          {activeWorkspace ? (
            <WorkspaceSettingsTab workspace={activeWorkspace} />
          ) : (
            <div className="text-muted-foreground text-sm">加载工作区信息中...</div>
          )}
        </TabsContent>

        <TabsContent value="system" className="mt-4">
          <SystemSettingsTab />
        </TabsContent>

        {configPlugins.map((plugin) => (
          <TabsContent key={plugin.id} value={plugin.id} className="mt-4">
            <PluginConfigTab plugin={plugin} />
          </TabsContent>
        ))}
      </Tabs>

      {loading && configPlugins.length === 0 && (
        <div className="text-muted-foreground text-sm">加载中...</div>
      )}
    </div>
  );
}

export const Route = createFileRoute("/dashboard/settings")({
  component: SettingsPage,
});
