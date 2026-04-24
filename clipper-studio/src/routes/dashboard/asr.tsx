import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState, useCallback, useRef } from "react";
import { open, ask } from "@tauri-apps/plugin-dialog";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";
import {
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
} from "@/components/ui/tabs";
import { Check, Loader2, Play, X } from "lucide-react";
import { getSettings, setSetting } from "@/services/settings";
import {
  checkASRHealth,
  validateASRPath,
  startASRService,
  stopASRService,
  openASRSetupTerminal,
  getASRServiceStatus,
  getASRServiceLogs,
  checkDockerCapability,
  checkDockerImagePulled,
  openDockerPullTerminal,
  forceRemoveASRContainer,
  getDefaultASRDockerDataDir,
  ERR_CONTAINER_CONFLICT,
  type ASRHealthInfo,
  type ASRPathValidation,
  type ASRServiceStatusInfo,
  type DockerCapability,
} from "@/services/asr";
import { useASRQueueStore, useASRActiveTasks, useASRActiveCount } from "@/stores/asr-queue";
import type { ASRQueueItem } from "@/services/asr";

type ASRMode = "local" | "remote" | "disabled";
type ASRLaunchMode = "native" | "docker";

const DOCKER_IMAGE_PREFIX = "lancelrq/qwen3-asr-service:";

interface DockerImageOption {
  value: string;
  label: string;
  hint?: string;
}

/** Filter docker image options by host platform/arch */
function getDockerImageOptions(cap: DockerCapability | null): DockerImageOption[] {
  if (!cap) return [];
  const { host_platform, host_arch } = cap;
  if (host_arch === "arm64") {
    return [
      { value: DOCKER_IMAGE_PREFIX + "latest-arm64", label: "ARM64 (OpenVINO FP32，推荐)" },
      { value: DOCKER_IMAGE_PREFIX + "latest-cpu", label: "CPU (通过 linux/amd64 模拟)" },
    ];
  }
  if (host_platform === "windows" || host_platform === "linux") {
    return [
      { value: DOCKER_IMAGE_PREFIX + "latest", label: "CUDA (需 NVIDIA GPU 和 nvidia-docker)" },
      { value: DOCKER_IMAGE_PREFIX + "latest-cpu", label: "CPU (OpenVINO INT8)" },
    ];
  }
  // macOS Intel
  return [{ value: DOCKER_IMAGE_PREFIX + "latest-cpu", label: "CPU (OpenVINO INT8)" }];
}

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

export const Route = createFileRoute("/dashboard/asr")({
  component: ASRPage,
});

function ASRPage() {
  return (
    <div className="h-full p-6">
      <ASRSettingsContent />
    </div>
  );
}

