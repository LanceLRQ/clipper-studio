import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { getAppInfo } from "@/services/workspace";
import { getSettings } from "@/services/settings";
import { listDeps } from "@/services/deps";

/**
 * Root index route: checks onboarding progress and redirects accordingly.
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
          return;
        }

        const settings = await getSettings([
          "onboarding_completed",
          "onboarding_deps_skipped",
        ]);

        if (settings.onboarding_completed === "true") {
          navigate({ to: "/dashboard", replace: true });
          return;
        }

        // Has workspace but onboarding not completed — resume to next step.
        let resumeStep: "deps" | "asr" = "deps";
        if (settings.onboarding_deps_skipped === "true") {
          resumeStep = "asr";
        } else {
          try {
            const deps = await listDeps();
            const anyHandled = deps.some(
              (d) => d.status === "installed" || d.system_available
            );
            if (anyHandled) resumeStep = "asr";
          } catch {
            /* ignore */
          }
        }
        navigate({ to: "/welcome", search: { step: resumeStep }, replace: true });
      } catch {
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
