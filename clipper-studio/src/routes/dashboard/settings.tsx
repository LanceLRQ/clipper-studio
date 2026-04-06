import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { getSettings, setSetting } from "@/services/settings";
import { checkASRHealth, type ASRHealthInfo } from "@/services/asr";
import { getAppInfo } from "@/services/workspace";

type ASRMode = "local" | "remote" | "disabled";

function SettingsPage() {
  // ASR settings
  const [asrMode, setAsrMode] = useState<ASRMode>("local");
  const [asrPort, setAsrPort] = useState("8765");
  const [asrUrl, setAsrUrl] = useState("");
  const [asrApiKey, setAsrApiKey] = useState("");
  const [asrHealth, setAsrHealth] = useState<ASRHealthInfo | null>(null);
  const [asrChecking, setAsrChecking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

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
    // Load settings
    getSettings(["asr_mode", "asr_port", "asr_url", "asr_api_key"])
      .then((settings) => {
        if (settings.asr_mode)
          setAsrMode(settings.asr_mode as ASRMode);
        if (settings.asr_port) setAsrPort(settings.asr_port);
        if (settings.asr_url) setAsrUrl(settings.asr_url);
        if (settings.asr_api_key) setAsrApiKey(settings.asr_api_key);
      })
      .catch(console.error);

    // Load app info
    getAppInfo()
      .then(setAppInfo)
      .catch(console.error);
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setSaved(false);
    try {
      await setSetting("asr_mode", asrMode);
      await setSetting("asr_port", asrPort);
      if (asrUrl) await setSetting("asr_url", asrUrl);
      if (asrApiKey) await setSetting("asr_api_key", asrApiKey);
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
    } catch (e) {
      setAsrHealth({ status: "error", device: null, model_size: null });
    } finally {
      setAsrChecking(false);
    }
  };

  return (
    <div className="space-y-6">
      <h2 className="text-2xl font-semibold">设置</h2>

      {/* ===== ASR Settings ===== */}
      <section className="rounded-lg border p-5 space-y-4">
        <h3 className="font-medium text-lg">ASR 语音识别</h3>

        {/* Mode selector */}
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

        {/* Local mode settings */}
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

        {/* Remote mode settings */}
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

        {/* Disabled mode info */}
        {asrMode === "disabled" && (
          <p className="text-sm text-muted-foreground pl-1">
            ASR 功能已禁用，视频详情页中将不会显示语音识别按钮。
          </p>
        )}

        {/* Health check + Save */}
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

          {/* Health status */}
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

      {/* ===== App Info ===== */}
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

export const Route = createFileRoute("/dashboard/settings")({
  component: SettingsPage,
});
