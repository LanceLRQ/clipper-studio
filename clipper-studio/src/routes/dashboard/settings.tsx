import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState, useCallback, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ask } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getSettings, setSetting } from "@/services/settings";
import {
  listDeps,
  installDep,
  uninstallDep,
  revealDepDir,
  setDepCustomPath,
} from "@/services/deps";
import type {
  DependencyStatus,
  InstallProgress,
  DepStatus,
} from "@/types/deps";
import {
  checkASRHealth,
  validateASRPath,
  startASRService,
  stopASRService,
  getASRServiceStatus,
  getASRServiceLogs,
  type ASRHealthInfo,
  type ASRPathValidation,
  type ASRServiceStatusInfo,
} from "@/services/asr";
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
  deleteWorkspace,
} from "@/services/workspace";
import { TagManager } from "@/components/tag/tag-manager";
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

// ===== Workspace List Section =====
function WorkspaceListSection({
  allWorkspaces,
  onReload,
}: {
  allWorkspaces: WorkspaceInfo[];
  onReload: () => void;
}) {
  const navigate = useNavigate();
  const activeId = useWorkspaceStore((s) => s.activeId);
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);

  const handleDelete = async (ws: WorkspaceInfo) => {
    if (
      !(await ask(
        `确定要删除工作区「${ws.name}」吗？\n\n注意：仅删除工作区记录，不会删除磁盘上的文件。`,
        { title: "删除工作区", kind: "warning" }
      ))
    )
      return;
    try {
      await deleteWorkspace(ws.id);
      const remaining = await listWorkspaces();
      if (ws.id === activeId) {
        if (remaining.length > 0) {
          await switchWorkspace(remaining[0].id);
        } else {
          navigate({ to: "/welcome", replace: true });
          return;
        }
      }
      onReload();
    } catch (e) {
      console.error("Failed to delete workspace:", e);
    }
  };

  return (
    <section className="rounded-lg border p-5 space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-medium text-lg">所有工作区</h3>
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigate({ to: "/welcome" })}
        >
          + 添加工作区
        </Button>
      </div>
      <div className="space-y-2">
        {allWorkspaces.map((ws) => (
          <div
            key={ws.id}
            className={`flex items-center justify-between rounded-md border p-3 ${ws.id === activeId ? "border-primary bg-accent/30" : ""}`}
          >
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="font-medium text-sm">{ws.name}</span>
                {ws.id === activeId && (
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-primary text-primary-foreground">
                    当前
                  </span>
                )}
                {(() => {
                  try {
                    if (ws.adapter_config) {
                      const cfg = JSON.parse(ws.adapter_config);
                      if (cfg.source === "smb") {
                        return (
                          <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300">
                            SMB
                          </span>
                        );
                      }
                    }
                  } catch { /* ignore */ }
                  return null;
                })()}
              </div>
              <div className="text-xs text-muted-foreground truncate">
                {ws.path}
              </div>
            </div>
            <div className="flex gap-1.5 shrink-0 ml-3">
              {ws.id !== activeId && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => switchWorkspace(ws.id)}
                >
                  切换
                </Button>
              )}
              <Button
                variant="ghost"
                size="sm"
                className="text-red-500 hover:text-red-600"
                onClick={() => handleDelete(ws)}
              >
                删除
              </Button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

// ===== ASR Settings Tab =====
function ASRSettingsTab() {
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

  // Local service management
  const [asrLocalPath, setAsrLocalPath] = useState("");
  const [asrLocalDevice, setAsrLocalDevice] = useState("auto");
  const [asrLocalModelSize, setAsrLocalModelSize] = useState("auto");
  const [asrLocalEnableAlign, setAsrLocalEnableAlign] = useState("true");
  const [asrLocalEnablePunc, setAsrLocalEnablePunc] = useState("true");
  const [asrLocalModelSource, setAsrLocalModelSource] = useState("modelscope");
  const [asrLocalMaxSegment, setAsrLocalMaxSegment] = useState("5");
  const [asrLocalAutoStart, setAsrLocalAutoStart] = useState("false");
  const [pathValidation, setPathValidation] = useState<ASRPathValidation | null>(null);
  const [validating, setValidating] = useState(false);
  const [serviceStatus, setServiceStatus] = useState<ASRServiceStatusInfo | null>(null);
  const [serviceLogs, setServiceLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [serviceActionLoading, setServiceActionLoading] = useState(false);

  useEffect(() => {
    getSettings([
      "asr_mode", "asr_port", "asr_url", "asr_api_key", "asr_language", "asr_max_chars",
      "asr_local_path", "asr_local_device", "asr_local_model_size",
      "asr_local_enable_align", "asr_local_enable_punc", "asr_local_model_source",
      "asr_local_max_segment", "asr_local_auto_start",
    ])
      .then((s) => {
        if (s.asr_mode) setAsrMode(s.asr_mode as ASRMode);
        if (s.asr_port) setAsrPort(s.asr_port);
        if (s.asr_url) setAsrUrl(s.asr_url);
        if (s.asr_api_key) setAsrApiKey(s.asr_api_key);
        if (s.asr_language) setAsrLanguage(s.asr_language);
        if (s.asr_max_chars) setAsrMaxChars(s.asr_max_chars);
        if (s.asr_local_path) setAsrLocalPath(s.asr_local_path);
        if (s.asr_local_device) setAsrLocalDevice(s.asr_local_device);
        if (s.asr_local_model_size) setAsrLocalModelSize(s.asr_local_model_size);
        if (s.asr_local_enable_align) setAsrLocalEnableAlign(s.asr_local_enable_align);
        if (s.asr_local_enable_punc) setAsrLocalEnablePunc(s.asr_local_enable_punc);
        if (s.asr_local_model_source) setAsrLocalModelSource(s.asr_local_model_source);
        if (s.asr_local_max_segment) setAsrLocalMaxSegment(s.asr_local_max_segment);
        if (s.asr_local_auto_start) setAsrLocalAutoStart(s.asr_local_auto_start);
        if (s.asr_local_path) {
          validateASRPath(s.asr_local_path).then(setPathValidation).catch(console.error);
        }
      })
      .catch(console.error);

    getASRServiceStatus().then(setServiceStatus).catch(console.error);
  }, []);

  // Listen for real-time service status events
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<ASRServiceStatusInfo>("asr-service-status", (event) => {
      setServiceStatus(event.payload);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handlePathChange = useCallback(async (newPath: string) => {
    setAsrLocalPath(newPath);
    if (!newPath.trim()) { setPathValidation(null); return; }
    setValidating(true);
    try { setPathValidation(await validateASRPath(newPath)); }
    catch { setPathValidation(null); }
    finally { setValidating(false); }
  }, []);

  const handlePickAsrDir = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) handlePathChange(selected as string);
  };

  const handleSave = async () => {
    setSaving(true); setSaved(false);
    try {
      await setSetting("asr_mode", asrMode);
      await setSetting("asr_port", asrPort);
      if (asrUrl) await setSetting("asr_url", asrUrl);
      if (asrApiKey) await setSetting("asr_api_key", asrApiKey);
      await setSetting("asr_language", asrLanguage);
      await setSetting("asr_max_chars", asrMaxChars);
      await setSetting("asr_local_path", asrLocalPath);
      await setSetting("asr_local_device", asrLocalDevice);
      await setSetting("asr_local_model_size", asrLocalModelSize);
      await setSetting("asr_local_enable_align", asrLocalEnableAlign);
      await setSetting("asr_local_enable_punc", asrLocalEnablePunc);
      await setSetting("asr_local_model_source", asrLocalModelSource);
      await setSetting("asr_local_max_segment", asrLocalMaxSegment);
      await setSetting("asr_local_auto_start", asrLocalAutoStart);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) { alert("保存失败: " + String(e)); }
    finally { setSaving(false); }
  };

  const handleCheckHealth = async () => {
    setAsrChecking(true); setAsrHealth(null);
    try { setAsrHealth(await checkASRHealth()); }
    catch { setAsrHealth({ status: "error", device: null, model_size: null }); }
    finally { setAsrChecking(false); }
  };

  const handleStartService = async () => {
    setServiceActionLoading(true);
    try { await handleSave(); await startASRService(); }
    catch (e) { alert("启动失败: " + String(e)); }
    finally { setServiceActionLoading(false); }
  };

  const handleStopService = async () => {
    setServiceActionLoading(true);
    try { await stopASRService(); }
    catch (e) { alert("停止失败: " + String(e)); }
    finally { setServiceActionLoading(false); }
  };

  const handleViewLogs = async () => {
    if (!showLogs) {
      try { setServiceLogs(await getASRServiceLogs(200)); }
      catch (e) { console.error("Failed to load logs:", e); }
    }
    setShowLogs(!showLogs);
  };

  return (
    <div className="space-y-6">
      {/* Basic ASR Settings */}
      <section className="rounded-lg border p-5 space-y-4">
        <h3 className="font-medium text-lg">基本设置</h3>

        <div className="space-y-1">
          <Label className="text-sm">识别模式</Label>
          <div className="flex rounded-md border w-fit">
            {([
              { value: "local", label: "本地引擎" },
              { value: "remote", label: "远程服务" },
              { value: "disabled", label: "禁用" },
            ] as const).map((opt) => (
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
              <option key={lang} value={lang}>{lang}</option>
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

        {asrMode === "remote" && (
          <div className="space-y-3 pl-1">
            <div className="space-y-1">
              <Label className="text-xs">服务地址</Label>
              <Input value={asrUrl} onChange={(e) => setAsrUrl(e.target.value)}
                placeholder="http://192.168.1.100:8765" className="text-sm h-8" />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">API Key（可选）</Label>
              <Input value={asrApiKey} onChange={(e) => setAsrApiKey(e.target.value)}
                placeholder="留空则不使用认证" type="password" className="text-sm h-8" />
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
            <Button variant="outline" onClick={handleCheckHealth} disabled={asrChecking}>
              {asrChecking ? "检测中..." : "测试连接"}
            </Button>
          )}
          {asrHealth && (
            <span className={`text-sm ${asrHealth.status === "ready" ? "text-green-600" : "text-red-500"}`}>
              {asrHealth.status === "ready"
                ? `连接成功 (${asrHealth.device ?? "unknown"}${asrHealth.model_size ? ` / ${asrHealth.model_size}` : ""})`
                : "连接失败"}
            </span>
          )}
        </div>
      </section>

      {/* Local Service Management (only when mode=local) */}
      {asrMode === "local" && (
        <>
          <section className="rounded-lg border p-5 space-y-4">
            <h3 className="font-medium text-lg">本地服务配置</h3>

            {/* Service Path */}
            <div className="space-y-1">
              <Label className="text-sm">服务目录路径</Label>
              <div className="flex gap-2">
                <Input
                  value={asrLocalPath}
                  onChange={(e) => handlePathChange(e.target.value)}
                  placeholder="选择 qwen3-asr-service 所在目录"
                  className="text-sm h-8 font-mono flex-1"
                />
                <Button variant="outline" size="sm" onClick={handlePickAsrDir}>
                  浏览...
                </Button>
              </div>
              {validating && (
                <p className="text-xs text-muted-foreground">校验中...</p>
              )}
              {pathValidation && !validating && (
                <p className={`text-xs ${pathValidation.valid ? "text-green-600" : "text-red-500"}`}>
                  {pathValidation.valid
                    ? `路径有效 (${pathValidation.python_path})`
                    : !pathValidation.has_venv
                      ? "未找到 Python 虚拟环境（asr-service/venv）"
                      : "未找到入口文件（asr-service/app/main.py）"}
                </p>
              )}
            </div>

            {/* Port */}
            <div className="space-y-1">
              <Label className="text-sm">本地服务端口</Label>
              <Input value={asrPort} onChange={(e) => setAsrPort(e.target.value)}
                placeholder="8765" className="w-32 text-sm h-8" />
            </div>

            {/* Startup Parameters */}
            <div className="space-y-3 pt-2">
              <h4 className="text-sm font-medium">启动参数</h4>
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1">
                  <Label className="text-xs">运行设备</Label>
                  <select value={asrLocalDevice} onChange={(e) => setAsrLocalDevice(e.target.value)}
                    className="h-8 w-full rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="auto">自动检测</option>
                    <option value="cuda">CUDA (GPU)</option>
                    <option value="cpu">CPU</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">模型大小</Label>
                  <select value={asrLocalModelSize} onChange={(e) => setAsrLocalModelSize(e.target.value)}
                    className="h-8 w-full rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="auto">自动选择</option>
                    <option value="0.6b">0.6B (轻量，2-3GB VRAM)</option>
                    <option value="1.7b">1.7B (完整，6-8GB VRAM)</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">模型来源</Label>
                  <select value={asrLocalModelSource} onChange={(e) => setAsrLocalModelSource(e.target.value)}
                    className="h-8 w-full rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="modelscope">ModelScope (国内推荐)</option>
                    <option value="huggingface">HuggingFace</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">VAD 合并阈值 (秒)</Label>
                  <Input value={asrLocalMaxSegment} onChange={(e) => setAsrLocalMaxSegment(e.target.value)}
                    placeholder="5" className="text-sm h-8" type="number" min={1} max={30} />
                </div>
              </div>

              <div className="grid grid-cols-3 gap-3">
                <div className="flex items-center gap-2">
                  <select value={asrLocalEnableAlign} onChange={(e) => setAsrLocalEnableAlign(e.target.value)}
                    className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="true">是</option>
                    <option value="false">否</option>
                  </select>
                  <Label className="text-xs">启用字级对齐</Label>
                </div>
                <div className="flex items-center gap-2">
                  <select value={asrLocalEnablePunc} onChange={(e) => setAsrLocalEnablePunc(e.target.value)}
                    className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="true">是</option>
                    <option value="false">否</option>
                  </select>
                  <Label className="text-xs">启用标点恢复</Label>
                </div>
                <div className="flex items-center gap-2">
                  <select value={asrLocalAutoStart} onChange={(e) => setAsrLocalAutoStart(e.target.value)}
                    className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="false">否</option>
                    <option value="true">是</option>
                  </select>
                  <Label className="text-xs">随应用启动</Label>
                </div>
              </div>
            </div>

            <div className="flex items-center gap-3 pt-2">
              <Button onClick={handleSave} disabled={saving}>
                {saving ? "保存中..." : saved ? "已保存 ✓" : "保存设置"}
              </Button>
            </div>
          </section>

          {/* Service Control */}
          <section className="rounded-lg border p-5 space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="font-medium text-lg">服务控制</h3>
              <div className="flex items-center gap-2">
                <span className={`inline-block h-2.5 w-2.5 rounded-full ${
                  serviceStatus?.status === "running" ? "bg-green-500"
                    : serviceStatus?.status === "starting" ? "bg-yellow-500 animate-pulse"
                    : serviceStatus?.status === "error" ? "bg-red-500"
                    : "bg-gray-400"
                }`} />
                <span className="text-sm text-muted-foreground">
                  {serviceStatus?.status === "running" ? "运行中"
                    : serviceStatus?.status === "starting" ? "启动中..."
                    : serviceStatus?.status === "stopping" ? "停止中..."
                    : serviceStatus?.status === "error" ? "错误"
                    : "已停止"}
                </span>
              </div>
            </div>

            {/* Health info when running */}
            {serviceStatus?.status === "running" && serviceStatus.health_info && (
              <div className="text-sm text-muted-foreground bg-accent/30 rounded-md px-4 py-3 space-y-1">
                <div>设备: <span className="font-medium text-foreground">{serviceStatus.health_info.device ?? "unknown"}</span></div>
                <div>模型: <span className="font-medium text-foreground">{serviceStatus.health_info.model_size ?? "unknown"}</span></div>
              </div>
            )}

            {/* Error message */}
            {serviceStatus?.status === "error" && (serviceStatus as { message?: string }).message && (
              <p className="text-sm text-red-500">
                {(serviceStatus as { message?: string }).message}
              </p>
            )}

            {/* Action buttons */}
            <div className="flex items-center gap-3">
              {serviceStatus?.status === "running" || serviceStatus?.status === "starting" ? (
                <Button
                  variant="destructive"
                  onClick={handleStopService}
                  disabled={serviceActionLoading || serviceStatus?.status === "stopping"}
                >
                  {serviceStatus?.status === "stopping" ? "停止中..." : "停止服务"}
                </Button>
              ) : (
                <Button
                  onClick={handleStartService}
                  disabled={serviceActionLoading || !pathValidation?.valid}
                >
                  {serviceActionLoading ? "操作中..." : "启动服务"}
                </Button>
              )}

              <Button variant="outline" onClick={handleViewLogs}>
                {showLogs ? "隐藏日志" : "查看日志"}
              </Button>
            </div>

            {/* Logs panel */}
            {showLogs && (
              <div className="rounded-md border bg-muted/30 p-3 max-h-72 overflow-auto">
                <pre className="text-[11px] font-mono whitespace-pre-wrap leading-relaxed">
                  {serviceLogs.length > 0 ? serviceLogs.join("\n") : "暂无日志"}
                </pre>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}

// ===== Dependency Manager Tab =====
function DependencyManagerTab() {
  const [deps, setDeps] = useState<DependencyStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [customPathEditing, setCustomPathEditing] = useState<string | null>(
    null
  );
  const [customPathValue, setCustomPathValue] = useState("");

  const loadDeps = useCallback(async () => {
    try {
      setDeps(await listDeps());
    } catch (e) {
      console.error("Failed to load deps:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadDeps();
  }, [loadDeps]);

  // Listen for install progress events
  useEffect(() => {
    let unlistenProgress: UnlistenFn | undefined;
    let unlistenComplete: UnlistenFn | undefined;
    let unlistenError: UnlistenFn | undefined;

    listen<InstallProgress>("dep:install-progress", (event) => {
      setProgress(event.payload);
    }).then((fn) => {
      unlistenProgress = fn;
    });

    listen<{ dep_id: string; version: string | null; needs_restart: boolean }>(
      "dep:install-complete",
      async (event) => {
        setInstallingId(null);
        setProgress(null);
        loadDeps();
        if (event.payload.needs_restart) {
          await ask(
            "安装成功！需要重启应用才能使新安装的依赖生效。\n\n请手动关闭并重新打开 ClipperStudio。",
            { title: "安装完成", kind: "info" }
          );
        }
      }
    ).then((fn) => {
      unlistenComplete = fn;
    });

    listen<{ dep_id: string; error: string }>(
      "dep:install-error",
      (event) => {
        setInstallingId(null);
        setProgress(null);
        loadDeps();
      }
    ).then((fn) => {
      unlistenError = fn;
    });

    return () => {
      unlistenProgress?.();
      unlistenComplete?.();
      unlistenError?.();
    };
  }, [loadDeps]);

  const handleInstall = async (depId: string) => {
    setInstallingId(depId);
    setProgress(null);
    try {
      await installDep(depId);
    } catch (e) {
      alert("安装失败: " + String(e));
      setInstallingId(null);
      setProgress(null);
      loadDeps();
    }
  };

  const handleUninstall = async (dep: DependencyStatus) => {
    if (
      !(await ask(
        `确定要卸载「${dep.name}」吗？\n\n将删除已下载的文件。`,
        { title: "卸载依赖", kind: "warning" }
      ))
    )
      return;
    try {
      await uninstallDep(dep.id);
      loadDeps();
    } catch (e) {
      alert("卸载失败: " + String(e));
    }
  };

  const handleReveal = async (depId: string) => {
    try {
      await revealDepDir(depId);
    } catch (e) {
      alert(String(e));
    }
  };

  const handleCustomPathSave = async (depId: string) => {
    try {
      await setDepCustomPath(depId, customPathValue);
      setCustomPathEditing(null);
      setCustomPathValue("");
      loadDeps();
    } catch (e) {
      alert("设置路径失败: " + String(e));
    }
  };

  const handlePickCustomPath = async (depId: string) => {
    const selected = await open({
      directory: false,
      multiple: false,
      title: "选择可执行文件",
    });
    if (selected) {
      setCustomPathValue(selected as string);
    }
  };

  const statusLabel = (dep: DependencyStatus): { text: string; className: string } => {
    switch (dep.status) {
      case "installed":
        return { text: "已安装（依赖管理器）", className: "text-green-600" };
      case "downloading":
        return { text: "下载中", className: "text-blue-500" };
      case "installing":
        return { text: "安装中", className: "text-blue-500" };
      case "error":
        return { text: "错误", className: "text-red-500" };
      default:
        // Not installed via deps manager, but maybe found in system
        if (dep.system_available) {
          return { text: "系统已安装", className: "text-green-600" };
        }
        return { text: "未安装", className: "text-muted-foreground" };
    }
  };

  if (loading) {
    return (
      <div className="text-muted-foreground text-sm p-4">加载依赖信息中...</div>
    );
  }

  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        以下工具是 ClipperStudio 运行所需的第三方依赖。点击"安装"自动下载，或手动指定已有安装路径。
      </p>

      {deps.map((dep) => {
        const isInstalling = installingId === dep.id;
        const currentProgress =
          isInstalling && progress && progress.dep_id === dep.id
            ? progress
            : null;
        const sl = statusLabel(dep);

        return (
          <section key={dep.id} className="rounded-lg border p-5 space-y-3">
            {/* Header */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <h3 className="font-medium">{dep.name}</h3>
                {dep.required && (
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-primary/10 text-primary font-medium">
                    核心
                  </span>
                )}
                {!dep.required && (
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground">
                    可选
                  </span>
                )}
                {(dep.version || dep.system_version) && (
                  <span className="text-xs text-muted-foreground font-mono">
                    v{dep.version || dep.system_version}
                  </span>
                )}
              </div>
              <span className={`text-sm font-medium ${sl.className}`}>
                {sl.text}
              </span>
            </div>

            {/* Description */}
            <p className="text-sm text-muted-foreground">{dep.description}</p>

            {/* Install path (deps manager) */}
            {dep.status === "installed" && dep.installed_path && (
              <p className="text-xs text-muted-foreground font-mono truncate">
                路径: {dep.installed_path}
              </p>
            )}

            {/* System path (when found via PATH/config but not in deps manager) */}
            {dep.status !== "installed" && dep.system_available && dep.system_path && (
              <p className="text-xs text-green-600 font-mono truncate">
                系统路径: {dep.system_path}
              </p>
            )}

            {/* Custom path override */}
            {dep.custom_path && (
              <p className="text-xs text-blue-600 font-mono truncate">
                自定义路径: {dep.custom_path}
              </p>
            )}

            {/* Error message */}
            {dep.status === "error" && dep.error_message && (
              <p className="text-sm text-red-500">{dep.error_message}</p>
            )}

            {/* Progress bar */}
            {currentProgress && (
              <div className="space-y-1.5">
                <div className="flex justify-between text-xs text-muted-foreground">
                  <span>
                    {currentProgress.phase === "downloading"
                      ? "下载中"
                      : currentProgress.phase === "extracting"
                        ? "解压中"
                        : "验证中"}
                  </span>
                  <span>{currentProgress.message}</span>
                </div>
                <div className="w-full bg-muted rounded-full h-2">
                  <div
                    className="bg-primary h-2 rounded-full transition-all duration-300"
                    style={{
                      width: `${Math.max(2, (() => {
                        // Map phase progress to overall: download 0-50%, extract 50-90%, verify 90-100%
                        const p = currentProgress.progress;
                        if (currentProgress.phase === "downloading") return p * 50;
                        if (currentProgress.phase === "extracting") return 50 + p * 40;
                        return 90 + p * 10;
                      })())}%`,
                    }}
                  />
                </div>
              </div>
            )}

            {/* macOS ASR notice */}
            {dep.id === "qwen3-asr" &&
              !dep.auto_install_available &&
              dep.status !== "installed" && (
                <p className="text-xs text-amber-600">
                  macOS 暂不支持自动安装，请参考文档手动安装后使用"自定义路径"指定。
                </p>
              )}

            {/* Custom path editor */}
            {customPathEditing === dep.id && (
              <div className="flex gap-2 items-center">
                <Input
                  value={customPathValue}
                  onChange={(e) => setCustomPathValue(e.target.value)}
                  placeholder="选择或输入可执行文件路径"
                  className="text-sm h-8 font-mono flex-1"
                />
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handlePickCustomPath(dep.id)}
                >
                  浏览...
                </Button>
                <Button size="sm" onClick={() => handleCustomPathSave(dep.id)}>
                  保存
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setCustomPathEditing(null)}
                >
                  取消
                </Button>
              </div>
            )}

            {/* Action buttons */}
            <div className="flex items-center gap-2 pt-1">
              {dep.status === "installed" ? (
                <>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => handleReveal(dep.id)}
                  >
                    打开目录
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-red-500 hover:text-red-600"
                    onClick={() => handleUninstall(dep)}
                  >
                    卸载
                  </Button>
                </>
              ) : dep.status === "not_installed" || dep.status === "error" ? (
                dep.auto_install_available && (
                  <Button
                    size="sm"
                    onClick={() => handleInstall(dep.id)}
                    disabled={isInstalling}
                  >
                    {isInstalling ? "安装中..." : "安装"}
                  </Button>
                )
              ) : null}

              {/* Custom path button (always available when not installing) */}
              {!isInstalling && customPathEditing !== dep.id && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setCustomPathEditing(dep.id);
                    setCustomPathValue(dep.custom_path ?? "");
                  }}
                >
                  自定义路径
                </Button>
              )}
            </div>
          </section>
        );
      })}
    </div>
  );
}

// ===== System Settings Tab =====
function SystemSettingsTab() {
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
    getSettings(["plugin_dir"])
      .then((settings) => {
        if (settings.plugin_dir) setPluginDir(settings.plugin_dir);
        setPluginDirLoaded(true);
      })
      .catch(console.error);

    getAppInfo().then(setAppInfo).catch(console.error);
  }, []);

  useEffect(() => {
    if (appInfo && pluginDirLoaded && !pluginDir) {
      setPluginDir(`${appInfo.data_dir}/plugins`);
    }
  }, [appInfo, pluginDirLoaded, pluginDir]);

  const handleSave = async () => {
    setSaving(true); setSaved(false);
    try {
      if (pluginDir) await setSetting("plugin_dir", pluginDir);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) { alert("保存失败: " + String(e)); }
    finally { setSaving(false); }
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

interface SettingsSearchParams {
  section?: string;
}

function SettingsPage() {
  const { section } = Route.useSearch();
  const [activeTab, setActiveTab] = useState<string>("workspace");
  const [configPlugins, setConfigPlugins] = useState<PluginInfo[]>([]);
  const [activeWorkspace, setActiveWorkspace] = useState<WorkspaceInfo | null>(
    null
  );
  const [allWorkspaces, setAllWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const wsActiveId = useWorkspaceStore((s) => s.activeId);
  const wsVersion = useWorkspaceStore((s) => s.version);
  const workspaceListRef = useRef<HTMLDivElement>(null);


  const loadWorkspaces = useCallback(async () => {
    try {
      const all = await listWorkspaces();
      setAllWorkspaces(all);
      const ws = all.find((w) => w.id === wsActiveId) ?? null;
      setActiveWorkspace(ws);
    } catch (e) {
      console.error("Failed to load workspaces:", e);
    }
  }, [wsActiveId]);

  useEffect(() => {
    setLoading(true);

    // Load plugins with config
    listPlugins()
      .then((list) => {
        setConfigPlugins(list.filter((p) => p.has_config));
      })
      .catch(console.error)
      .finally(() => setLoading(false));

    // Load workspaces
    loadWorkspaces();
  }, [wsActiveId, wsVersion, loadWorkspaces]);

  // Auto-scroll to workspace list when section=workspaces
  useEffect(() => {
    if (section !== "workspaces" || !allWorkspaces.length) return;
    requestAnimationFrame(() => {
      workspaceListRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }, [section, allWorkspaces]);

  // Sync active tab from URL section param
  useEffect(() => {
    if (section === "tags") {
      setActiveTab("tags");
    } else if (section === "asr") {
      setActiveTab("asr");
    } else if (section === "deps") {
      setActiveTab("deps");
    } else if (section && section !== "workspaces" && configPlugins.some((p) => p.id === section)) {
      setActiveTab(section);
    } else if (section === "system") {
      setActiveTab("system");
    } else {
      setActiveTab("workspace");
    }
  }, [section, configPlugins]);

  return (
    <div className="space-y-4 p-6">
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value="workspace">工作区</TabsTrigger>
          <TabsTrigger value="tags">标签</TabsTrigger>
          <TabsTrigger value="asr">语音识别</TabsTrigger>
          <TabsTrigger value="deps">依赖管理</TabsTrigger>
          <TabsTrigger value="system">系统设置</TabsTrigger>
          {configPlugins.map((plugin) => (
            <TabsTrigger key={plugin.id} value={plugin.id}>
              {plugin.name}
            </TabsTrigger>
          ))}
        </TabsList>

        <TabsContent value="workspace" className="mt-4 space-y-6">
          {activeWorkspace ? (
            <WorkspaceSettingsTab workspace={activeWorkspace} />
          ) : (
            <div className="text-muted-foreground text-sm">加载工作区信息中...</div>
          )}

          {/* Workspace list */}
          <div ref={workspaceListRef}>
            <WorkspaceListSection
              allWorkspaces={allWorkspaces}
              onReload={loadWorkspaces}
            />
          </div>
        </TabsContent>

        <TabsContent value="tags" className="mt-4">
          <TagManager />
        </TabsContent>

        <TabsContent value="asr" className="mt-4">
          <ASRSettingsTab />
        </TabsContent>

        <TabsContent value="deps" className="mt-4">
          <DependencyManagerTab />
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
  validateSearch: (search: Record<string, unknown>): SettingsSearchParams => ({
    section: (search.section as string) || undefined,
  }),
});
