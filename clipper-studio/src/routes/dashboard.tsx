import { createFileRoute, Outlet, Link, useMatches, useNavigate } from "@tanstack/react-router";
import { useEffect, useState, useCallback } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { WorkspaceSwitcher } from "@/components/workspace/workspace-switcher";
import { getAppInfo } from "@/services/workspace";
import { getSettings } from "@/services/settings";
import { getASRServiceStatus, type ASRServiceStatusInfo } from "@/services/asr";
import { useWorkspaceStore } from "@/stores/workspace";
import { useASRQueueStore, useASRActiveCount, useASRRunningTask } from "@/stores/asr-queue";
import { useASRHealth } from "@/stores/asr-health";
import bannerImg from "@/assets/banner.png";
import bannerDarkImg from "@/assets/banner-dark.png";
import { listPlugins } from "@/services/plugin";
import type { PluginInfo } from "@/services/plugin";
import { useThemeStore } from "@/stores/theme";
import { Sun, Moon, Monitor, Search, AlertTriangle, RefreshCw, Mic, ChevronDown, ChevronRight } from "lucide-react";
import { GlobalSearchDialog } from "@/components/search/global-search-dialog";

// ===== Types =====

interface NavItem {
  to: string;
  label: string;
  exact?: boolean;
  /** If present, this item has a collapsible sub-menu */
  children?: NavChildItem[];
}

interface NavChildItem {
  to: string;
  label: string;
  /** Route params for dynamic routes */
  params?: Record<string, string>;
}

// ===== Static nav items =====

const staticNavItems: NavItem[] = [
  { to: "/dashboard", label: "首页", exact: true },
  { to: "/dashboard/videos", label: "视频列表" },
  { to: "/dashboard/tasks", label: "任务中心" },
  { to: "/dashboard/asr", label: "语音识别" },
  {
    to: "/dashboard/plugins",
    label: "插件",
    children: [
      { to: "/dashboard/plugins", label: "插件管理" },
      // Dynamic plugin children are injected at runtime
    ],
  },
  { to: "/dashboard/settings", label: "设置" },
];

const THEME_CYCLE = ["light", "dark", "system"] as const;
const THEME_ICONS = { light: Sun, dark: Moon, system: Monitor };
const THEME_TIPS = { light: "浅色模式", dark: "深色模式", system: "跟随系统" };

function ThemeToggle() {
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);
  const Icon = THEME_ICONS[mode];

  const cycle = () => {
    const idx = THEME_CYCLE.indexOf(mode);
    setMode(THEME_CYCLE[(idx + 1) % THEME_CYCLE.length]);
  };

  return (
    <Button variant="ghost" size="sm" onClick={cycle} title={THEME_TIPS[mode]}>
      <Icon className="h-4 w-4" />
    </Button>
  );
}

