import { createFileRoute, Outlet, Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { WorkspaceSwitcher } from "@/components/workspace/workspace-switcher";
import { getAppInfo } from "@/services/workspace";

function DashboardLayout() {
  const [version, setVersion] = useState("");

  useEffect(() => {
    getAppInfo()
      .then((info) => setVersion(info.version))
      .catch(console.error);
  }, []);

  const navItems = [
    { to: "/dashboard" as const, label: "首页", exact: true },
    { to: "/dashboard/videos" as const, label: "视频列表" },
    { to: "/dashboard/tasks" as const, label: "任务中心" },
    { to: "/dashboard/workspaces" as const, label: "工作区" },
    { to: "/dashboard/settings" as const, label: "设置" },
  ];

  return (
    <div className="flex h-screen flex-col">
      {/* Header */}
      <header className="flex h-14 shrink-0 items-center justify-between border-b px-6">
        <div className="flex items-center">
          <h1 className="text-lg font-semibold">ClipperStudio</h1>
          {version && (
            <span className="ml-2 text-xs text-muted-foreground">
              v{version}
            </span>
          )}
        </div>
        <WorkspaceSwitcher />
      </header>

      {/* Main Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <aside className="w-56 shrink-0 border-r p-4">
          <nav className="space-y-1">
            {navItems.map((item) => (
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
            ))}
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
