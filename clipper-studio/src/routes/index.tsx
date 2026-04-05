import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { getAppInfo } from "@/services/workspace";

/**
 * Root index route: checks workspace status and redirects accordingly.
 * Uses component-level redirect instead of beforeLoad to ensure
 * Tauri IPC bridge is fully ready.
 */
function IndexPage() {
  const navigate = useNavigate();
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    const check = async () => {
      try {
        const info = await getAppInfo();
        if (!info.has_workspaces) {
          navigate({ to: "/welcome", replace: true });
        } else {
          navigate({ to: "/dashboard", replace: true });
        }
      } catch {
        // Fallback: go to dashboard if IPC fails
        navigate({ to: "/dashboard", replace: true });
      } finally {
        setChecking(false);
      }
    };
    check();
  }, [navigate]);

  if (checking) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="text-muted-foreground">加载中...</div>
      </div>
    );
  }

  return null;
}

export const Route = createFileRoute("/")({
  component: IndexPage,
});