function ASRStatusIndicator() {
  const navigate = useNavigate();
  const [localStatus, setLocalStatus] = useState<ASRServiceStatusInfo | null>(null);
  const [asrMode, setAsrMode] = useState<string>("local");
  // P5-PERF-25：通过共享 store 订阅远程健康状态，避免多处独立 30s 轮询
  const remoteHealthy = useASRHealth(asrMode === "remote");

  // ASR queue state
  const activeCount = useASRActiveCount();
  const runningTask = useASRRunningTask();

  const loadAsrMode = useCallback(() => {
    getSettings(["asr_mode"]).then((s) => {
      const mode = s.asr_mode || "local";
      setAsrMode(mode);
    }).catch(console.error);
  }, []);

  useEffect(() => {
    loadAsrMode();
    // Load local service status
    getASRServiceStatus().then(setLocalStatus).catch(console.error);

    // Refresh when ASR settings are saved
    const handler = () => loadAsrMode();
    window.addEventListener("asr-settings-changed", handler);
    return () => window.removeEventListener("asr-settings-changed", handler);
  }, [loadAsrMode]);

  // Listen for real-time local service status events
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: UnlistenFn | undefined;
    listen<ASRServiceStatusInfo>("asr-service-status", (event) => {
      setLocalStatus(event.payload);
    }).then((fn) => { if (cancelled) { fn(); } else { unlistenFn = fn; } });
    return () => { cancelled = true; unlistenFn?.(); };
  }, []);

  // Determine display state based on mode
  let dotColor: string;
  let tip: string;
  let colorClass: string;

  if (asrMode === "disabled") {
    dotColor = "bg-gray-400";
    tip = "ASR 已禁用，点击前往设置";
    colorClass = "text-muted-foreground";
  } else if (asrMode === "remote") {
    if (remoteHealthy === null) {
      dotColor = "bg-yellow-500 animate-pulse";
      tip = "远程 ASR 服务检测中...";
      colorClass = "text-yellow-500";
    } else if (remoteHealthy) {
      dotColor = "bg-green-500";
      tip = "远程 ASR 服务已连接";
      colorClass = "text-green-500";
    } else {
      dotColor = "bg-red-500";
      tip = "远程 ASR 服务无法连接";
      colorClass = "text-red-500";
    }
  } else {
    // local mode
    const st = localStatus?.status ?? "stopped";
    if (st === "running") {
      dotColor = "bg-green-500";
      tip = "本地 ASR 服务运行中";
      colorClass = "text-green-500";
    } else if (st === "starting") {
      dotColor = "bg-yellow-500 animate-pulse";
      tip = "本地 ASR 服务启动中...";
      colorClass = "text-yellow-500";
    } else if (st === "error") {
      dotColor = "bg-red-500";
      tip = "本地 ASR 服务异常";
      colorClass = "text-red-500";
    } else {
      dotColor = "bg-gray-400";
      tip = "本地 ASR 服务未启动";
      colorClass = "text-muted-foreground";
    }
  }

  // Enhance tooltip with queue info
  if (activeCount > 0) {
    tip += ` | ${activeCount} 个识别任务`;
    if (runningTask) {
      tip += ` (${Math.round(runningTask.progress * 100)}%)`;
    }
  }

  // Mode label for display
  const modeLabel =
    asrMode === "disabled" ? "禁用"
    : asrMode === "remote" ? "远程"
    : "本地";

  return (
    <Button
      variant="ghost"
      size="sm"
      title={tip}
      className={`gap-1.5 ${colorClass}`}
      onClick={() => navigate({ to: "/dashboard/asr" })}
    >
      <Mic className="h-4 w-4" />
      <span className="hidden sm:inline text-xs">{modeLabel}</span>
      {activeCount > 0 && (
        <>
          <span className="inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-primary text-primary-foreground px-1 text-[10px] font-medium">
            {activeCount}
          </span>
          {runningTask && (
            <span className="hidden sm:inline text-[10px] tabular-nums">
              {Math.round(runningTask.progress * 100)}%
            </span>
          )}
        </>
      )}
      <span className={`inline-block h-1.5 w-1.5 rounded-full ${dotColor}`} />
    </Button>
  );
}

