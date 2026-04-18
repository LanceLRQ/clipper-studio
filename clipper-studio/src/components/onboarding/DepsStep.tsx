import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ArrowRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  listDeps,
  installDep,
  uninstallDep,
  revealDepDir,
  setDepCustomPath,
  setDepsProxy,
  getDepsProxy,
} from "@/services/deps";
import type { DependencyStatus, InstallProgress } from "@/types/deps";

export interface DepsSectionContext {
  deps: DependencyStatus[];
  anyInstalledOrAvailable: boolean;
  anyInstalling: boolean;
}

export interface DepsSectionProps {
  /** 顶部提示文字，默认展示设置页文案；引导流程可自定义 */
  intro?: ReactNode;
  /** 底部自定义区域（例如引导流程的"跳过/下一步"按钮） */
  footer?: (ctx: DepsSectionContext) => ReactNode;
}

export function DepsSection({ intro, footer }: DepsSectionProps) {
  const [deps, setDeps] = useState<DependencyStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [customPathEditing, setCustomPathEditing] = useState<string | null>(
    null
  );
  const [customPathValue, setCustomPathValue] = useState("");
  const [proxyUrl, setProxyUrl] = useState("");
  const [proxyLoaded, setProxyLoaded] = useState(false);
  const [proxySaving, setProxySaving] = useState(false);

  useEffect(() => {
    getDepsProxy().then((url) => {
      setProxyUrl(url);
      setProxyLoaded(true);
    });
  }, []);

  const handleProxySave = async () => {
    setProxySaving(true);
    try {
      await setDepsProxy(proxyUrl.trim());
    } catch (e) {
      alert("保存代理设置失败: " + String(e));
    } finally {
      setProxySaving(false);
    }
  };

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

  useEffect(() => {
    let cancelled = false;
    let unlistenProgress: UnlistenFn | undefined;
    let unlistenComplete: UnlistenFn | undefined;
    let unlistenError: UnlistenFn | undefined;

    listen<InstallProgress>("dep:install-progress", (event) => {
      setProgress(event.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenProgress = fn;
    });

    listen<{ dep_id: string; version: string | null }>(
      "dep:install-complete",
      () => {
        setInstallingId(null);
        setProgress(null);
        loadDeps();
      }
    ).then((fn) => {
      if (cancelled) fn();
      else unlistenComplete = fn;
    });

    listen<{ dep_id: string; error: string }>(
      "dep:install-error",
      () => {
        setInstallingId(null);
        setProgress(null);
        loadDeps();
      }
    ).then((fn) => {
      if (cancelled) fn();
      else unlistenError = fn;
    });

    return () => {
      cancelled = true;
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

  const handlePickCustomPath = async (_depId: string) => {
    const selected = await open({
      directory: false,
      multiple: false,
      title: "选择可执行文件",
    });
    if (selected) {
      setCustomPathValue(selected as string);
    }
  };

  const statusLabel = (
    dep: DependencyStatus
  ): { text: string; className: string } => {
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
        if (dep.system_available) {
          return { text: "系统已安装", className: "text-green-600" };
        }
        return { text: "未安装", className: "text-muted-foreground" };
    }
  };

  const ctx: DepsSectionContext = useMemo(
    () => ({
      deps,
      anyInstalledOrAvailable: deps.some(
        (d) => d.status === "installed" || d.system_available
      ),
      anyInstalling: installingId !== null,
    }),
    [deps, installingId]
  );

  if (loading) {
    return (
      <div className="text-muted-foreground text-sm p-4">加载依赖信息中...</div>
    );
  }

  return (
    <div className="space-y-4">
      {intro ?? (
        <p className="text-sm text-muted-foreground">
          以下工具是 ClipperStudio 运行所需的第三方依赖。点击"安装"自动下载，或手动指定已有安装路径。
        </p>
      )}

      {proxyLoaded && (
        <section className="rounded-lg border p-4 space-y-2">
          <h3 className="font-medium text-sm">下载代理</h3>
          <p className="text-xs text-muted-foreground">
            无法访问 GitHub 下载？设置 HTTP 代理地址后重试。
          </p>
          <div className="flex gap-2 items-center">
            <Input
              value={proxyUrl}
              onChange={(e) => setProxyUrl(e.target.value)}
              placeholder="http://127.0.0.1:7890"
              className="text-sm h-8 font-mono flex-1"
            />
            <Button
              size="sm"
              onClick={handleProxySave}
              disabled={proxySaving}
            >
              {proxySaving ? "保存中..." : "保存"}
            </Button>
          </div>
        </section>
      )}

      {deps.map((dep) => {
        const isInstalling = installingId === dep.id;
        const currentProgress =
          isInstalling && progress && progress.dep_id === dep.id
            ? progress
            : null;
        const sl = statusLabel(dep);

        return (
          <section key={dep.id} className="rounded-lg border p-5 space-y-3">
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

            <p className="text-sm text-muted-foreground">{dep.description}</p>

            {dep.status === "installed" && dep.installed_path && (
              <p className="text-xs text-muted-foreground font-mono truncate">
                路径: {dep.installed_path}
              </p>
            )}

            {dep.status !== "installed" &&
              dep.system_available &&
              dep.system_path && (
                <p className="text-xs text-green-600 font-mono truncate">
                  系统路径: {dep.system_path}
                </p>
              )}

            {dep.custom_path && (
              <p className="text-xs text-blue-600 font-mono truncate">
                自定义路径: {dep.custom_path}
              </p>
            )}

            {dep.status === "error" && dep.error_message && (
              <p className="text-sm text-red-500">{dep.error_message}</p>
            )}

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
                      width: `${Math.max(
                        2,
                        (() => {
                          const p = currentProgress.progress;
                          if (currentProgress.phase === "downloading")
                            return p * 50;
                          if (currentProgress.phase === "extracting")
                            return 50 + p * 40;
                          return 90 + p * 10;
                        })()
                      )}%`,
                    }}
                  />
                </div>
                {currentProgress.phase === "downloading" &&
                  dep.manual_download_url && (
                    <button
                      className="text-xs text-muted-foreground hover:text-primary transition-colors cursor-pointer bg-transparent border-none p-0 inline-flex items-center gap-0.5"
                      onClick={() => shellOpen(dep.manual_download_url!)}
                    >
                      下载缓慢？尝试手动下载
                      <ArrowRight className="h-3 w-3" />
                    </button>
                  )}
              </div>
            )}

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
                <>
                  {dep.auto_install_available && (
                    <Button
                      size="sm"
                      onClick={() => handleInstall(dep.id)}
                      disabled={isInstalling}
                    >
                      {isInstalling ? "安装中..." : "安装"}
                    </Button>
                  )}
                  {dep.manual_download_url && (
                    <button
                      className="text-xs text-muted-foreground hover:text-primary transition-colors cursor-pointer bg-transparent border-none p-0 inline-flex items-center gap-0.5"
                      onClick={() => shellOpen(dep.manual_download_url!)}
                    >
                      {dep.auto_install_available
                        ? "无法下载？尝试手动下载"
                        : "手动下载"}
                      <ArrowRight className="h-3 w-3" />
                    </button>
                  )}
                </>
              ) : null}

              {!isInstalling && customPathEditing !== dep.id && (
                <Button
                  variant="outline"
                  size="sm"
                  className="ml-auto"
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

      {footer?.(ctx)}
    </div>
  );
}
