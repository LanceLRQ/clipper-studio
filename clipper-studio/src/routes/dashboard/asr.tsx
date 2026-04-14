import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState, useCallback, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
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
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { getSettings, setSetting } from "@/services/settings";
import {
  checkASRHealth,
  validateASRPath,
  startASRService,
  stopASRService,
  openASRSetupTerminal,
  getASRServiceStatus,
  getASRServiceLogs,
  type ASRHealthInfo,
  type ASRPathValidation,
  type ASRServiceStatusInfo,
} from "@/services/asr";
import { useASRQueueStore, useASRActiveTasks } from "@/stores/asr-queue";
import type { ASRQueueItem } from "@/services/asr";

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
  const [asrLocalEnablePunc, setAsrLocalEnablePunc] = useState("false");
  const [asrLocalModelSource, setAsrLocalModelSource] = useState("modelscope");
  const [asrLocalMaxSegment, setAsrLocalMaxSegment] = useState("5");
  const [pathValidation, setPathValidation] = useState<ASRPathValidation | null>(null);
  const [validating, setValidating] = useState(false);
  const [serviceStatus, setServiceStatus] = useState<ASRServiceStatusInfo | null>(null);
  const [serviceLogs, setServiceLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [serviceActionLoading, setServiceActionLoading] = useState(false);
  const logsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    getSettings([
      "asr_mode", "asr_port", "asr_url", "asr_api_key", "asr_language", "asr_max_chars",
      "asr_local_path", "asr_local_device", "asr_local_model_size",
      "asr_local_enable_align", "asr_local_enable_punc", "asr_local_model_source",
      "asr_local_max_segment",
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

  // Auto-trigger health check when local service becomes running
  useEffect(() => {
    if (asrMode === "local" && serviceStatus?.status === "running") {
      handleCheckHealth();
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serviceStatus?.status]);

  // Auto-scroll logs to bottom when new lines arrive
  useEffect(() => {
    if (showLogs && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [serviceLogs, showLogs]);

  // Listen for real-time log events from the managed process
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<string>("asr-service-log", (event) => {
      setServiceLogs((prev) => {
        const next = [...prev, event.payload];
        return next.length > 2000 ? next.slice(-2000) : next;
      });
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
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 h-full min-h-0">
      {/* Left column: Queue Display + Basic ASR Settings */}
      <div className="overflow-y-auto min-h-0 pr-1">
      <ASRQueueDisplay />
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
      </div>

      {/* Right column: Service Control (sticky) + Local Config (scrollable) */}
      <div className="flex flex-col min-h-0 h-full gap-6">
          {/* Local Service Control - sticky */}
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

            {/* Action buttons */}
            <div className="flex flex-wrap items-center gap-3">
              {serviceStatus?.status === "running" || serviceStatus?.status === "starting" ? (
                <>
                  <Button
                    variant="destructive"
                    onClick={handleStopService}
                    disabled={serviceActionLoading || serviceStatus?.status === "stopping"}
                  >
                    {serviceStatus?.status === "stopping" ? "停止中..." : "停止服务"}
                  </Button>
                </>
              ) : (
                <Button
                  onClick={handleStartService}
                  disabled={serviceActionLoading || !pathValidation?.valid}
                >
                  {serviceActionLoading ? "操作中..." : "启动服务"}
                </Button>
              )}

              <Button variant="outline" onClick={() => { handleRefreshValidation(); getASRServiceStatus().then(setServiceStatus).catch(console.error); }}>
                刷新状态
              </Button>

              <Button variant="outline" onClick={handleViewLogs}>
                查看日志
              </Button>
            </div>
          </section>

          {/* Local Service Config - scrollable */}
          <div className="overflow-y-auto min-h-0 flex-1 pr-1">
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
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Label className="text-xs cursor-help border-b border-dashed border-muted-foreground">VAD 合并阈值 (秒)</Label>
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
                    <TooltipTrigger asChild>
                      <Label className="text-xs cursor-help border-b border-dashed border-muted-foreground">启用字级对齐</Label>
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
                    <TooltipTrigger asChild>
                      <Label className="text-xs cursor-help border-b border-dashed border-muted-foreground">启用标点恢复</Label>
                    </TooltipTrigger>
                    <TooltipContent side="top" className="max-w-64">
                      仅在 ASR 识别结果无标点时尝试开启，否则可能产生反作用
                    </TooltipContent>
                  </Tooltip>
                </div>
              </div>
            </div>

            <div className="flex items-center gap-3 pt-2">
              <Button onClick={handleSave} disabled={saving}>
                {saving ? "保存中..." : saved ? "已保存 ✓" : "保存设置"}
              </Button>
            </div>
          </section>
          </div>
      </div>

      {/* Logs Dialog */}
      <Dialog open={showLogs} onOpenChange={setShowLogs}>
        <DialogContent className="max-w-[60vw] sm:max-w-[60vw] h-[80vh] flex flex-col">
          <DialogHeader>
            <DialogTitle>ASR 服务日志</DialogTitle>
          </DialogHeader>
          <div className="flex-1 overflow-auto rounded-md border bg-muted/30 p-3 min-h-0">
            <pre className="text-[11px] font-mono whitespace-pre-wrap leading-relaxed">
              {serviceLogs.length > 0 ? serviceLogs.join("\n") : "暂无日志"}
            </pre>
            <div ref={logsEndRef} />
          </div>
        </DialogContent>
      </Dialog>
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

  if (activeTasks.length === 0) return null;

  // Running task first, then queued tasks
  const running = activeTasks.filter(
    (t) => t.status === "converting" || t.status === "submitting" || t.status === "processing"
  );
  const queued = activeTasks.filter((t) => t.status === "queued");
  const sorted = [...running, ...queued];

  return (
    <section className="rounded-lg border p-5 space-y-3 mb-6">
      <div className="flex items-center justify-between">
        <h3 className="font-medium text-lg">识别队列</h3>
        <span className="text-xs text-muted-foreground">{activeTasks.length} 个任务</span>
      </div>
      <div className="space-y-2">
        {sorted.map((task) => (
          <ASRQueueTaskRow
            key={task.task_id}
            task={task}
            queuePosition={task.status === "queued" ? queued.indexOf(task) + 1 : undefined}
            onCancel={() => cancelTask(task.task_id)}
          />
        ))}
      </div>
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
          <span className="text-sm shrink-0">
            {isRunning ? "▶" : "⏳"}
          </span>
          <span className="text-sm truncate" title={task.video_file_name}>
            {task.video_file_name}
          </span>
        </div>
        <button
          className="shrink-0 px-1.5 py-0.5 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-colors text-xs leading-none"
          onClick={onCancel}
          title="取消任务"
        >
          ✕
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