function DashboardLayout() {
  const navigate = useNavigate();
  const wsInitialized = useWorkspaceStore((s) => s.initialized);
  const wsNoWorkspaces = useWorkspaceStore((s) => s.noWorkspaces);
  const wsActiveId = useWorkspaceStore((s) => s.activeId);
  const wsPathAccessible = useWorkspaceStore((s) => s.pathAccessible);
  const wsRecheckPath = useWorkspaceStore((s) => s.recheckPath);
  const initializeASRQueue = useASRQueueStore((s) => s.initialize);

  const [version, setVersion] = useState("");
  const [enabledPlugins, setEnabledPlugins] = useState<PluginInfo[]>([]);
  const [expandedMenus, setExpandedMenus] = useState<Set<string>>(new Set());
  const [searchOpen, setSearchOpen] = useState(false);
  const matches = useMatches();

  // Check if current route is under a given prefix
  const isUnderPath = useCallback(
    (prefix: string) => matches.some((m) => m.fullPath.startsWith(prefix)),
    [matches],
  );

  useEffect(() => {
    getAppInfo()
      .then((info) => setVersion(info.version))
      .catch(console.error);
  }, []);

  // Initialize ASR task queue store
  useEffect(() => {
    initializeASRQueue();
  }, [initializeASRQueue]);

  // Cmd+K / Ctrl+K keyboard shortcut for search
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setSearchOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Load enabled plugins for sidebar sub-items
  const refreshPlugins = useCallback(async () => {
    try {
      const plugins = await listPlugins();
      setEnabledPlugins(plugins.filter((p) => p.enabled));
    } catch {
      // Silently ignore - sidebar still works without plugin sub-items
    }
  }, []);

  useEffect(() => {
    refreshPlugins();
    // Refresh when plugins are enabled/disabled
    const handler = () => refreshPlugins();
    window.addEventListener("plugins-changed", handler);
    return () => window.removeEventListener("plugins-changed", handler);
  }, [refreshPlugins]);

  // Auto-expand menu when route matches
  useEffect(() => {
    if (isUnderPath("/dashboard/plugins")) {
      setExpandedMenus((prev) => {
        const next = new Set(prev);
        next.add("/dashboard/plugins");
        return next;
      });
    }
  }, [isUnderPath]);

  // Build nav items with dynamic plugin children
  const navItems: NavItem[] = staticNavItems.map((item) => {
    if (item.to === "/dashboard/plugins" && item.children) {
      const dynamicChildren: NavChildItem[] = enabledPlugins.map((p) => ({
        to: "/dashboard/plugins/$pluginId",
        label: p.name,
        params: { pluginId: p.id },
      }));
      return {
        ...item,
        children: [item.children[0], ...dynamicChildren],
      };
    }
    return item;
  });

  // Workspace guard: redirect to welcome if no workspaces
  useEffect(() => {
    if (wsInitialized && wsNoWorkspaces) {
      navigate({ to: "/welcome", replace: true });
    }
  }, [wsInitialized, wsNoWorkspaces, navigate]);

  const toggleMenu = (to: string) => {
    setExpandedMenus((prev) => {
      const next = new Set(prev);
      if (next.has(to)) {
        next.delete(to);
      } else {
        next.add(to);
      }
      return next;
    });
  };

  // Wait for workspace initialization
  if (!wsInitialized || wsNoWorkspaces || wsActiveId == null) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-muted-foreground text-sm">加载中...</div>
      </div>
    );
  }

  return (
    <div className="flex h-screen flex-col">
      {/* Header */}
      <header className="flex h-14 shrink-0 items-center justify-between border-b px-6">
        <div className="flex items-center gap-2">
          <img src={bannerImg} alt="ClipperStudio" className="h-8 dark:hidden" />
          <img src={bannerDarkImg} alt="ClipperStudio" className="h-8 hidden dark:block" />
          {version && (
            <span className="text-xs text-muted-foreground self-end pb-1.5">
              v{version}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setSearchOpen(true)}
            title="搜索字幕 (⌘K)"
            className="gap-1.5 text-muted-foreground"
          >
            <Search className="h-4 w-4" />
            <span className="hidden sm:inline text-xs">搜索</span>
            <kbd className="hidden sm:inline-flex h-5 items-center rounded border bg-muted px-1.5 text-[10px] font-medium">
              ⌘K
            </kbd>
          </Button>
          <ASRStatusIndicator />
          <ThemeToggle />
          <WorkspaceSwitcher />
        </div>
      </header>

      {/* Workspace path inaccessible warning */}
      {!wsPathAccessible && (
        <div className="shrink-0 flex items-center gap-2 bg-yellow-50 dark:bg-yellow-950/30 border-b border-yellow-200 dark:border-yellow-800 px-6 py-2">
          <AlertTriangle className="h-4 w-4 text-yellow-600 shrink-0" />
          <span className="text-sm text-yellow-800 dark:text-yellow-200">
            当前工作区目录不可访问，可能是网络存储已断开。扫描、播放、切片等功能暂时不可用。
          </span>
          <Button
            variant="outline"
            size="sm"
            className="ml-auto shrink-0 h-7 text-xs gap-1"
            onClick={wsRecheckPath}
          >
            <RefreshCw className="h-3 w-3" />
            重新检测
          </Button>
        </div>
      )}

      {/* Main Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <aside className="w-56 shrink-0 border-r p-4 overflow-y-auto">
          <nav className="space-y-1">
            {navItems.map((item) => {
              const hasChildren = item.children && item.children.length > 0;
              const isExpanded = expandedMenus.has(item.to);
              const isParentActive = isUnderPath(item.to);

              if (hasChildren) {
                return (
                  <div key={item.to}>
                    {/* Parent item - click to toggle */}
                    <Button
                      variant={isParentActive && !isExpanded ? "secondary" : "ghost"}
                      className="w-full justify-between"
                      onClick={() => toggleMenu(item.to)}
                    >
                      <span>{item.label}</span>
                      {isExpanded ? (
                        <ChevronDown className="h-4 w-4 text-muted-foreground" />
                      ) : (
                        <ChevronRight className="h-4 w-4 text-muted-foreground" />
                      )}
                    </Button>

                    {/* Children */}
                    {isExpanded && (
                      <div className="ml-3 mt-1 space-y-0.5 border-l pl-2">
                        {item.children!.map((child) => (
                          <Link
                            key={child.params?.pluginId ?? child.to}
                            to={child.to}
                            params={child.params ?? {}}
                            activeOptions={{ exact: true }}
                          >
                            {({ isActive }) => (
                              <Button
                                variant={isActive ? "secondary" : "ghost"}
                                size="sm"
                                className="w-full justify-start text-sm"
                              >
                                {child.label}
                              </Button>
                            )}
                          </Link>
                        ))}
                      </div>
                    )}
                  </div>
                );
              }

              // Regular item (no children)
              return (
                <Link
                  key={item.to}
                  to={item.to}
                  activeOptions={{ exact: item.exact }}
                >
                  {({ isActive }) => (
                    <Button
                      variant={isActive ? "secondary" : "ghost"}
                      className="w-full justify-start"
                    >
                      {item.label}
                    </Button>
                  )}
                </Link>
              );
            })}
          </nav>
        </aside>

        {/* Content Area */}
        <main className="flex-1 overflow-auto">
          <Outlet />
        </main>
      </div>

      {/* Global Search Dialog */}
      <GlobalSearchDialog open={searchOpen} onOpenChange={setSearchOpen} />
    </div>
  );
}

export const Route = createFileRoute("/dashboard")({
  component: DashboardLayout,
});
