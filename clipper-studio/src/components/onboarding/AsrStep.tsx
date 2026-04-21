import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { Check, ExternalLink, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import { setSetting } from "@/services/settings";
import {
  checkDockerCapability,
  checkDockerImagePulled,
  openDockerPullTerminal,
  validateASRPath,
  type ASRPathValidation,
  type DockerCapability,
} from "@/services/asr";

type AsrMode = "local-native" | "local-docker" | "remote";

interface AsrStepProps {
  onSaved: () => void;
}

const BAIDU_PAN =
  "https://pan.baidu.com/s/1ahqW1mxIoNJTG2k6b4PkkA?pwd=6cth";
const GITHUB_REPO = "https://github.com/LanceLRQ/qwen3-asr-service";
const DOCKER_IMAGE_PREFIX = "lancelrq/qwen3-asr-service:";

interface DockerImageOption {
  value: string;
  label: string;
}

function getDockerImageOptions(
  cap: DockerCapability | null
): DockerImageOption[] {
  if (!cap) return [];
  const { host_platform, host_arch } = cap;
  if (host_arch === "arm64") {
    return [
      {
        value: DOCKER_IMAGE_PREFIX + "latest-arm64",
        label: "ARM64 (OpenVINO FP32，推荐)",
      },
      {
        value: DOCKER_IMAGE_PREFIX + "latest-cpu",
        label: "CPU (通过 linux/amd64 模拟)",
      },
    ];
  }
  if (host_platform === "windows" || host_platform === "linux") {
    return [
      {
        value: DOCKER_IMAGE_PREFIX + "latest",
        label: "CUDA (需 NVIDIA GPU + nvidia-docker)",
      },
      {
        value: DOCKER_IMAGE_PREFIX + "latest-cpu",
        label: "CPU (OpenVINO INT8)",
      },
    ];
  }
  return [
    {
      value: DOCKER_IMAGE_PREFIX + "latest-cpu",
      label: "CPU (OpenVINO INT8)",
    },
  ];
}

export function AsrStep({ onSaved }: AsrStepProps) {
  const [mode, setMode] = useState<AsrMode | null>(null);
  const [saving, setSaving] = useState(false);

  // local-native
  const [localPath, setLocalPath] = useState("");
  const [pathValidation, setPathValidation] =
    useState<ASRPathValidation | null>(null);
  const [validating, setValidating] = useState(false);

  // local-docker
  const [dockerCap, setDockerCap] = useState<DockerCapability | null>(null);
  const [dockerImage, setDockerImage] = useState<string>("");
  const [imagePulled, setImagePulled] = useState<boolean | null>(null);
  const [checkingImage, setCheckingImage] = useState(false);

  // remote
  const [remoteUrl, setRemoteUrl] = useState("");
  const [remoteKey, setRemoteKey] = useState("");

  useEffect(() => {
    checkDockerCapability().then(setDockerCap).catch(() => setDockerCap(null));
  }, []);

  const dockerOptions = useMemo(() => getDockerImageOptions(dockerCap), [
    dockerCap,
  ]);

  useEffect(() => {
    if (mode === "local-docker" && dockerOptions.length > 0 && !dockerImage) {
      setDockerImage(dockerOptions[0].value);
    }
  }, [mode, dockerOptions, dockerImage]);

  useEffect(() => {
    if (mode !== "local-docker" || !dockerImage) {
      setImagePulled(null);
      return;
    }
    let cancelled = false;
    setCheckingImage(true);
    checkDockerImagePulled(dockerImage)
      .then((ok) => {
        if (!cancelled) setImagePulled(ok);
      })
      .catch(() => {
        if (!cancelled) setImagePulled(false);
      })
      .finally(() => {
        if (!cancelled) setCheckingImage(false);
      });
    return () => {
      cancelled = true;
    };
  }, [mode, dockerImage]);

  const handlePickAsrDir = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;
    const p = selected as string;
    setLocalPath(p);
    setValidating(true);
    try {
      setPathValidation(await validateASRPath(p));
    } catch {
      setPathValidation(null);
    } finally {
      setValidating(false);
    }
  };

  const canSubmit = (): boolean => {
    if (mode === "local-native")
      return !!pathValidation && pathValidation.valid;
    if (mode === "local-docker")
      return !!dockerCap?.installed && imagePulled === true;
    if (mode === "remote") return remoteUrl.trim().length > 0;
    return false;
  };

  const handleSave = async () => {
    if (!mode) return;
    setSaving(true);
    try {
      if (mode === "local-native") {
        await setSetting("asr_mode", "local");
        await setSetting("asr_launch_mode", "native");
        await setSetting("asr_local_path", localPath);
      } else if (mode === "local-docker") {
        await setSetting("asr_mode", "local");
        await setSetting("asr_launch_mode", "docker");
        await setSetting("asr_docker_image", dockerImage);
      } else if (mode === "remote") {
        await setSetting("asr_mode", "remote");
        await setSetting("asr_url", remoteUrl.trim());
        if (remoteKey.trim()) {
          await setSetting("asr_api_key", remoteKey.trim());
        }
      }
      onSaved();
    } catch (e) {
      alert("保存 ASR 设置失败: " + String(e));
    } finally {
      setSaving(false);
    }
  };

  const ModeCard = ({
    value,
    title,
    desc,
    badge,
  }: {
    value: AsrMode;
    title: string;
    desc: string;
    badge?: string;
  }) => (
    <button
      type="button"
      onClick={() => setMode(value)}
      className={cn(
        "rounded-lg border-2 p-4 text-left transition-colors",
        mode === value
          ? "border-primary bg-accent"
          : "border-dashed hover:border-primary hover:bg-accent"
      )}
    >
      <div className="flex items-center gap-2">
        <div className="text-base font-medium">{title}</div>
        {badge && (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-primary/10 text-primary font-medium">
            {badge}
          </span>
        )}
      </div>
      <div className="text-xs text-muted-foreground mt-1">{desc}</div>
    </button>
  );

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg font-medium">配置语音识别（ASR）</h2>
        <p className="text-sm text-muted-foreground mt-1">
          用于从视频中自动识别语音生成字幕。你可以选择本地整合包、Docker 容器或远程
          API；现在也可以稍后再配置。
        </p>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <ModeCard
          value="local-native"
          title="整合包（推荐）"
          desc="下载 qwen3-asr-service 整合包后关联文件夹"
          badge="国内可用"
        />
        <ModeCard
          value="local-docker"
          title="Docker 镜像"
          desc="通过 Docker 拉取官方镜像运行，适合已有 Docker 环境"
        />
        <ModeCard
          value="remote"
          title="远程 API"
          desc="连接已部署的服务（兼容 OpenAI 格式）"
        />
      </div>

      {mode === "local-native" && (
        <div className="rounded-lg border p-4 space-y-3">
          <div className="text-sm space-y-1">
            <div>
              第一次使用请先下载 qwen3-asr-service 整合包，解压后选择其目录
              （选择根目录或 <code className="text-xs">asr-service</code> 子目录均可）：
            </div>
            <div className="flex flex-wrap gap-2 pt-1">
              <Button
                variant="outline"
                size="sm"
                onClick={() => shellOpen(BAIDU_PAN)}
              >
                <ExternalLink className="h-3.5 w-3.5 mr-1" />
                百度网盘（国内）
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => shellOpen(GITHUB_REPO)}
              >
                <ExternalLink className="h-3.5 w-3.5 mr-1" />
                GitHub 仓库
              </Button>
            </div>
          </div>

          <div className="space-y-2">
            <Label>qwen3-asr-service 目录</Label>
            <div className="flex gap-2">
              <Input
                value={localPath}
                readOnly
                placeholder="选择 qwen3-asr-service 解压目录"
                className="text-sm h-8 font-mono flex-1"
              />
              <Button variant="outline" size="sm" onClick={handlePickAsrDir}>
                浏览...
              </Button>
            </div>
            {validating && (
              <div className="text-xs text-muted-foreground flex items-center gap-1">
                <Loader2 className="h-3 w-3 animate-spin" /> 正在校验...
              </div>
            )}
            {pathValidation && (
              <div
                className={cn(
                  "text-xs",
                  pathValidation.valid ? "text-green-600" : "text-red-500"
                )}
              >
                {pathValidation.valid ? (
                  <span className="inline-flex items-center gap-1">
                    <Check className="h-3.5 w-3.5" />
                    目录检测通过
                  </span>
                ) : (
                  pathValidation.setup_hint || "目录不完整，请先运行 setup 脚本"
                )}
              </div>
            )}
          </div>
        </div>
      )}

      {mode === "local-docker" && (
        <div className="rounded-lg border p-4 space-y-3">
          {!dockerCap?.installed ? (
            <div className="text-sm text-red-500">
              未检测到 Docker，请先安装 Docker Desktop 后重试。
              {dockerCap?.hint && (
                <div className="text-xs text-muted-foreground mt-1">
                  {dockerCap.hint}
                </div>
              )}
            </div>
          ) : (
            <>
              <div className="space-y-2">
                <Label>选择镜像</Label>
                <select
                  value={dockerImage}
                  onChange={(e) => setDockerImage(e.target.value)}
                  className="h-8 w-full rounded-md border border-input bg-background px-3 py-1 text-sm"
                >
                  {dockerOptions.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </div>

              <div className="flex items-center gap-2 text-sm">
                {checkingImage ? (
                  <span className="text-muted-foreground inline-flex items-center gap-1">
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    正在检测镜像...
                  </span>
                ) : imagePulled ? (
                  <span className="text-green-600 inline-flex items-center gap-1">
                    <Check className="h-3.5 w-3.5" />
                    镜像已就绪
                  </span>
                ) : (
                  <>
                    <span className="text-muted-foreground">
                      镜像尚未拉取。
                    </span>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => openDockerPullTerminal(dockerImage)}
                    >
                      打开终端 Pull 镜像
                    </Button>
                  </>
                )}
              </div>
            </>
          )}
        </div>
      )}

      {mode === "remote" && (
        <div className="rounded-lg border p-4 space-y-3">
          <div className="space-y-2">
            <Label>服务地址</Label>
            <Input
              value={remoteUrl}
              onChange={(e) => setRemoteUrl(e.target.value)}
              placeholder="http://localhost:9000"
              className="text-sm h-8 font-mono"
            />
          </div>
          <div className="space-y-2">
            <Label>API Key（可选）</Label>
            <Input
              value={remoteKey}
              onChange={(e) => setRemoteKey(e.target.value)}
              placeholder="Bearer token，留空表示无鉴权"
              className="text-sm h-8 font-mono"
              type="password"
            />
          </div>
        </div>
      )}

      {mode && (
        <div className="flex justify-end">
          <Button onClick={handleSave} disabled={!canSubmit() || saving}>
            {saving ? "保存中..." : "保存并继续"}
          </Button>
        </div>
      )}
    </div>
  );
}
