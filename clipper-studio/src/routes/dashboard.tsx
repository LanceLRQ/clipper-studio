import { createFileRoute, Outlet, Link, useMatches } from "@tanstack/react-router";
import { useEffect, useState, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { WorkspaceSwitcher } from "@/components/workspace/workspace-switcher";
import { getAppInfo } from "@/services/workspace";
import bannerImg from "@/assets/banner.png";
import { listPlugins } from "@/services/plugin";
import type { PluginInfo } from "@/services/plugin";

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
  { to: "/dashboard/workspaces", label: "工作区" },
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

function DashboardLayout() {
  const [version, setVersion] = useState("");
  const [enabledPlugins, setEnabledPlugins] = useState<PluginInfo[]>([]);
  const [expandedMenus, setExpandedMenus] = useState<Set<string>>(new Set());
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

  return (
    <div className="flex h-screen flex-col">
      {/* Header */}
      <header className="flex h-14 shrink-0 items-center justify-between border-b px-6">
        <div className="flex items-center gap-2">
          <img src={bannerImg} alt="ClipperStudio" className="h-8" />
          {version && (
            <span className="text-xs text-muted-foreground self-end pb-1.5">
              v{version}
            </span>
          )}
        </div>
        <WorkspaceSwitcher />
      </header>

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
                      <span className="text-xs text-muted-foreground">
                        {isExpanded ? "▾" : "▸"}
                      </span>
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
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

export const Route = createFileRoute("/dashboard")({
  component: DashboardLayout,
});