function ASRSettingsContent() {
  const activeTaskCount = useASRActiveCount();
  const [asrMode, setAsrMode] = useState<ASRMode>("local");
  const [asrHost, setAsrHost] = useState("127.0.0.1");
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
  const [launchMode, setLaunchMode] = useState<ASRLaunchMode>("native");
  const [asrLocalPath, setAsrLocalPath] = useState("");
  const [asrLocalDevice, setAsrLocalDevice] = useState("auto");
  const [asrLocalModelSize, setAsrLocalModelSize] = useState("auto");
  const [asrLocalEnableAlign, setAsrLocalEnableAlign] = useState("true");
  const [asrLocalEnablePunc, setAsrLocalEnablePunc] = useState("false");
  const [asrLocalModelSource, setAsrLocalModelSource] = useState("modelscope");
  const [asrLocalMaxSegment, setAsrLocalMaxSegment] = useState("5");
  const [pathValidation, setPathValidation] = useState<ASRPathValidation | null>(null);
  const [validating, setValidating] = useState(false);

  // Docker mode
  const [dockerCap, setDockerCap] = useState<DockerCapability | null>(null);
  const [dockerImage, setDockerImage] = useState("");
  const [dockerDataDir, setDockerDataDir] = useState("");
  const [imagePulled, setImagePulled] = useState<boolean | null>(null);
  const [checkingImage, setCheckingImage] = useState(false);
  const dockerImageOptions = getDockerImageOptions(dockerCap);
  const dockerReady = dockerCap?.installed === true && dockerCap?.daemon_running === true;
  const [serviceStatus, setServiceStatus] = useState<ASRServiceStatusInfo | null>(null);
  const [serviceLogs, setServiceLogs] = useState<string[]>([]);
  const [leftTab, setLeftTab] = useState<"general" | "local-service">("general");
  const [rightTab, setRightTab] = useState<"logs" | "tasks">("tasks");
  const [serviceActionLoading, setServiceActionLoading] = useState(false);
  const [slowStartMessage, setSlowStartMessage] = useState<string | null>(null);
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    getSettings([
      "asr_mode", "asr_host", "asr_port", "asr_url", "asr_api_key", "asr_language", "asr_max_chars",
      "asr_local_path", "asr_local_device", "asr_local_model_size",
      "asr_local_enable_align", "asr_local_enable_punc", "asr_local_model_source",
      "asr_local_max_segment",
      "asr_launch_mode", "asr_docker_image", "asr_docker_data_dir",
    ])
      .then(async (s) => {
        if (s.asr_mode) setAsrMode(s.asr_mode as ASRMode);
        if (s.asr_host) setAsrHost(s.asr_host);
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
        if (s.asr_launch_mode === "docker" || s.asr_launch_mode === "native") {
          setLaunchMode(s.asr_launch_mode);
        }
        if (s.asr_docker_image) setDockerImage(s.asr_docker_image);
        if (s.asr_docker_data_dir) {
          setDockerDataDir(s.asr_docker_data_dir);
        } else {
          try {
            const def = await getDefaultASRDockerDataDir();
            setDockerDataDir(def);
          } catch { /* ignore */ }
        }
        if (s.asr_local_path) {
          validateASRPath(s.asr_local_path).then(setPathValidation).catch(console.error);
        }
      })
      .catch(console.error);

    getASRServiceStatus().then(setServiceStatus).catch(console.error);
    checkDockerCapability().then(setDockerCap).catch(console.error);
  }, []);

  // Detect whether the chosen docker image has been pulled
  useEffect(() => {
    if (launchMode !== "docker" || !dockerImage) {
      setImagePulled(null);
      return;
    }
    let cancelled = false;
    setCheckingImage(true);
    checkDockerImagePulled(dockerImage)
      .then((ok) => { if (!cancelled) setImagePulled(ok); })
      .catch(() => { if (!cancelled) setImagePulled(false); })
      .finally(() => { if (!cancelled) setCheckingImage(false); });
    return () => { cancelled = true; };
  }, [launchMode, dockerImage]);

  // Listen for real-time service status events
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: UnlistenFn | undefined;
    listen<ASRServiceStatusInfo>("asr-service-status", (event) => {
      setServiceStatus(event.payload);
      // Clear slow-start hint when service leaves Starting state
      if (event.payload.status !== "starting") {
        setSlowStartMessage(null);
      }
    }).then((fn) => { if (cancelled) { fn(); } else { unlistenFn = fn; } });
    return () => { cancelled = true; unlistenFn?.(); };
  }, []);

  // Listen for slow-start notification (model download taking long)
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: UnlistenFn | undefined;
    listen<string>("asr-service-slow-start", (event) => {
      setSlowStartMessage(event.payload);
    }).then((fn) => { if (cancelled) { fn(); } else { unlistenFn = fn; } });
    return () => { cancelled = true; unlistenFn?.(); };
  }, []);

  // Auto-trigger health check when local service becomes running
  useEffect(() => {
    if (asrMode === "local" && serviceStatus?.status === "running") {
      handleCheckHealth();
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serviceStatus?.status]);

  // Auto-scroll logs to bottom when new lines arrive and logs tab is active
  useEffect(() => {
    if (rightTab === "logs" && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [serviceLogs, rightTab]);

  // Backfill historical logs once on mount; real-time events will append later
  useEffect(() => {
    getASRServiceLogs(200).then(setServiceLogs).catch(console.error);
  }, []);

  // Listen for real-time log events from the managed process
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: UnlistenFn | undefined;
    listen<string>("asr-service-log", (event) => {
      setServiceLogs((prev) => {
        const next = [...prev, event.payload];
        return next.length > 2000 ? next.slice(-2000) : next;
      });
    }).then((fn) => { if (cancelled) { fn(); } else { unlistenFn = fn; } });
    return () => { cancelled = true; unlistenFn?.(); };
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

  const handleRefreshValidation = async () => {
    if (!asrLocalPath.trim()) return;
    setValidating(true);
    try { setPathValidation(await validateASRPath(asrLocalPath)); }
    catch { setPathValidation(null); }
    finally { setValidating(false); }
  };

  const handleOpenSetupTerminal = async () => {
    try { await openASRSetupTerminal(); }
    catch (e) { alert("打开终端失败: " + String(e)); }
  };

  const handleSave = async () => {
    setSaving(true); setSaved(false);
    try {
      await setSetting("asr_mode", asrMode);
      await setSetting("asr_host", asrHost);
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
      await setSetting("asr_launch_mode", launchMode);
      await setSetting("asr_docker_image", dockerImage);
      await setSetting("asr_docker_data_dir", dockerDataDir);
      window.dispatchEvent(new CustomEvent("asr-settings-changed"));
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
    setRightTab("logs");
    try {
      await handleSave();
      try {
        await startASRService();
      } catch (e) {
        const msg = String(e);
        if (msg.includes(ERR_CONTAINER_CONFLICT)) {
          const confirmed = await ask(
            "检测到同名 Docker 容器 qwen3-asr-service 已存在，是否强制清理并重建？",
            { title: "Docker 容器冲突", kind: "warning" },
          );
          if (!confirmed) return;
          await forceRemoveASRContainer();
          await startASRService();
        } else {
          alert("启动失败: " + msg);
        }
      }
    } catch (e) {
      alert("启动失败: " + String(e));
    } finally {
      setServiceActionLoading(false);
    }
  };

  const handleRefreshDocker = async () => {
    try {
      const cap = await checkDockerCapability();
      setDockerCap(cap);
      if (dockerImage) {
        setCheckingImage(true);
        try {
          setImagePulled(await checkDockerImagePulled(dockerImage));
        } finally {
          setCheckingImage(false);
        }
      }
    } catch (e) {
      console.error("Refresh docker failed:", e);
    }
  };

  const handlePickDockerDataDir = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) setDockerDataDir(selected as string);
  };

  const handleLaunchModeChange = async (newMode: ASRLaunchMode) => {
    if (newMode === launchMode) return;
    if (activeTaskCount > 0) {
      alert(`当前有 ${activeTaskCount} 个 ASR 任务正在进行，请等待任务完成或取消后再切换启动方式。`);
      return;
    }
    if (newMode === "docker" && !dockerReady) {
      alert(dockerCap?.hint ?? "Docker 不可用");
      return;
    }
    const isActive =
      serviceStatus?.status === "running" || serviceStatus?.status === "starting";
    if (isActive) {
      const confirmed = await ask(
        "本地服务正在运行，切换启动方式需要先停止服务。是否停止并切换？",
        { title: "停止本地服务", kind: "warning" },
      );
      if (!confirmed) return;
      try {
        await stopASRService();
      } catch (e) {
        alert("停止本地服务失败: " + String(e));
        return;
      }
    }
    setLaunchMode(newMode);
  };

  const handleStopService = async () => {
    if (activeTaskCount > 0) {
      alert(`当前有 ${activeTaskCount} 个 ASR 任务正在进行，请等待任务完成或取消后再停止本地服务。`);
      return;
    }
    setServiceActionLoading(true);
    try { await stopASRService(); }
    catch (e) { alert("停止失败: " + String(e)); }
    finally { setServiceActionLoading(false); }
  };

  const handleModeChange = async (newMode: ASRMode) => {
    if (newMode === asrMode) return;
    // 有识别任务在进行时禁止切换模式
    if (activeTaskCount > 0) {
      alert(`当前有 ${activeTaskCount} 个 ASR 任务正在进行，请等待任务完成或取消后再切换识别模式。`);
      return;
    }
    // 离开 local 模式时，若本地服务在运行则先询问停止
    if (asrMode === "local" && newMode !== "local") {
      const isActive =
        serviceStatus?.status === "running" ||
        serviceStatus?.status === "starting";
      if (isActive) {
        const confirmed = await ask(
          "本地 ASR 服务正在运行，切换到其他模式需要先停止本地服务。是否停止并切换？",
          { title: "停止本地 ASR 服务", kind: "warning" },
        );
        if (!confirmed) return;
        try {
          await stopASRService();
        } catch (e) {
          alert("停止本地服务失败: " + String(e));
          return;
        }
      }
    }
    // 切换模式后清空健康检测状态（原状态对新模式已失效）
    setAsrHealth(null);
    setAsrMode(newMode);
  };

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 h-full min-h-0">
      {/* Left column: 语音识别设置 / 本地服务设置 */}
      <div className="flex flex-col min-h-0 h-full">
        <Tabs
          value={leftTab}
          onValueChange={(v) => setLeftTab(v as typeof leftTab)}
          className="flex flex-col h-full min-h-0"
        >
          <TabsList>
            <TabsTrigger value="general">语音识别设置</TabsTrigger>
            <TabsTrigger value="local-service">本地服务设置</TabsTrigger>
          </TabsList>

          <TabsContent
            value="general"
            className="mt-4 flex-1 min-h-0 flex flex-col gap-6"
          >
            <section className="rounded-lg border p-5 space-y-4 flex-1 min-h-0 overflow-y-auto">
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
                      onClick={() => handleModeChange(opt.value)}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>

              {asrMode !== "disabled" && (
                <>
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
                </>
              )}

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
                  {saving ? "保存中..." : saved ? (
                    <span className="inline-flex items-center gap-1">
                      <Check className="h-4 w-4" />
                      已保存
                    </span>
                  ) : "保存设置"}
                </Button>
                {asrMode !== "disabled" && (
                  <Button variant="outline" onClick={handleCheckHealth} disabled={asrChecking}>
                    {asrChecking ? "检测中..." : "测试连接"}
                  </Button>
                )}
              </div>

              {/* Health info */}
              {asrHealth && (
                asrHealth.status === "ready" ? (
                  <div className="text-sm bg-green-500/10 border border-green-500/20 rounded-md px-4 py-3 space-y-1">
                    <div className="text-green-600 font-medium">连接成功</div>
                    <div className="text-muted-foreground">设备: <span className="font-medium text-foreground">{asrHealth.device ?? "unknown"}</span></div>
                    <div className="text-muted-foreground">模型: <span className="font-medium text-foreground">{asrHealth.model_size ?? "unknown"}</span></div>
                  </div>
                ) : (
                  <div className="text-sm text-red-500 bg-red-500/10 border border-red-500/20 rounded-md px-4 py-3">
                    连接失败
                  </div>
                )
              )}
            </section>

            {/* 本地服务控制 - 仅在本地引擎模式下展示，紧跟在基本设置下方 */}
            {asrMode === "local" && (
              <section className="rounded-lg border p-5 space-y-4 shrink-0">
                <div className="flex items-center justify-between">
                  <h3 className="font-medium text-lg">本地服务控制</h3>
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

                {/* Error message */}
                {serviceStatus?.status === "error" && (serviceStatus as { message?: string }).message && (
                  <p className="text-sm text-red-500">
                    {(serviceStatus as { message?: string }).message}
                  </p>
                )}

                {/* Slow-start warning (model download) */}
                {slowStartMessage && (
                  <p className="text-sm text-amber-600 bg-amber-500/10 border border-amber-500/20 rounded-md px-4 py-3">
                    {slowStartMessage}
                  </p>
                )}

                {/* Launch configuration check */}
                {(() => {
                  const launchReady =
                    launchMode === "native"
                      ? !!pathValidation?.valid
                      : dockerReady && !!dockerImage && imagePulled === true;
                  return (
                    <div className="rounded-md border bg-muted/20 px-3 py-2.5 space-y-2 text-sm">
                      <div className="flex items-center justify-between">
                        <span className="font-medium">
                          启动方式：{launchMode === "native" ? "本地源码" : "Docker"}
                        </span>
                        <span className={`text-xs inline-flex items-center gap-1 ${launchReady ? "text-green-600" : "text-amber-600"}`}>
                          {launchReady ? (
                            <>
                              <Check className="h-3.5 w-3.5" />
                              配置已就绪
                            </>
                          ) : (
                            "配置未就绪"
                          )}
                        </span>
                      </div>
                      <ul className="text-xs space-y-1">
                        {launchMode === "native" ? (
                          <li className="flex items-start gap-1.5">
                            <span className={pathValidation?.valid ? "text-green-600 shrink-0" : "text-red-500 shrink-0"}>
                              {pathValidation?.valid ? "✓" : "✗"}
                            </span>
                            <span className="flex-1 min-w-0">
                              <span className="text-muted-foreground">服务目录：</span>
                              {asrLocalPath ? (
                                pathValidation?.valid ? (
                                  <Tooltip>
                                    <TooltipTrigger render={<span className="cursor-help border-b border-dashed border-muted-foreground/60" />}>
                                      已设置
                                    </TooltipTrigger>
                                    <TooltipContent side="top" className="max-w-96 break-all font-mono text-xs">
                                      {asrLocalPath}
                                    </TooltipContent>
                                  </Tooltip>
                                ) : (
                                  <span className="text-red-500">
                                    {pathValidation == null
                                      ? "未校验"
                                      : !pathValidation.has_python_env
                                        ? "缺少 Python 环境"
                                        : "缺少入口文件 asr-service/app/main.py"}
                                  </span>
                                )
                              ) : (
                                <span className="text-red-500">未设置</span>
                              )}
                            </span>
                          </li>
                        ) : (
                          <>
                            <li className="flex items-start gap-1.5">
                              <span className={dockerReady ? "text-green-600 shrink-0" : "text-red-500 shrink-0"}>
                                {dockerReady ? "✓" : "✗"}
                              </span>
                              <span className="flex-1 min-w-0">
                                <span className="text-muted-foreground">Docker 引擎：</span>
                                {dockerReady ? (
                                  <span>{dockerCap?.version ?? "已就绪"}</span>
                                ) : (
                                  <span className="text-red-500">{dockerCap?.hint ?? "不可用或未运行"}</span>
                                )}
                              </span>
                            </li>
                            <li className="flex items-start gap-1.5">
                              <span className={dockerImage ? "text-green-600 shrink-0" : "text-red-500 shrink-0"}>
                                {dockerImage ? "✓" : "✗"}
                              </span>
                              <span className="flex-1 min-w-0">
                                <span className="text-muted-foreground">镜像：</span>
                                {dockerImage ? (
                                  <span className="break-all font-mono">{dockerImage}</span>
                                ) : (
                                  <span className="text-red-500">未选择</span>
                                )}
                              </span>
                            </li>
                            <li className="flex items-start gap-1.5">
                              <span className={imagePulled === true ? "text-green-600 shrink-0" : imagePulled === false ? "text-red-500 shrink-0" : "text-muted-foreground shrink-0"}>
                                {imagePulled === true ? "✓" : imagePulled === false ? "✗" : "…"}
                              </span>
                              <span className="flex-1 min-w-0">
                                <span className="text-muted-foreground">镜像状态：</span>
                                {imagePulled === true ? (
                                  "已拉取到本地"
                                ) : imagePulled === false ? (
                                  <span className="text-red-500">未拉取</span>
                                ) : (
                                  <span className="text-muted-foreground">检测中...</span>
                                )}
                              </span>
                            </li>
                          </>
                        )}
                      </ul>
                      {!launchReady && (
                        <div className="pt-1">
                          <Button variant="outline" size="sm" onClick={() => setLeftTab("local-service")}>
                            前往「本地服务设置」完善配置 →
                          </Button>
                        </div>
                      )}
                    </div>
                  );
                })()}

                {/* Action buttons */}
                <div className="flex flex-wrap items-center gap-3">
                  {serviceStatus?.status === "running" || serviceStatus?.status === "starting" ? (
                    <Button
                      variant="destructive"
                      onClick={handleStopService}
                      disabled={
                        serviceActionLoading ||
                        activeTaskCount > 0
                      }
                      title={activeTaskCount > 0 ? `有 ${activeTaskCount} 个识别任务进行中，无法停止服务` : undefined}
                    >
                      停止服务
                    </Button>
                  ) : (
                    <Button
                      onClick={handleStartService}
                      disabled={
                        serviceActionLoading ||
                        (launchMode === "native"
                          ? !pathValidation?.valid
                          : !dockerReady || !dockerImage || imagePulled !== true)
                      }
                      title={
                        launchMode === "docker"
                          ? !dockerReady
                            ? dockerCap?.hint ?? "Docker 不可用"
                            : !dockerImage
                            ? "请先选择 Docker 镜像"
                            : imagePulled !== true
                            ? "镜像尚未拉取到本地"
                            : undefined
                          : !pathValidation?.valid
                          ? "服务目录无效"
                          : undefined
                      }
                    >
                      {serviceActionLoading ? "操作中..." : "启动服务"}
                    </Button>
                  )}

                  <Button variant="outline" onClick={() => { handleRefreshValidation(); getASRServiceStatus().then(setServiceStatus).catch(console.error); }}>
                    刷新状态
                  </Button>
                </div>
              </section>
            )}
          </TabsContent>

          <TabsContent
            value="local-service"
            className="mt-4 flex-1 min-h-0 relative flex flex-col"
          >
            {/* Mask when current mode is not local */}
            {asrMode !== "local" && (
              <div className="absolute inset-0 z-10 flex items-center justify-center rounded-lg bg-white/60 dark:bg-black/60 backdrop-blur-[1px]">
                <div className="rounded-md border bg-background/90 px-4 py-3 shadow-sm text-sm text-muted-foreground max-w-xs text-center">
                  当前识别模式为「{asrMode === "remote" ? "远程服务" : "禁用"}」，
                  切换回「本地引擎」后即可管理本地服务。
                </div>
              </div>
            )}
            <section className="rounded-lg border p-5 flex-1 min-h-0 flex flex-col gap-4">
            <div className="flex items-center justify-between gap-2 shrink-0">
              <div className="flex items-center gap-2">
                <h3 className="font-medium text-lg">本地服务配置</h3>
                <span className="inline-flex items-center rounded-md border border-input bg-muted px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">
                  qwen3-asr-service
                </span>
              </div>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
                  title="GitHub 源码"
                  onClick={() => shellOpen("https://github.com/LanceLRQ/qwen3-asr-service").catch(console.error)}
                >
                  <GithubIcon className="h-4 w-4" />
                </button>
                <button
                  type="button"
                  className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
                  title="Docker Hub 镜像"
                  onClick={() => shellOpen("https://hub.docker.com/r/lancelrq/qwen3-asr-service").catch(console.error)}
                >
                  <DockerIcon className="h-4 w-4" />
                </button>
              </div>
            </div>

            {/* Scrollable middle content */}
            <div className="flex-1 min-h-0 overflow-y-auto space-y-4 pr-1">

            {/* Launch mode radio */}
            <div className="space-y-1">
              <Label className="text-sm">启动方式</Label>
              <div className="flex rounded-md border w-fit">
                <button
                  className={`px-4 py-1.5 text-sm ${launchMode === "native" ? "bg-accent font-medium" : ""}`}
                  onClick={() => handleLaunchModeChange("native")}
                >
                  本地源码模式
                </button>
                <Tooltip>
                  <TooltipTrigger
                    render={<button
                      className={`px-4 py-1.5 text-sm ${launchMode === "docker" ? "bg-accent font-medium" : ""} ${!dockerReady ? "opacity-50 cursor-not-allowed" : ""}`}
                      onClick={() => handleLaunchModeChange("docker")}
                    />}
                  >
                    Docker 方式
                  </TooltipTrigger>
                  <TooltipContent side="top" className="max-w-72">
                    {!dockerReady
                      ? dockerCap?.hint ?? "Docker 不可用"
                      : `${dockerCap?.version ?? "Docker"} (${dockerCap?.host_platform}/${dockerCap?.host_arch})`}
                  </TooltipContent>
                </Tooltip>
              </div>
              {dockerReady && dockerCap && (
                <p className="text-xs text-muted-foreground">
                  已检测到 Docker：{dockerCap.version ?? "unknown"} ({dockerCap.host_platform}/{dockerCap.host_arch})
                </p>
              )}
            </div>

            {/* Native: Service Path */}
            {launchMode === "native" && (
            <div className="space-y-1">
              <Label className="text-sm">服务目录路径</Label>
              <div className="flex gap-2">
                <Input
                  value={asrLocalPath}
                  onChange={(e) => handlePathChange(e.target.value)}
                  placeholder="选择 qwen3-asr-service 解压目录（根目录或 asr-service 子目录均可）"
                  className="text-sm h-8 font-mono flex-1"
                />
                <Button variant="outline" size="sm" onClick={handlePickAsrDir}>
                  浏览...
                </Button>
                <Button variant="outline" size="sm" onClick={handleRefreshValidation}
                  disabled={validating || !asrLocalPath.trim()}>
                  刷新
                </Button>
              </div>
              {validating && (
                <p className="text-xs text-muted-foreground">校验中...</p>
              )}
              {pathValidation && !validating && (
                <div className="space-y-1">
                  <p className={`text-xs break-all ${pathValidation.valid ? "text-green-600" : "text-red-500"}`}>
                    {pathValidation.valid
                      ? `路径有效 (${pathValidation.python_path})`
                      : !pathValidation.has_python_env
                        ? pathValidation.platform === "windows"
                          ? "未找到便携 Python 环境（bin/python/python.exe 或 lib/）"
                          : "未找到 Python 虚拟环境（asr-service/venv）"
                        : "未找到入口文件（asr-service/app/main.py）"}
                  </p>
                  {pathValidation.setup_hint && (
                    <p className="text-xs text-amber-600">{pathValidation.setup_hint}</p>
                  )}
                  {pathValidation && !pathValidation.has_python_env && (
                    <div className="flex gap-2 pt-1">
                      {pathValidation.platform === "windows" ? (
                        <p className="text-xs text-muted-foreground">
                          请在命令行中运行 <code className="bg-muted px-1 py-0.5 rounded text-[11px]">setup.bat</code> 安装 Python 环境
                        </p>
                      ) : (
                        <Button variant="outline" size="sm" onClick={handleOpenSetupTerminal}>
                          打开终端运行 setup.sh
                        </Button>
                      )}
                    </div>
                  )}
                </div>
              )}
            </div>
            )}

            {/* Docker: image + data dir */}
            {launchMode === "docker" && (
              <>
                <div className="space-y-1">
                  <Label className="text-sm">Docker 镜像</Label>
                  <div className="flex gap-2">
                    <select
                      value={dockerImage}
                      onChange={(e) => setDockerImage(e.target.value)}
                      className="h-8 flex-1 rounded-md border border-input bg-background px-3 py-1 text-sm"
                    >
                      <option value="">请选择镜像...</option>
                      {dockerImageOptions.map((opt) => (
                        <option key={opt.value} value={opt.value}>{opt.label}</option>
                      ))}
                    </select>
                    <Button variant="outline" size="sm" className="h-8" onClick={handleRefreshDocker}>
                      刷新
                    </Button>
                  </div>
                  {dockerImage && (
                    checkingImage ? (
                      <p className="text-xs text-muted-foreground">检测中...</p>
                    ) : imagePulled === false ? (
                      <div className="space-y-1.5">
                        <p className="text-xs text-amber-600">未检测到镜像 {dockerImage}</p>
                        <div className="flex gap-2">
                          <Button variant="outline" size="sm" className="h-8"
                            onClick={() => openDockerPullTerminal(dockerImage).catch((e) => alert("打开终端失败: " + String(e)))}>
                            打开终端拉取
                          </Button>
                          <Button variant="outline" size="sm" className="h-8" onClick={handleRefreshDocker}>
                            我已拉取，检测
                          </Button>
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          首次 pull 可能较慢（GPU 镜像约 8-10GB，CPU/ARM64 约 3-4GB）
                        </p>
                      </div>
                    ) : imagePulled === true ? (
                      <div className="flex items-center gap-2 flex-wrap">
                        <p className="text-xs text-green-600 inline-flex items-center gap-1">
                          <Check className="h-3.5 w-3.5" />
                          镜像已就绪
                        </p>
                        <Button
                          variant="outline"
                          size="sm"
                          className="h-7"
                          onClick={() => openDockerPullTerminal(dockerImage).catch((e) => alert("打开终端失败: " + String(e)))}
                          title="在终端中重新拉取以获取最新版本"
                        >
                          重新拉取
                        </Button>
                      </div>
                    ) : null
                  )}
                </div>

                <div className="space-y-1">
                  <Label className="text-sm">数据目录</Label>
                  <div className="flex gap-2">
                    <Input
                      value={dockerDataDir}
                      onChange={(e) => setDockerDataDir(e.target.value)}
                      placeholder="数据目录（默认为 App 数据目录）"
                      className="text-sm h-8 font-mono flex-1"
                    />
                    <Button variant="outline" size="sm" className="h-8" onClick={handlePickDockerDataDir}>
                      浏览...
                    </Button>
                  </div>
                  <p className="text-[11px] text-muted-foreground break-all">
                    将挂载 <code className="bg-muted px-1 py-0.5 rounded text-[10px]">{dockerDataDir || "(未设置)"}/models</code> → <code className="bg-muted px-1 py-0.5 rounded text-[10px]">/app/models</code>（你可以选择本地源码的 asr-service 目录，以便共享使用模型文件夹）
                  </p>
                  <p className="text-[11px] text-muted-foreground">
                    首次启动容器会自动下载模型到该目录，可能需要 5-15 分钟
                  </p>
                </div>
              </>
            )}

            {/* Startup Parameters */}
            <div className="space-y-3 pt-2">
              <h4 className="text-sm font-medium">启动参数</h4>
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1">
                  <Tooltip>
                    <TooltipTrigger render={<Label className="text-xs cursor-help border-b border-dashed border-muted-foreground w-fit" />}>
                      主机地址 (Host)
                    </TooltipTrigger>
                    <TooltipContent side="top" className="max-w-72">
                      本地 ASR 服务监听地址，默认 127.0.0.1 仅本机可访问；若需局域网其他设备访问请改为 0.0.0.0
                    </TooltipContent>
                  </Tooltip>
                  <Input value={asrHost} onChange={(e) => setAsrHost(e.target.value)}
                    placeholder="127.0.0.1" className="text-sm h-8 font-mono" />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">端口 (Port)</Label>
                  <Input value={asrPort} onChange={(e) => setAsrPort(e.target.value)}
                    placeholder="8765" className="text-sm h-8 font-mono" type="number" min={1} max={65535} />
                </div>
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
                  <Tooltip>
                    <TooltipTrigger render={<Label className="text-xs cursor-help border-b border-dashed border-muted-foreground w-fit" />}>
                      VAD 合并阈值 (秒)
                    </TooltipTrigger>
                    <TooltipContent side="top" className="max-w-72">
                      设定语音段落的最长窗口（5~30秒）。若无法启用字级对齐，可尝试缩短该值以获取合适长度的字幕
                    </TooltipContent>
                  </Tooltip>
                  <Input value={asrLocalMaxSegment} onChange={(e) => setAsrLocalMaxSegment(e.target.value)}
                    placeholder="5" className="text-sm h-8" type="number" min={5} max={30} />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div className="flex items-center gap-2">
                  <select value={asrLocalEnableAlign} onChange={(e) => setAsrLocalEnableAlign(e.target.value)}
                    className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="true">是</option>
                    <option value="false">否</option>
                  </select>
                  <Tooltip>
                    <TooltipTrigger render={<Label className="text-xs cursor-help border-b border-dashed border-muted-foreground w-fit" />}>
                      启用字级对齐
                    </TooltipTrigger>
                    <TooltipContent side="top" className="max-w-64">
                      关闭后，字幕按字数自动拆分功能将会失效
                    </TooltipContent>
                  </Tooltip>
                </div>
                <div className="flex items-center gap-2">
                  <select value={asrLocalEnablePunc} onChange={(e) => setAsrLocalEnablePunc(e.target.value)}
                    className="h-8 rounded-md border border-input bg-background px-3 py-1 text-sm">
                    <option value="true">是</option>
                    <option value="false">否</option>
                  </select>
                  <Tooltip>
                    <TooltipTrigger render={<Label className="text-xs cursor-help border-b border-dashed border-muted-foreground w-fit" />}>
                      启用标点恢复
                    </TooltipTrigger>
                    <TooltipContent side="top" className="max-w-64">
                      仅在 ASR 识别结果无标点时尝试开启，否则可能产生反作用
                    </TooltipContent>
                  </Tooltip>
                </div>
              </div>
            </div>

            </div>
            {/* End of scrollable middle content */}

            <div className="flex items-center gap-3 pt-2 shrink-0 border-t">
              <Button onClick={handleSave} disabled={saving}>
                {saving ? "保存中..." : saved ? (
                  <span className="inline-flex items-center gap-1">
                    <Check className="h-4 w-4" />
                    已保存
                  </span>
                ) : "保存设置"}
              </Button>
            </div>
            </section>
          </TabsContent>
        </Tabs>
      </div>

      {/* Right column: ASR 任务 / 本地 ASR 日志 */}
      <div className="flex flex-col min-h-0 h-full">
        <Tabs
          value={rightTab}
          onValueChange={(v) => setRightTab(v as typeof rightTab)}
          className="flex flex-col h-full min-h-0"
        >
          <TabsList>
            <TabsTrigger value="tasks">ASR 任务</TabsTrigger>
            <TabsTrigger value="logs">本地 ASR 日志</TabsTrigger>
          </TabsList>

          <TabsContent
            value="tasks"
            className="mt-4 flex-1 min-h-0 flex flex-col"
          >
            <ASRQueueDisplay />
          </TabsContent>

          <TabsContent
            value="logs"
            className="mt-4 flex-1 min-h-0 flex flex-col"
          >
            <div className="flex-1 overflow-auto rounded-md border bg-muted/30 p-3 min-h-0">
              <pre className="text-[11px] font-mono whitespace-pre-wrap leading-relaxed">
                {serviceLogs.length > 0 ? serviceLogs.join("\n") : "暂无日志"}
              </pre>
              <div ref={logsEndRef} />
            </div>
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}

// ==================== ASR Queue Display ====================

function queueStatusLabel(status: string): string {
  switch (status) {
    case "queued": return "排队中";
    case "converting": return "音频转换中...";
    case "submitting": return "提交任务...";
    case "processing": return "识别中";
    default: return status;
  }
}

function ASRQueueDisplay() {
  const activeTasks = useASRActiveTasks();
  const cancelTask = useASRQueueStore((s) => s.cancelTask);

  // Running task first, then queued tasks
  const running = activeTasks.filter(
    (t) => t.status === "converting" || t.status === "submitting" || t.status === "processing"
  );
  const queued = activeTasks.filter((t) => t.status === "queued");
  const sorted = [...running, ...queued];

  return (
    <section className="rounded-lg border p-5 flex-1 min-h-0 flex flex-col gap-3">
      <div className="flex items-center justify-between shrink-0">
        <h3 className="font-medium text-lg">识别队列</h3>
        <span className="text-xs text-muted-foreground">{activeTasks.length} 个任务</span>
      </div>
      {sorted.length === 0 ? (
        <p className="text-sm text-muted-foreground py-4 text-center">暂无识别任务</p>
      ) : (
        <div className="space-y-2 flex-1 min-h-0 overflow-y-auto">
          {sorted.map((task) => (
            <ASRQueueTaskRow
              key={task.task_id}
              task={task}
              queuePosition={task.status === "queued" ? queued.indexOf(task) + 1 : undefined}
              onCancel={() => cancelTask(task.task_id)}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function ASRQueueTaskRow({
  task,
  queuePosition,
  onCancel,
}: {
  task: ASRQueueItem;
  queuePosition?: number;
  onCancel: () => void;
}) {
  const isRunning = task.status !== "queued";

  return (
    <div className="rounded-md border bg-muted/30 px-3 py-2 space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <span className="shrink-0">
            {isRunning ? (
              <Play className="h-3.5 w-3.5 text-green-600" />
            ) : (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
            )}
          </span>
          <span className="text-sm truncate" title={task.video_file_name}>
            {task.video_file_name}
          </span>
        </div>
        <button
          className="shrink-0 p-1 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors"
          onClick={onCancel}
          title="取消任务"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>
          {queuePosition != null
            ? `${queueStatusLabel(task.status)}（第 ${queuePosition} 位）`
            : task.status === "processing"
              ? `${queueStatusLabel(task.status)} ${Math.round(task.progress * 100)}%`
              : queueStatusLabel(task.status)}
        </span>
      </div>
      {task.status === "processing" && (
        <div className="w-full h-1.5 bg-muted rounded-full overflow-hidden">
          <div
            className="h-full bg-primary transition-all duration-300"
            style={{ width: `${task.progress * 100}%` }}
          />
        </div>
      )}
      {(task.status === "converting" || task.status === "submitting") && (
        <div className="w-full h-1.5 bg-muted rounded-full overflow-hidden">
          <div className="h-full bg-primary/60 animate-pulse rounded-full" style={{ width: "100%" }} />
        </div>
      )}
    </div>
  );
}

// ==================== Brand Icons ====================

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden="true">
      <path d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.28-.01-1.02-.02-2-3.2.69-3.87-1.54-3.87-1.54-.52-1.33-1.28-1.69-1.28-1.69-1.05-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.28 1.19-3.09-.12-.29-.52-1.46.11-3.04 0 0 .97-.31 3.18 1.18a11.05 11.05 0 0 1 5.79 0c2.21-1.49 3.18-1.18 3.18-1.18.63 1.58.23 2.75.11 3.04.74.81 1.19 1.83 1.19 3.09 0 4.42-2.7 5.39-5.27 5.68.41.36.78 1.06.78 2.14 0 1.55-.01 2.8-.01 3.18 0 .31.21.68.8.56C20.21 21.39 23.5 17.08 23.5 12 23.5 5.65 18.35.5 12 .5z" />
    </svg>
  );
}

function DockerIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden="true">
      <path d="M13.983 11.078h2.119a.186.186 0 0 0 .186-.185V9.006a.186.186 0 0 0-.186-.186h-2.119a.185.185 0 0 0-.185.185v1.888c0 .102.083.185.185.185zm-2.954-5.43h2.119a.186.186 0 0 0 .185-.186V3.574a.186.186 0 0 0-.185-.185h-2.119a.185.185 0 0 0-.185.185v1.888c0 .102.082.185.185.185zm0 2.716h2.119a.186.186 0 0 0 .185-.186V6.29a.185.185 0 0 0-.185-.185h-2.119a.185.185 0 0 0-.185.185v1.887c0 .102.082.186.185.186zm-2.93 0h2.12a.185.185 0 0 0 .184-.186V6.29a.185.185 0 0 0-.185-.185H8.1a.185.185 0 0 0-.185.185v1.887c0 .102.083.186.185.186zm-2.964 0h2.12a.185.185 0 0 0 .184-.186V6.29a.185.185 0 0 0-.184-.185H5.136a.185.185 0 0 0-.186.185v1.887c0 .102.084.186.186.186zm5.893 2.715h2.12a.186.186 0 0 0 .184-.185V9.006a.186.186 0 0 0-.184-.186h-2.12a.185.185 0 0 0-.184.185v1.888c0 .102.082.185.185.185zm-2.93 0h2.12a.185.185 0 0 0 .184-.185V9.006a.185.185 0 0 0-.184-.186H8.1a.185.185 0 0 0-.185.185v1.888c0 .102.083.185.185.185zm-2.964 0h2.12a.185.185 0 0 0 .184-.185V9.006a.185.185 0 0 0-.184-.186H5.136a.185.185 0 0 0-.186.185v1.888c0 .102.084.185.186.185zm-2.92 0h2.12a.185.185 0 0 0 .184-.185V9.006a.185.185 0 0 0-.184-.186H2.215a.185.185 0 0 0-.185.185v1.888c0 .102.082.185.185.185zM23.763 9.89c-.065-.051-.672-.51-1.954-.51-.338.001-.676.03-1.01.087-.248-1.7-1.653-2.53-1.716-2.566l-.344-.199-.226.327c-.284.438-.49.922-.612 1.43-.23.97-.09 1.882.403 2.661-.595.332-1.55.413-1.744.42H.751a.751.751 0 0 0-.75.748 11.376 11.376 0 0 0 .692 4.062c.545 1.428 1.355 2.48 2.41 3.124 1.18.723 3.1 1.137 5.275 1.137a16.06 16.06 0 0 0 2.91-.263 12.07 12.07 0 0 0 3.793-1.383 10.39 10.39 0 0 0 2.585-2.114c1.243-1.41 1.984-2.98 2.535-4.376l.226-.001c1.37 0 2.213-.547 2.678-1.005.308-.293.55-.65.71-1.046l.099-.292z" />
    </svg>
  );
}
